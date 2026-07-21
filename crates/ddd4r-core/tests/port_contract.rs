//! Contracts for mapper, event-sourcing and runtime ports added during the ddd4j migration.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ddd4r_core::domain::{AggregateRoot, DomainModel, Entity};
use ddd4r_core::event::EventEnvelope;
use ddd4r_core::event_sourcing::EventSourcingRepository;
use ddd4r_core::mapper::DomainObjectMapper;
use ddd4r_core::runtime::RuntimeRegistry;
use ddd4r_core::{DddError, DddResult};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TestAggregate {
    id: String,
    value: String,
    version: u64,
    #[serde(skip)]
    events: Vec<EventEnvelope>,
}

impl DomainModel for TestAggregate {
    type Id = String;

    fn id(&self) -> &Self::Id {
        &self.id
    }
}

impl Entity for TestAggregate {}

impl AggregateRoot for TestAggregate {
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

#[derive(Debug, Clone, PartialEq)]
struct TestPo {
    id: String,
    value: String,
    version: u64,
}

struct TestMapper;

impl DomainObjectMapper<TestAggregate, TestPo> for TestMapper {
    fn to_model(&self, persistence_object: TestPo) -> DddResult<TestAggregate> {
        Ok(TestAggregate {
            id: persistence_object.id,
            value: persistence_object.value,
            version: persistence_object.version,
            events: Vec::new(),
        })
    }

    fn to_persistence_object(&self, model: TestAggregate) -> DddResult<TestPo> {
        Ok(TestPo {
            id: model.id,
            value: model.value,
            version: model.version,
        })
    }
}

#[derive(Default)]
struct InMemoryEventSourcingRepository {
    streams: Mutex<HashMap<String, Vec<TestAggregate>>>,
}

impl EventSourcingRepository<TestAggregate> for InMemoryEventSourcingRepository {
    fn read<'a>(
        &'a self,
        aggregate_id: &'a String,
    ) -> BoxFuture<'a, DddResult<Option<TestAggregate>>> {
        Box::pin(async move {
            Ok(self
                .streams
                .lock()
                .unwrap()
                .get(aggregate_id)
                .and_then(|stream| stream.last().cloned()))
        })
    }

    fn read_at<'a>(
        &'a self,
        aggregate_id: &'a String,
        version: u64,
    ) -> BoxFuture<'a, DddResult<Option<TestAggregate>>> {
        Box::pin(async move {
            Ok(self
                .streams
                .lock()
                .unwrap()
                .get(aggregate_id)
                .and_then(|stream| stream.iter().find(|item| item.version == version).cloned()))
        })
    }

    fn add<'a>(&'a self, aggregate: &'a mut TestAggregate) -> BoxFuture<'a, DddResult<()>> {
        Box::pin(async move {
            let mut streams = self.streams.lock().unwrap();
            if streams.contains_key(aggregate.id()) {
                return Err(DddError::OptimisticLockConflict {
                    aggregate: "TestAggregate",
                    expected: 0,
                    actual: aggregate.version(),
                });
            }
            aggregate.set_version(1);
            streams.insert(aggregate.id.clone(), vec![aggregate.clone()]);
            Ok(())
        })
    }

    fn update<'a>(&'a self, aggregate: &'a mut TestAggregate) -> BoxFuture<'a, DddResult<()>> {
        Box::pin(async move {
            let mut streams = self.streams.lock().unwrap();
            let stream = streams.get_mut(aggregate.id()).ok_or(DddError::Adapter {
                adapter: "event-sourcing-test",
                message: "stream not found".to_owned(),
            })?;
            let actual = stream.last().map_or(0, AggregateRoot::version);
            if actual != aggregate.version() {
                return Err(DddError::OptimisticLockConflict {
                    aggregate: "TestAggregate",
                    expected: aggregate.version(),
                    actual,
                });
            }
            aggregate.set_version(actual.saturating_add(1));
            stream.push(aggregate.clone());
            Ok(())
        })
    }
}

#[tokio::test]
async fn mapper_event_sourcing_and_runtime_registry_are_executable_contracts() {
    let mapper = TestMapper;
    let model = mapper
        .to_model(TestPo {
            id: "ES-1".to_owned(),
            value: "draft".to_owned(),
            version: 0,
        })
        .unwrap();
    assert_eq!(
        mapper.to_persistence_object(model.clone()).unwrap().id,
        "ES-1"
    );

    let repository = InMemoryEventSourcingRepository::default();
    let mut aggregate = model;
    repository.add(&mut aggregate).await.unwrap();
    assert_eq!(aggregate.version(), 1);
    aggregate.value = "confirmed".to_owned();
    repository.update(&mut aggregate).await.unwrap();
    assert_eq!(
        repository.read(&aggregate.id).await.unwrap(),
        Some(aggregate.clone())
    );
    assert_eq!(
        repository
            .read_at(&aggregate.id, 1)
            .await
            .unwrap()
            .unwrap()
            .value,
        "draft"
    );

    let service_name = format!("test-service-{}", Uuid::now_v7());
    RuntimeRegistry::register(&service_name, Arc::new(String::from("ready"))).unwrap();
    assert_eq!(
        RuntimeRegistry::get_or_err::<String>(&service_name)
            .unwrap()
            .as_str(),
        "ready"
    );
    RuntimeRegistry::unregister::<String>(&service_name).unwrap();
}
