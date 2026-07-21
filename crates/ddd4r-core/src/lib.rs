//! Framework-neutral DDD, CQRS and event-sourcing contracts.

#![forbid(unsafe_code)]

pub mod command;
pub mod context;
pub mod domain;
pub mod error;
pub mod event;
pub mod event_sourcing;
pub mod mapper;
pub mod module;
pub mod projection;
pub mod query;
pub mod repository;
pub mod runtime;
pub mod uow;

pub use error::{DddError, DddResult};

/// Common imports for applications using ddd4r.
pub mod prelude {
    pub use crate::command::{CommandBus, CommandExecutor, DefaultCommandBus};
    pub use crate::context::{ContextScope, Contexts, Registry};
    pub use crate::domain::{AggregateRoot, DomainModel, Entity, ValueObject};
    pub use crate::event::{DomainEvent, DomainEventExt, DomainEventPublisher, EventEnvelope};
    pub use crate::event_sourcing::EventSourcingRepository;
    pub use crate::mapper::DomainObjectMapper;
    pub use crate::projection::{Projection, ProjectionRunner, ReadModel};
    pub use crate::query::{Condition, Operator, Order, Page, PageRequest, PropertyRef, Query};
    pub use crate::repository::{AggregateRootExt, Repository, RepositoryRegistry, RepositoryRow};
    pub use crate::runtime::RuntimeRegistry;
    pub use crate::uow::{OutboxRecord, OutboxStatus, OutboxStore, UnitOfWork};
    pub use crate::{DddError, DddResult};
}
