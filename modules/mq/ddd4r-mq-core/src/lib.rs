//! Broker-neutral messaging contracts and at-least-once consumption engine.

#![forbid(unsafe_code)]

mod acknowledgment;
mod consumer;
mod error;
mod idempotency;
mod model;
mod port;
mod routing;

pub mod testkit;

pub use acknowledgment::{
    AckCommand, AckState, Acknowledgment, InMemoryAcknowledgment, UnsupportedAckOperation,
};
pub use consumer::{ConsumeResult, ConsumerEngine, RetryPolicy};
pub use error::{MqError, MqResult};
pub use idempotency::{Claim, IdempotencyStore, InMemoryIdempotencyStore};
pub use model::{
    BrokerType, Delivery, HandlerOutcome, MessageHeaders, MqEvent, MqProperties,
    PartitionKeyStrategy, PublishReceipt, TopicAddress,
};
pub use port::{MessageHandler, MessagePublisher, MessageStore, Subscription};
pub use routing::TagExpression;

use ddd4r_core::module::{ModuleDescriptor, ModuleMaturity};

/// Machine-readable migration descriptor.
pub const MODULE: ModuleDescriptor = ModuleDescriptor {
    java_artifact: "ddd4j-mq-core",
    rust_package: "ddd4r-mq-core",
    group: "mq",
    maturity: ModuleMaturity::InProgress,
};
