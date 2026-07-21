//! Shared persistence capabilities and repository conformance suite.

#![forbid(unsafe_code)]

use ddd4r_core::domain::{AggregateRoot, DomainModel, Entity};
use ddd4r_core::event::{DomainEvent, EventEnvelope};
use ddd4r_core::module::{ModuleDescriptor, ModuleMaturity};
use ddd4r_core::query::{Condition, Operator, Order, Page, PropertyRef, Query};
use ddd4r_core::repository::{Repository, RepositoryRow};
use ddd4r_core::{DddError, DddResult};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

/// Machine-readable migration descriptor.
pub const MODULE: ModuleDescriptor = ModuleDescriptor {
    java_artifact: "ddd4j-data",
    rust_package: "ddd4r-data",
    group: "data",
    maturity: ModuleMaturity::InProgress,
};

/// Database engines required by the stable compatibility matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseKind {
    /// `PostgreSQL`.
    PostgreSql,
    /// `MySQL` or protocol-compatible database.
    MySql,
    /// `SQLite`.
    Sqlite,
    /// Microsoft SQL Server.
    SqlServer,
}

/// Auditable feature declaration for a data adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Implemented and backed by executable conformance evidence.
    Supported,
    /// Required for stable release but not implemented yet.
    Planned,
}

/// Auditable feature declaration for a data adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendCapabilities {
    /// Basic create/read/update/delete support.
    pub crud: Capability,
    /// Batched mutations.
    pub batch: Capability,
    /// Strongly typed predicates, ordering, and pages.
    pub query: Capability,
    /// Optimistic version checks.
    pub optimistic_lock: Capability,
    /// Logical deletion.
    pub logical_delete: Capability,
    /// Transactional unit of work.
    pub unit_of_work: Capability,
    /// Aggregate and outbox atomic commit.
    pub transactional_outbox: Capability,
    /// Tenant isolation.
    pub tenant_isolation: Capability,
    /// Data-permission predicates.
    pub data_scope: Capability,
    /// Created/updated audit filling.
    pub audit_fill: Capability,
    /// Event-sourcing repository support.
    pub event_sourcing: Capability,
}

impl BackendCapabilities {
    /// Returns whether every stable-backend capability is implemented.
    pub const fn is_complete(self) -> bool {
        matches!(
            self,
            Self {
                crud: Capability::Supported,
                batch: Capability::Supported,
                query: Capability::Supported,
                optimistic_lock: Capability::Supported,
                logical_delete: Capability::Supported,
                unit_of_work: Capability::Supported,
                transactional_outbox: Capability::Supported,
                tenant_isolation: Capability::Supported,
                data_scope: Capability::Supported,
                audit_fill: Capability::Supported,
                event_sourcing: Capability::Supported,
            }
        )
    }
}

/// Adapter identity and declared support matrix.
pub trait DataBackend: Send + Sync + 'static {
    /// Stable backend name such as `sqlx` or `rbatis`.
    fn name(&self) -> &'static str;

    /// Supported database engines.
    fn databases(&self) -> &'static [DatabaseKind];

    /// Implemented behaviors. Any `Planned` capability prevents stable publication.
    fn capabilities(&self) -> BackendCapabilities;
}

/// Repository extension that persists an aggregate and its buffered events atomically.
pub trait TransactionalEventRepository<A>: Repository<A>
where
    A: AggregateRoot,
{
    /// Commits the aggregate and its current event buffer in one database transaction.
    /// Events are removed from the aggregate only after the database commit succeeds.
    fn save_with_outbox<'a>(&'a self, aggregate: &'a mut A) -> BoxFuture<'a, DddResult<()>>;

    /// Returns committed outbox envelopes in insertion order for conformance and dispatching.
    fn pending_outbox_events(&self) -> BoxFuture<'_, DddResult<Vec<EventEnvelope>>>;
}

/// Canonical aggregate used by every backend's executable conformance suite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformanceAggregate {
    /// Stable aggregate identifier.
    pub id: String,
    /// Queryable business value.
    pub name: String,
    /// Optimistic-lock version.
    pub version: u64,
    /// Buffered events are not persisted by the aggregate table.
    #[serde(skip)]
    pub events: Vec<EventEnvelope>,
}

impl ConformanceAggregate {
    /// Creates a new, unpersisted aggregate.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: 0,
            events: Vec::new(),
        }
    }
}

impl DomainModel for ConformanceAggregate {
    type Id = String;

    fn id(&self) -> &Self::Id {
        &self.id
    }
}

impl Entity for ConformanceAggregate {}

impl AggregateRoot for ConformanceAggregate {
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

/// Evidence returned after a backend passes the shared repository contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepositoryConformanceReport {
    /// CRUD and batch operations passed.
    pub crud_and_batch: bool,
    /// Predicate, ordering, and paging passed.
    pub query_and_page: bool,
    /// Stale updates were rejected.
    pub optimistic_lock: bool,
}

