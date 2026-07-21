//! Broker, handler and persistence ports.

use futures::future::BoxFuture;

use crate::{Delivery, HandlerOutcome, MqEvent, MqResult, PublishReceipt};

/// Asynchronous message publisher.
pub trait MessagePublisher: Send + Sync + 'static {
    /// Publishes one immutable envelope.
    fn publish(&self, event: MqEvent) -> BoxFuture<'_, MqResult<PublishReceipt>>;
}

/// Asynchronous delivery handler.
pub trait MessageHandler: Send + Sync + 'static {
    /// Handles one delivery and selects a terminal or retry disposition.
    fn handle(&self, delivery: Delivery) -> BoxFuture<'_, MqResult<HandlerOutcome>>;
}

/// Optional pre-handler message persistence port.
pub trait MessageStore: Send + Sync + 'static {
    /// Persists a message idempotently.
    fn store(&self, event: &MqEvent) -> BoxFuture<'_, MqResult<()>>;
}

/// Owned subscription handle.
pub trait Subscription: Send + Sync + 'static {
    /// Stable subscription identifier.
    fn id(&self) -> &str;
    /// Stops delivery and waits for owned tasks to finish.
    fn close(&self) -> BoxFuture<'_, MqResult<()>>;
}
