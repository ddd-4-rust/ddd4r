//! Facade for the broker-neutral MQ contracts and built-in local adapter.

#![forbid(unsafe_code)]

pub use ddd4r_mq_core as core;
pub use ddd4r_mq_disruptor as disruptor;

/// Common messaging imports.
pub mod prelude {
    pub use ddd4r_mq_core::{
        Acknowledgment, BrokerType, ConsumerEngine, Delivery, HandlerOutcome, IdempotencyStore,
        MessageHandler, MessagePublisher, MqEvent, MqProperties, RetryPolicy, Subscription,
        TagExpression, TopicAddress,
    };
    pub use ddd4r_mq_disruptor::DisruptorBroker;
}

use ddd4r_core::module::{ModuleDescriptor, ModuleMaturity};

/// Machine-readable migration descriptor.
pub const MODULE: ModuleDescriptor = ModuleDescriptor {
    java_artifact: "ddd4j-mq",
    rust_package: "ddd4r-mq",
    group: "mq",
    maturity: ModuleMaturity::InProgress,
};