/// Evidence returned after a backend passes the transactional outbox contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionalOutboxConformanceReport {
    /// Aggregate and event were committed together.
    pub atomic_commit: bool,
    /// A duplicate outbox event rolled the aggregate update back.
    pub atomic_rollback: bool,
    /// Aggregate events were cleared only after commit and retained after rollback.
    pub event_buffer_lifecycle: bool,
}

/// Runs the same core repository behavior against any adapter implementation.
pub async fn verify_repository_conformance(
    repository: &dyn Repository<ConformanceAggregate>,
) -> DddResult<RepositoryConformanceReport> {
    const NAME: PropertyRef<ConformanceAggregate, String> = PropertyRef::new("name");

    let mut first = ConformanceAggregate::new("contract-1", "first");
    repository.save(&mut first).await?;
    if first.version != 1 || repository.find_by_id(&first.id).await? != Some(first.clone()) {
        return Err(conformance_error("insert/find contract failed"));
    }

    let mut stale = first.clone();
    "updated".clone_into(&mut first.name);
    repository.update_by_id(&mut first).await?;
    if first.version != 2 {
        return Err(conformance_error(
            "update did not advance aggregate version",
        ));
    }
    "stale".clone_into(&mut stale.name);
    if !matches!(
        repository.update_by_id(&mut stale).await,
        Err(DddError::OptimisticLockConflict { .. })
    ) {
        return Err(conformance_error("stale update was not rejected"));
    }

    let mut batch = [
        ConformanceAggregate::new("contract-2", "second"),
        ConformanceAggregate::new("contract-3", "third"),
    ];
    repository.save_batch(&mut batch).await?;
    if repository.count_all().await? != 3 {
        return Err(conformance_error("batch/count contract failed"));
    }

    let query = Query::new()
        .and(NAME.ne("missing".to_owned())?)
        .order_by(Order::desc("name"))
        .page(1, 2);
    let page = repository.page(&query).await?;
    if page.total != 3 || page.records.len() != 2 || page.records[0].name != "updated" {
        return Err(conformance_error("query/order/page contract failed"));
    }

    repository.delete_by_id(&batch[0].id).await?;
    if repository.exists_by_id(&batch[0].id).await? {
        return Err(conformance_error("delete contract failed"));
    }
    Ok(RepositoryConformanceReport {
        crud_and_batch: true,
        query_and_page: true,
        optimistic_lock: true,
    })
}

/// Runs the same aggregate/outbox atomicity contract against any relational adapter.
pub async fn verify_transactional_outbox_conformance(
    repository: &dyn TransactionalEventRepository<ConformanceAggregate>,
) -> DddResult<TransactionalOutboxConformanceReport> {
    #[derive(Serialize)]
    struct Created {
        source: String,
    }

    impl DomainEvent for Created {
        fn source(&self) -> String {
            self.source.clone()
        }
    }

    let mut aggregate = ConformanceAggregate::new("outbox-contract", "committed");
    aggregate.record_event(&Created {
        source: aggregate.id.clone(),
    })?;
    let duplicate = aggregate.recorded_events()[0].clone();
    repository.save_with_outbox(&mut aggregate).await?;
    let committed = repository.pending_outbox_events().await?;
    if aggregate.version != 1
        || aggregate.has_recorded_events()
        || committed.as_slice() != [duplicate.clone()]
    {
        return Err(conformance_error("aggregate/outbox atomic commit failed"));
    }

    "must-roll-back".clone_into(&mut aggregate.name);
    aggregate.recorded_events_mut().push(duplicate);
    if repository.save_with_outbox(&mut aggregate).await.is_ok() {
        return Err(conformance_error(
            "duplicate outbox event did not fail the transaction",
        ));
    }
    let persisted = repository
        .find_by_id(&aggregate.id)
        .await?
        .ok_or_else(|| conformance_error("aggregate disappeared after rollback"))?;
    if aggregate.version != 1
        || aggregate.recorded_events().len() != 1
        || persisted.version != 1
        || persisted.name != "committed"
        || repository.pending_outbox_events().await?.len() != 1
    {
        return Err(conformance_error("aggregate/outbox atomic rollback failed"));
    }

    Ok(TransactionalOutboxConformanceReport {
        atomic_commit: true,
        atomic_rollback: true,
        event_buffer_lifecycle: true,
    })
}

fn conformance_error(message: &str) -> DddError {
    DddError::Adapter {
        adapter: "data-conformance",
        message: message.to_owned(),
    }
}

/// Filters and orders already-loaded aggregates using the shared query semantics.
pub fn matching_aggregates<A>(
    aggregates: impl IntoIterator<Item = A>,
    query: &Query<A>,
) -> DddResult<Vec<A>>
where
    A: AggregateRoot + Serialize,
{
    let mut matches = Vec::new();
    for aggregate in aggregates {
        if matches_query(&aggregate, query)? {
            matches.push(aggregate);
        }
    }
    matches.sort_by(|left, right| compare_aggregates(left, right, query));
    Ok(matches)
}

