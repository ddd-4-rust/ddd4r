//! Unit of Work and transactional outbox ports.

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DddResult;
use crate::event::EventEnvelope;

/// Transaction boundary used by repository adapters.
pub trait UnitOfWork: Send + Sync + 'static {
    /// Commits aggregate and outbox changes atomically.
    fn commit(&self) -> BoxFuture<'_, DddResult<()>>;

    /// Rolls back all pending changes.
    fn rollback(&self) -> BoxFuture<'_, DddResult<()>>;
}

/// Durable delivery state of an outbox record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxStatus {
    /// Committed and ready for delivery.
    Pending,
    /// Temporarily leased by one publisher.
    Publishing,
    /// Successfully acknowledged.
    Published,
    /// Exhausted retries and requires operator action.
    DeadLetter,
}

/// Durable event plus retry metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboxRecord {
    /// Outbox row identifier.
    pub id: Uuid,
    /// Domain event to deliver.
    pub event: EventEnvelope,
    /// Current delivery state.
    pub status: OutboxStatus,
    /// Number of failed delivery attempts.
    pub attempts: u32,
    /// UTC creation time.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// Earliest next retry time.
    #[serde(with = "time::serde::rfc3339::option")]
    pub next_attempt_at: Option<OffsetDateTime>,
    /// Sanitized last failure.
    pub last_error: Option<String>,
}

impl OutboxRecord {
    /// Creates a pending outbox record.
    pub fn pending(event: EventEnvelope) -> Self {
        Self {
            id: Uuid::now_v7(),
            event,
            status: OutboxStatus::Pending,
            attempts: 0,
            created_at: OffsetDateTime::now_utc(),
            next_attempt_at: None,
            last_error: None,
        }
    }
}

/// Durable transactional outbox port.
pub trait OutboxStore: Send + Sync + 'static {
    /// Appends events inside the current database transaction.
    fn append<'a>(&'a self, events: &'a [EventEnvelope]) -> BoxFuture<'a, DddResult<()>>;

    /// Claims a delivery batch until the supplied lease expiry.
    fn claim(
        &self,
        limit: usize,
        lease_until: OffsetDateTime,
    ) -> BoxFuture<'_, DddResult<Vec<OutboxRecord>>>;

    /// Marks a record as successfully published.
    fn mark_published(&self, id: Uuid) -> BoxFuture<'_, DddResult<()>>;

    /// Reschedules or dead-letters a failed record.
    fn mark_failed<'a>(
        &'a self,
        id: Uuid,
        retry_at: Option<OffsetDateTime>,
        error: &'a str,
    ) -> BoxFuture<'a, DddResult<()>>;
}
