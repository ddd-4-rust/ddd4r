//! Domain model marker and aggregate contracts.

use std::fmt::{Debug, Display};
use std::hash::Hash;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::DddResult;
use crate::event::{DomainEvent, EventEnvelope};

/// Base contract for an identified domain model.
pub trait DomainModel: Send + Sync + 'static {
    /// Identifier type.
    type Id: Clone
        + Debug
        + Display
        + Eq
        + Hash
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static;

    /// Returns the stable model identifier.
    fn id(&self) -> &Self::Id;
}

/// Marker contract for an entity with identity and lifecycle.
pub trait Entity: DomainModel {}

/// Marker contract for an immutable, structurally equal value object.
pub trait ValueObject: Clone + Eq + Send + Sync + 'static {}

/// Root of a consistency boundary.
pub trait AggregateRoot: Entity + Sized {
    /// Returns the optimistic-lock/event-stream version.
    fn version(&self) -> u64;

    /// Updates the optimistic-lock/event-stream version.
    fn set_version(&mut self, version: u64);

    /// Returns buffered, not-yet-committed events.
    fn recorded_events(&self) -> &[EventEnvelope];

    /// Returns mutable access to buffered events for infrastructure code.
    fn recorded_events_mut(&mut self) -> &mut Vec<EventEnvelope>;

    /// Records a domain event against this aggregate.
    fn record_event<E>(&mut self, event: &E) -> DddResult<()>
    where
        E: DomainEvent,
    {
        let envelope = EventEnvelope::for_aggregate(self, event)?;
        self.recorded_events_mut().push(envelope);
        Ok(())
    }

    /// Atomically drains buffered events after persistence has accepted them.
    fn pull_events(&mut self) -> Vec<EventEnvelope> {
        std::mem::take(self.recorded_events_mut())
    }
}
