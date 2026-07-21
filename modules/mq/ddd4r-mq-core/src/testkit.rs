//! Shared broker conformance helpers.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::future::BoxFuture;

use crate::{
    AckCommand, BrokerType, HandlerOutcome, InMemoryAcknowledgment, MessageHandler, MqError,
    MqResult,
};

/// Creates an in-memory acknowledgment and an operation log.
pub fn acknowledgment(
    broker: BrokerType,
    message_id: impl Into<String>,
) -> (
    InMemoryAcknowledgment,
    Arc<std::sync::Mutex<Vec<AckCommand>>>,
) {
    let commands = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = commands.clone();
    let callback = Arc::new(move |command| {
        sink.lock()
            .map_err(|_| MqError::Broker {
                broker: broker.as_str().to_owned(),
                operation: "ack",
                message: "ack command log is poisoned".to_owned(),
            })?
            .push(command);
        Ok(())
    });
    (
        InMemoryAcknowledgment::new(1, message_id, None, broker, callback),
        commands,
    )
}

/// Deterministic handler with an invocation counter.
#[derive(Clone)]
pub struct CountingHandler {
    /// Number of invocations.
    pub calls: Arc<AtomicUsize>,
    outcome: Result<HandlerOutcome, MqError>,
}

impl CountingHandler {
    /// Creates a successful handler.
    pub fn success(outcome: HandlerOutcome) -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            outcome: Ok(outcome),
        }
    }

    /// Creates a failing handler.
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            outcome: Err(MqError::Handler {
                message: message.into(),
            }),
        }
    }

    /// Reads invocation count.
    pub fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl MessageHandler for CountingHandler {
    fn handle(&self, _delivery: crate::Delivery) -> BoxFuture<'_, MqResult<HandlerOutcome>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.outcome.clone()
        })
    }
}
