//! Cross-cutting compatibility contracts ported from ddd4j core tests.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ddd4r_core::command::{CommandExecutor, DefaultCommandBus};
use ddd4r_core::context::{ContextScope, Contexts, Registry};
use ddd4r_core::domain::{AggregateRoot, DomainModel, Entity};
use ddd4r_core::event::{
    DOMAIN_EVENT_PUBLISHER_KEY, DomainEvent, DomainEventExt, DomainEventPublisher, EventEnvelope,
};
use ddd4r_core::query::{Page, Query};
use ddd4r_core::repository::{AggregateRootExt, Repository, RepositoryRegistry};
use ddd4r_core::{DddError, DddResult};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

static REPOSITORY_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Order {
    id: String,
    name: String,
    version: u64,
    #[serde(skip)]
    events: Vec<EventEnvelope>,
}

impl Order {
    fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            version: 0,
            events: Vec::new(),
        }
    }
}

impl DomainModel for Order {
    type Id = String;

    fn id(&self) -> &Self::Id {
        &self.id
    }
}

impl Entity for Order {}

impl AggregateRoot for Order {
    fn version(&self) -> u64 {
        self.version
    }

    fn set_version(&mut self, version: u64) {
        self.version = version;
    }

    fn recorded_events(&self) -> &[EventEnvelope] {
        &self.events
    }

    fn recorded_events_mut(&mut self) -> &mut Vec<EventEnvelope> {
        &mut self.events
    }
}

#[derive(Default)]
struct InMemoryOrderRepository {
    values: Mutex<HashMap<String, Order>>,
    label: &'static str,
}

impl InMemoryOrderRepository {
    fn labelled(label: &'static str) -> Self {
        Self {
            values: Mutex::new(HashMap::new()),
            label,
        }
    }
}

impl Repository<Order> for InMemoryOrderRepository {
    fn find_by_id<'a>(&'a self, id: &'a String) -> BoxFuture<'a, DddResult<Option<Order>>> {
        Box::pin(async move { Ok(self.values.lock().unwrap().get(id).cloned()) })
    }

    fn save<'a>(&'a self, aggregate: &'a mut Order) -> BoxFuture<'a, DddResult<()>> {
        Box::pin(async move {
            aggregate.name = format!("{}:{}", self.label, aggregate.name);
            aggregate.set_version(aggregate.version().saturating_add(1));
            self.values
                .lock()
                .unwrap()
                .insert(aggregate.id.clone(), aggregate.clone());
            Ok(())
        })
    }

    fn delete_by_id<'a>(&'a self, id: &'a String) -> BoxFuture<'a, DddResult<()>> {
        Box::pin(async move {
            self.values.lock().unwrap().remove(id);
            Ok(())
        })
    }

    fn find_list<'a>(&'a self, _query: &'a Query<Order>) -> BoxFuture<'a, DddResult<Vec<Order>>> {
        Box::pin(async move { Ok(self.values.lock().unwrap().values().cloned().collect()) })
    }

    fn find_first<'a>(
        &'a self,
        _query: &'a Query<Order>,
    ) -> BoxFuture<'a, DddResult<Option<Order>>> {
        Box::pin(async move { Ok(self.values.lock().unwrap().values().next().cloned()) })
    }

    fn page<'a>(&'a self, query: &'a Query<Order>) -> BoxFuture<'a, DddResult<Page<Order>>> {
        Box::pin(async move {
            let records = self
                .values
                .lock()
                .unwrap()
                .values()
                .cloned()
                .collect::<Vec<_>>();
            let size = query.page.size.unwrap_or(records.len() as u64);
            Ok(Page {
                total: records.len() as u64,
                current: query.page.current,
                size,
                records,
            })
        })
    }
}

#[tokio::test]
async fn aggregate_facade_and_rich_query_use_registered_repository() {
    let _guard = REPOSITORY_TEST_LOCK.lock().await;
    let _ = RepositoryRegistry::unregister::<Order>();
    RepositoryRegistry::register::<Order>(Arc::new(InMemoryOrderRepository::labelled("global")))
        .unwrap();

    let mut order = Order::new("O-1", "created");
    order.save().await.unwrap();
    assert_eq!(order.version, 1);
    assert_eq!(order.name, "global:created");

    let query = Order::query().page(1, 10);
    assert_eq!(query.count().await.unwrap(), 1);
    assert_eq!(query.one().await.unwrap().unwrap().id, "O-1");
    assert_eq!(query.list_page().await.unwrap().total, 1);

    order.delete().await.unwrap();
    assert!(!query.exists().await.unwrap());
    RepositoryRegistry::unregister::<Order>().unwrap();
}

