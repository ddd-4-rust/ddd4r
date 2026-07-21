//! Domain event envelopes and publisher compatibility facade.

use std::any::type_name;
use std::borrow::Cow;

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DddResult;
use crate::context::Contexts;
use crate::domain::AggregateRoot;

/// Global service key for the domain event publisher.
pub const DOMAIN_EVENT_PUBLISHER_KEY: &str = "ddd4r.spi.domain.DomainEventPublisher";

/// Serializable domain event contract.
pub trait DomainEvent: Serialize + Send + Sync + 'static {
    /// Stable event type. Defaults to the Rust type name.
    fn event_type(&self) -> Cow<'static, str> {
        Cow::Borrowed(type_name::<Self>())
    }

    /// Version of the serialized event schema.
    fn event_version(&self) -> u32 {
        1
    }

    /// Aggregate/source identifier used for direct event publishing.
    fn source(&self) -> String;

    /// Aggregate type used for direct event publishing.
    fn aggregate_type(&self) -> Cow<'static, str> {
        Cow::Borrowed("unknown")
    }

    /// Optional tenant identifier.
    fn tenant_id(&self) -> Option<&str> {
        None
    }

    /// Optional correlation identifier.
    fn correlation_id(&self) -> Option<Uuid> {
        None
    }

    /// Optional causation identifier.
    fn causation_id(&self) -> Option<Uuid> {
        None
    }

    /// Strategy keys supported by this event.
    fn support_keys(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Stable event representation used by event stores, outboxes and publishers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope {
    /// Globally unique event identifier.
    pub event_id: Uuid,
    /// Aggregate Rust/domain type.
    pub aggregate_type: String,
    /// String form of the aggregate identifier.
    pub aggregate_id: String,
    /// Aggregate version at which the event was recorded.
    pub aggregate_version: u64,
    /// Stable event type.
    pub event_type: String,
    /// Event schema version.
    pub event_version: u32,
    /// UTC occurrence time.
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: OffsetDateTime,
    /// Optional tenant identifier.
    pub tenant_id: Option<String>,
    /// Optional correlation identifier.
    pub correlation_id: Option<Uuid>,
    /// Optional causation identifier.
    pub causation_id: Option<Uuid>,
    /// Strategy keys used by filtered listeners.
    pub support_keys: Vec<String>,
    /// Domain payload.
    pub payload: Value,
    /// Extensible, non-domain transport metadata.
    pub metadata: Map<String, Value>,
}

impl EventEnvelope {
    /// Builds an envelope for an event recorded by an aggregate.
    pub fn for_aggregate<A, E>(aggregate: &A, event: &E) -> DddResult<Self>
    where
        A: AggregateRoot,
        E: DomainEvent,
    {
        Ok(Self {
            event_id: Uuid::now_v7(),
            aggregate_type: type_name::<A>().to_owned(),
            aggregate_id: aggregate.id().to_string(),
            aggregate_version: aggregate.version().saturating_add(1),
            event_type: event.event_type().into_owned(),
            event_version: event.event_version(),
            occurred_at: OffsetDateTime::now_utc(),
            tenant_id: event.tenant_id().map(str::to_owned),
            correlation_id: event.correlation_id(),
            causation_id: event.causation_id(),
            support_keys: event.support_keys(),
            payload: serde_json::to_value(event)?,
            metadata: Map::new(),
        })
    }

    /// Builds an envelope for a directly published event.
    pub fn from_event<E>(event: &E) -> DddResult<Self>
    where
        E: DomainEvent,
    {
        Ok(Self {
            event_id: Uuid::now_v7(),
            aggregate_type: event.aggregate_type().into_owned(),
            aggregate_id: event.source(),
            aggregate_version: 0,
            event_type: event.event_type().into_owned(),
            event_version: event.event_version(),
            occurred_at: OffsetDateTime::now_utc(),
            tenant_id: event.tenant_id().map(str::to_owned),
            correlation_id: event.correlation_id(),
            causation_id: event.causation_id(),
            support_keys: event.support_keys(),
            payload: serde_json::to_value(event)?,
            metadata: Map::new(),
        })
    }

    /// Returns whether this event supports at least one supplied strategy key.
    pub fn supports<'a>(&self, keys: impl IntoIterator<Item = &'a str>) -> bool {
        keys.into_iter()
            .any(|key| self.support_keys.iter().any(|candidate| candidate == key))
    }

    /// Returns whether this event belongs to one of the supplied tenants.
    pub fn tenant_in<'a>(&self, tenants: impl IntoIterator<Item = &'a str>) -> bool {
        self.tenant_id
            .as_deref()
            .is_some_and(|tenant| tenants.into_iter().any(|candidate| candidate == tenant))
    }
}

/// Framework-neutral event publisher SPI.
pub trait DomainEventPublisher: Send + Sync + 'static {
    /// Publishes a domain event and optionally returns a handler result.
    fn publish<'a>(&'a self, event: &'a EventEnvelope) -> BoxFuture<'a, DddResult<Option<Value>>>;
}

/// ddd4j-compatible `event.publish().await` facade.
pub trait DomainEventExt: DomainEvent {
    /// Publishes this event through task-local/global context lookup.
    fn publish(&self) -> BoxFuture<'_, DddResult<Option<Value>>>
    where
        Self: Sized,
    {
        Box::pin(async move {
            let envelope = EventEnvelope::from_event(self)?;
            let publisher =
                Contexts::get_or_err::<dyn DomainEventPublisher>(DOMAIN_EVENT_PUBLISHER_KEY)?;
            publisher.publish(&envelope).await
        })
    }
}

impl<T> DomainEventExt for T where T: DomainEvent {}
