//! Idempotency claim state used by at-least-once consumers.

use std::sync::Arc;

use dashmap::DashMap;
use futures::future::BoxFuture;
use uuid::Uuid;

use crate::MqResult;

/// Claim result for a message identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// This consumer owns first processing.
    Acquired,
    /// Another handler currently owns processing.
    InFlight,
    /// The message already completed.
    Completed,
}

/// Idempotency state store.
pub trait IdempotencyStore: Send + Sync + 'static {
    /// Atomically claims a message.
    fn claim(&self, message_id: Uuid) -> BoxFuture<'_, MqResult<Claim>>;
    /// Marks successful or terminal processing.
    fn complete(&self, message_id: Uuid) -> BoxFuture<'_, MqResult<()>>;
    /// Releases a failed claim so redelivery can retry.
    fn release(&self, message_id: Uuid) -> BoxFuture<'_, MqResult<()>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    InFlight,
    Completed,
}

/// Process-local idempotency store for tests and single-instance applications.
#[derive(Debug, Clone, Default)]
pub struct InMemoryIdempotencyStore {
    states: Arc<DashMap<Uuid, State>>,
}

impl IdempotencyStore for InMemoryIdempotencyStore {
    fn claim(&self, message_id: Uuid) -> BoxFuture<'_, MqResult<Claim>> {
        Box::pin(async move {
            use dashmap::mapref::entry::Entry;
            Ok(match self.states.entry(message_id) {
                Entry::Vacant(entry) => {
                    entry.insert(State::InFlight);
                    Claim::Acquired
                }
                Entry::Occupied(entry) if *entry.get() == State::Completed => Claim::Completed,
                Entry::Occupied(_) => Claim::InFlight,
            })
        })
    }

    fn complete(&self, message_id: Uuid) -> BoxFuture<'_, MqResult<()>> {
        Box::pin(async move {
            self.states.insert(message_id, State::Completed);
            Ok(())
        })
    }

    fn release(&self, message_id: Uuid) -> BoxFuture<'_, MqResult<()>> {
        Box::pin(async move {
            self.states
                .remove_if(&message_id, |_, state| *state == State::InFlight);
            Ok(())
        })
    }
}