/// Applies the common one-based page contract to ordered aggregates.
pub fn page_aggregates<A>(aggregates: Vec<A>, query: &Query<A>) -> DddResult<Page<A>>
where
    A: AggregateRoot,
{
    let total = u64::try_from(aggregates.len()).map_err(|error| DddError::Adapter {
        adapter: "data-query",
        message: error.to_string(),
    })?;
    let size = query
        .page
        .size
        .unwrap_or_else(|| u64::try_from(aggregates.len()).unwrap_or(u64::MAX));
    let offset = query.page.current.saturating_sub(1).saturating_mul(size);
    let offset = usize::try_from(offset).unwrap_or(usize::MAX);
    let limit = usize::try_from(size).unwrap_or(usize::MAX);
    Ok(Page {
        records: aggregates.into_iter().skip(offset).take(limit).collect(),
        total,
        current: query.page.current,
        size,
    })
}

/// Converts aggregates to map rows and applies an optional select list.
pub fn selected_rows<A>(aggregates: Vec<A>, query: &Query<A>) -> DddResult<Vec<RepositoryRow>>
where
    A: AggregateRoot + Serialize,
{
    aggregates
        .into_iter()
        .map(|aggregate| {
            let serde_json::Value::Object(mut row) = serde_json::to_value(aggregate)? else {
                return Err(DddError::Adapter {
                    adapter: "data-query",
                    message: "aggregate must serialize as an object".to_owned(),
                });
            };
            if !query.select_columns.is_empty() {
                row.retain(|key, _| query.select_columns.contains(key));
            }
            Ok(row)
        })
        .collect()
}

fn matches_query<A>(aggregate: &A, query: &Query<A>) -> DddResult<bool>
where
    A: AggregateRoot + Serialize,
{
    let serde_json::Value::Object(object) = serde_json::to_value(aggregate)? else {
        return Ok(false);
    };
    Ok(query
        .conditions
        .iter()
        .all(|condition| matches_condition(object.get(&condition.property), condition)))
}

fn matches_condition(value: Option<&serde_json::Value>, condition: &Condition) -> bool {
    let first = condition.operands.first();
    match condition.operator {
        Operator::Eq => value == first,
        Operator::Ne => value != first,
        Operator::Gt => compare_values(value, first).is_gt(),
        Operator::Ge => !compare_values(value, first).is_lt(),
        Operator::Lt => compare_values(value, first).is_lt(),
        Operator::Le => !compare_values(value, first).is_gt(),
        Operator::Like => text(value)
            .is_some_and(|value| text(first).is_some_and(|operand| value.contains(operand))),
        Operator::LikeLeft => text(value)
            .is_some_and(|value| text(first).is_some_and(|operand| value.ends_with(operand))),
        Operator::LikeRight => text(value)
            .is_some_and(|value| text(first).is_some_and(|operand| value.starts_with(operand))),
        Operator::NotLike => !matches_condition(
            value,
            &Condition {
                property: condition.property.clone(),
                operator: Operator::Like,
                operands: condition.operands.clone(),
            },
        ),
        Operator::In => value.is_some_and(|value| condition.operands.contains(value)),
        Operator::NotIn => value.is_none_or(|value| !condition.operands.contains(value)),
        Operator::IsNull => value.is_none_or(serde_json::Value::is_null),
        Operator::IsNotNull => value.is_some_and(|value| !value.is_null()),
    }
}

fn compare_aggregates<A>(left: &A, right: &A, query: &Query<A>) -> std::cmp::Ordering
where
    A: AggregateRoot + Serialize,
{
    let left = serde_json::to_value(left).unwrap_or(serde_json::Value::Null);
    let right = serde_json::to_value(right).unwrap_or(serde_json::Value::Null);
    for order in &query.orders {
        let ordering = compare_values(left.get(&order.property), right.get(&order.property));
        if !ordering.is_eq() {
            return if order.ascending {
                ordering
            } else {
                ordering.reverse()
            };
        }
    }
    std::cmp::Ordering::Equal
}

fn compare_values(
    left: Option<&serde_json::Value>,
    right: Option<&serde_json::Value>,
) -> std::cmp::Ordering {
    use serde_json::Value;
    use std::cmp::Ordering;

    match (left, right) {
        (Some(Value::Number(left)), Some(Value::Number(right))) => left
            .as_f64()
            .partial_cmp(&right.as_f64())
            .unwrap_or(Ordering::Equal),
        (Some(Value::String(left)), Some(Value::String(right))) => left.cmp(right),
        (Some(Value::Bool(left)), Some(Value::Bool(right))) => left.cmp(right),
        (None | Some(Value::Null), None | Some(Value::Null)) => Ordering::Equal,
        (None | Some(Value::Null), _) => Ordering::Less,
        (_, None | Some(Value::Null)) => Ordering::Greater,
        (Some(left), Some(right)) => left.to_string().cmp(&right.to_string()),
    }
}

fn text(value: Option<&serde_json::Value>) -> Option<&str> {
    value.and_then(serde_json::Value::as_str)
}