#[tokio::test]
async fn task_repository_overrides_global_without_leaking() {
    let _guard = REPOSITORY_TEST_LOCK.lock().await;
    let _ = RepositoryRegistry::unregister::<Order>();
    RepositoryRegistry::register::<Order>(Arc::new(InMemoryOrderRepository::labelled("global")))
        .unwrap();
    let local = Registry::new();
    RepositoryRegistry::register_in::<Order>(
        &local,
        Arc::new(InMemoryOrderRepository::labelled("task")),
    )
    .unwrap();

    let mut scoped = Order::new("O-2", "created");
    ContextScope::run(local, async {
        scoped.save().await.unwrap();
    })
    .await;
    assert_eq!(scoped.name, "task:created");

    let mut global = Order::new("O-3", "created");
    global.save().await.unwrap();
    assert_eq!(global.name, "global:created");
    RepositoryRegistry::unregister::<Order>().unwrap();
}

#[derive(Debug, Serialize)]
struct OrderCreated {
    order_id: String,
}

impl DomainEvent for OrderCreated {
    fn source(&self) -> String {
        self.order_id.clone()
    }

    fn aggregate_type(&self) -> std::borrow::Cow<'static, str> {
        "Order".into()
    }

    fn support_keys(&self) -> Vec<String> {
        vec!["created".to_owned()]
    }
}

#[derive(Default)]
struct CapturingPublisher {
    events: Mutex<Vec<EventEnvelope>>,
}

impl DomainEventPublisher for CapturingPublisher {
    fn publish<'a>(&'a self, event: &'a EventEnvelope) -> BoxFuture<'a, DddResult<Option<Value>>> {
        Box::pin(async move {
            self.events.lock().unwrap().push(event.clone());
            Ok(Some(Value::Bool(true)))
        })
    }
}

#[tokio::test]
async fn event_publish_uses_context_and_preserves_filter_metadata() {
    let registry = Registry::new();
    let publisher = Arc::new(CapturingPublisher::default());
    registry
        .register::<dyn DomainEventPublisher>(DOMAIN_EVENT_PUBLISHER_KEY, publisher.clone())
        .unwrap();

    let result = ContextScope::run(registry, async {
        OrderCreated {
            order_id: "O-4".to_owned(),
        }
        .publish()
        .await
        .unwrap()
    })
    .await;
    assert_eq!(result, Some(Value::Bool(true)));
    let events = publisher.events.lock().unwrap();
    assert_eq!(events[0].aggregate_id, "O-4");
    assert!(events[0].supports(["created"]));
}

#[derive(Debug)]
struct RenameOrder {
    value: String,
}

struct RenameOrderExecutor;

impl CommandExecutor<RenameOrder> for RenameOrderExecutor {
    type Output = String;

    fn execute<'a>(&'a self, command: &'a RenameOrder) -> BoxFuture<'a, DddResult<Self::Output>> {
        Box::pin(async move { Ok(command.value.to_uppercase()) })
    }
}

#[tokio::test]
async fn command_bus_rejects_duplicates_and_routes_by_type() {
    let bus = DefaultCommandBus::new();
    bus.register::<RenameOrder, _>(RenameOrderExecutor).unwrap();
    assert_eq!(
        bus.execute::<_, String>(&RenameOrder {
            value: "renamed".to_owned(),
        })
        .await
        .unwrap(),
        "RENAMED"
    );
    assert!(matches!(
        bus.register::<RenameOrder, _>(RenameOrderExecutor),
        Err(DddError::MultipleCommandExecutors { .. })
    ));
}

#[test]
fn missing_context_service_has_stable_error() {
    let Err(error) = Contexts::get_or_err::<dyn DomainEventPublisher>("missing") else {
        panic!("missing service unexpectedly resolved");
    };
    assert!(matches!(error, DddError::ServiceNotFound { .. }));
}
