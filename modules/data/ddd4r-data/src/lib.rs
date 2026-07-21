//! Shared persistence capabilities and repository conformance suite.

#![forbid(unsafe_code)]

use ddd4r_core::domain::{AggregateRoot, DomainModel, Entity};
use ddd4r_core::event::EventEnvelope;
use ddd4r_core::module::{ModuleDescriptor, ModuleMaturity};
use ddd4r_core::query::{Order, PropertyRef, Query};
use ddd4r_core::repository::Repository;
use ddd4r_core::{DddError, DddResult};
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

    /// Implemented behaviors. A false flag prevents stable publication.
    fn capabilities(&self) -> BackendCapabilities;
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

fn conformance_error(message: &str) -> DddError {
    DddError::Adapter {
        adapter: "data-conformance",
        message: message.to_owned(),
    }
}
