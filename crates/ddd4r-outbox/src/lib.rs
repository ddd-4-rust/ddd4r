//! Transactional outbox implementations.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::sync::Arc;

use ddd4r_core::DddResult;
use ddd4r_core::event::EventEnvelope;
use ddd4r_core::uow::{OutboxRecord, OutboxStatus, OutboxStore};
use futures::future::BoxFuture;
use time::OffsetDateTime;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Deterministic in-memory outbox for contract tests and local development.
#[derive(Clone, Default)]
pub struct InMemoryOutboxStore {
    records: Arc<Mutex<BTreeMap<Uuid, OutboxRecord>>>,
}

impl InMemoryOutboxStore {
    /// Creates an empty outbox.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns all records ordered by identifier.
    pub async fn records(&self) -> Vec<OutboxRecord> {
        self.records.lock().await.values().cloned().collect()
    }
}

impl OutboxStore for InMemoryOutboxStore {
    fn append<'a>(&'a self, events: &'a [EventEnvelope]) -> BoxFuture<'a, DddResult<()>> {
        Box::pin(async move {
            let mut records = self.records.lock().await;
            for event in events {
                let record = OutboxRecord::pending(event.clone());
                records.entry(record.id).or_insert(record);
            }
            Ok(())
        })
    }

    fn claim(
        &self,
        limit: usize,
        lease_until: OffsetDateTime,
    ) -> BoxFuture<'_, DddResult<Vec<OutboxRecord>>> {
        Box::pin(async move {
            let now = OffsetDateTime::now_utc();
            let mut records = self.records.lock().await;
            let ids = records
                .iter()
                .filter(|(_, record)| {
                    matches!(
                        record.status,
                        OutboxStatus::Pending | OutboxStatus::Publishing
                    ) && record
                        .next_attempt_at
                        .is_none_or(|ready_at| ready_at <= now)
                })
                .map(|(id, _)| *id)
                .take(limit)
                .collect::<Vec<_>>();
            let mut claimed = Vec::with_capacity(ids.len());
            for id in ids {
                if let Some(record) = records.get_mut(&id) {
                    record.status = OutboxStatus::Publishing;
                    record.next_attempt_at = Some(lease_until);
                    claimed.push(record.clone());
                }
            }
            Ok(claimed)
        })
    }

    fn mark_published(&self, id: Uuid) -> BoxFuture<'_, DddResult<()>> {
        Box::pin(async move {
            if let Some(record) = self.records.lock().await.get_mut(&id) {
                record.status = OutboxStatus::Published;
                record.next_attempt_at = None;
                record.last_error = None;
            }
            Ok(())
        })
    }

    fn mark_failed<'a>(
        &'a self,
        id: Uuid,
        retry_at: Option<OffsetDateTime>,
        error: &'a str,
    ) -> BoxFuture<'a, DddResult<()>> {
        Box::pin(async move {
            if let Some(record) = self.records.lock().await.get_mut(&id) {
                record.attempts = record.attempts.saturating_add(1);
                record.status = if retry_at.is_some() {
                    OutboxStatus::Pending
                } else {
                    OutboxStatus::DeadLetter
                };
                record.next_attempt_at = retry_at;
                record.last_error = Some(error.to_owned());
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use ddd4r_core::event::EventEnvelope;
    use ddd4r_core::uow::{OutboxStatus, OutboxStore};
    use serde_json::{Map, Value};
    use time::{Duration, OffsetDateTime};
    use uuid::Uuid;

    use super::InMemoryOutboxStore;

    fn event() -> EventEnvelope {
        EventEnvelope {
            event_id: Uuid::now_v7(),
            aggregate_type: "Order".to_owned(),
            aggregate_id: "1".to_owned(),
            aggregate_version: 1,
            event_type: "OrderCreated".to_owned(),
            event_version: 1,
            occurred_at: OffsetDateTime::now_utc(),
            tenant_id: None,
            correlation_id: None,
            causation_id: None,
            support_keys: Vec::new(),
            payload: Value::Null,
            metadata: Map::new(),
        }
    }

    #[tokio::test]
    async fn append_is_idempotent_and_publish_is_explicit() {
        let store = InMemoryOutboxStore::new();
        let event = event();
        store.append(&[event.clone(), event]).await.unwrap();
        assert_eq!(store.records().await.len(), 1);
        let claimed = store
            .claim(10, OffsetDateTime::now_utc() + Duration::minutes(1))
            .await
            .unwrap();
        assert_eq!(claimed.len(), 1);
        store.mark_published(claimed[0].id).await.unwrap();
        assert_eq!(store.records().await[0].status, OutboxStatus::Published);
    }

    #[tokio::test]
    async fn expired_leases_are_recovered_and_failures_retry_or_dead_letter() {
        let store = InMemoryOutboxStore::new();
        store.append(&[event()]).await.unwrap();
        let first = store
            .claim(1, OffsetDateTime::now_utc() - Duration::seconds(1))
            .await
            .unwrap();
        let recovered = store
            .claim(1, OffsetDateTime::now_utc() + Duration::minutes(1))
            .await
            .unwrap();
        assert_eq!(recovered[0].id, first[0].id);

        store
            .mark_failed(
                recovered[0].id,
                Some(OffsetDateTime::now_utc() - Duration::seconds(1)),
                "temporary",
            )
            .await
            .unwrap();
        let retried = store
            .claim(1, OffsetDateTime::now_utc() + Duration::minutes(1))
            .await
            .unwrap();
        store
            .mark_failed(retried[0].id, None, "exhausted")
            .await
            .unwrap();
        let record = &store.records().await[0];
        assert_eq!(record.status, OutboxStatus::DeadLetter);
        assert_eq!(record.attempts, 2);
        assert_eq!(record.last_error.as_deref(), Some("exhausted"));
    }
}
