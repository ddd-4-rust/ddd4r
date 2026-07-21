//! At-least-once, idempotent consumer orchestration.

use std::sync::Arc;

use tracing::warn;

use crate::{
    Claim, Delivery, HandlerOutcome, IdempotencyStore, MessageHandler, MessageStore, MqResult,
    TagExpression,
};

/// Retry and dead-letter policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Maximum delivery attempt, including the first attempt.
    pub max_attempts: u32,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self { max_attempts: 3 }
    }
}

/// Observable result of one consumer-engine invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumeResult {
    /// Handler completed and delivery was acknowledged.
    Acknowledged,
    /// Message was already completed; delivery was acknowledged without invoking the handler.
    Duplicate,
    /// Message did not match the listener tag expression and was acknowledged.
    Filtered,
    /// Delivery was negatively acknowledged for retry.
    Requeued,
    /// Delivery reached a terminal discard/dead-letter disposition.
    DeadLettered,
}

/// Shared consumption state machine used by every broker adapter.
#[derive(Clone)]
pub struct ConsumerEngine {
    idempotency: Arc<dyn IdempotencyStore>,
    store: Option<Arc<dyn MessageStore>>,
    retry: RetryPolicy,
}

impl ConsumerEngine {
    /// Creates an engine with an optional pre-handler persistence sink.
    pub fn new(
        idempotency: Arc<dyn IdempotencyStore>,
        store: Option<Arc<dyn MessageStore>>,
        retry: RetryPolicy,
    ) -> Self {
        Self {
            idempotency,
            store,
            retry: RetryPolicy {
                max_attempts: retry.max_attempts.max(1),
            },
        }
    }

    /// Consumes and settles one delivery.
    pub async fn consume(
        &self,
        delivery: Delivery,
        tags: &TagExpression,
        handler: &dyn MessageHandler,
    ) -> MqResult<ConsumeResult> {
        let message_id = delivery.event.message_id;
        if !tags.matches(delivery.event.destination.tag.as_deref()) {
            delivery.acknowledgment.ack().await?;
            return Ok(ConsumeResult::Filtered);
        }

        match self.idempotency.claim(message_id).await? {
            Claim::Completed => {
                delivery.acknowledgment.ack().await?;
                return Ok(ConsumeResult::Duplicate);
            }
            Claim::InFlight => {
                delivery.acknowledgment.nack(true).await?;
                return Ok(ConsumeResult::Requeued);
            }
            Claim::Acquired => {}
        }

        if let Some(store) = &self.store
            && let Err(error) = store.store(&delivery.event).await
        {
            // Frozen ddd4j treats optional audit persistence as fail-open.
            warn!(message_id = %message_id, error = %error, "MQ audit persistence failed");
        }

        let outcome = handler.handle(delivery.clone()).await;
        match outcome {
            Ok(HandlerOutcome::Ack) => {
                self.idempotency.complete(message_id).await?;
                delivery.acknowledgment.ack().await?;
                Ok(ConsumeResult::Acknowledged)
            }
            Ok(HandlerOutcome::Requeue) => {
                self.idempotency.release(message_id).await?;
                delivery.acknowledgment.nack(true).await?;
                Ok(ConsumeResult::Requeued)
            }
            Err(_) if delivery.event.delivery_attempt < self.retry.max_attempts => {
                self.idempotency.release(message_id).await?;
                delivery.acknowledgment.nack(true).await?;
                Ok(ConsumeResult::Requeued)
            }
            Ok(HandlerOutcome::Discard) | Err(_) => {
                self.idempotency.complete(message_id).await?;
                delivery.acknowledgment.nack(false).await?;
                Ok(ConsumeResult::DeadLettered)
            }
        }
    }
}
