//! Object-safe settlement port.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use futures::future::BoxFuture;
use thiserror::Error;

use crate::{BrokerType, MqError, MqResult};

/// Current settlement state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AckState {
    /// Delivery is unsettled.
    #[default]
    Pending,
    /// Delivery was acknowledged.
    Acknowledged,
    /// Delivery was negatively acknowledged for redelivery.
    Requeued,
    /// Delivery was rejected without redelivery.
    Discarded,
    /// Broker recover semantics were requested.
    Recovered,
}

impl AckState {
    const fn as_u8(self) -> u8 {
        match self {
            Self::Pending => 0,
            Self::Acknowledged => 1,
            Self::Requeued => 2,
            Self::Discarded => 3,
            Self::Recovered => 4,
        }
    }

    const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Acknowledged,
            2 => Self::Requeued,
            3 => Self::Discarded,
            4 => Self::Recovered,
            _ => Self::Pending,
        }
    }
}

/// Settlement operation passed to an adapter callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckCommand {
    /// Ack one message or all messages through this delivery tag.
    Ack {
        /// Whether the broker should use batch semantics.
        multiple: bool,
    },
    /// Negative acknowledgment.
    Nack {
        /// Whether the broker should use batch semantics.
        multiple: bool,
        /// Whether to redeliver.
        requeue: bool,
    },
    /// Reject one message.
    Reject {
        /// Whether to redeliver.
        requeue: bool,
    },
    /// Broker recovery operation.
    Recover {
        /// Whether to redeliver.
        requeue: bool,
    },
}

/// Marker error useful to adapters that cannot map an operation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("unsupported acknowledgment operation: {operation}")]
pub struct UnsupportedAckOperation {
    /// Stable operation name.
    pub operation: &'static str,
}

/// Message acknowledgment port.
pub trait Acknowledgment: Send + Sync + 'static {
    /// Broker delivery tag or sequence.
    fn delivery_tag(&self) -> u64;
    /// Message identifier.
    fn message_id(&self) -> &str;
    /// Correlation identifier.
    fn correlation_id(&self) -> Option<&str>;
    /// Broker family.
    fn broker_type(&self) -> BrokerType;
    /// Whether the underlying channel and delivery remain settleable.
    fn is_open(&self) -> bool;
    /// Current settlement state.
    fn state(&self) -> AckState;
    /// Applies exactly one terminal settlement.
    fn settle(&self, command: AckCommand) -> BoxFuture<'_, MqResult<()>>;

    /// Acknowledges one delivery.
    fn ack(&self) -> BoxFuture<'_, MqResult<()>> {
        self.settle(AckCommand::Ack { multiple: false })
    }

    /// Negative acknowledgment with explicit redelivery policy.
    fn nack(&self, requeue: bool) -> BoxFuture<'_, MqResult<()>> {
        self.settle(AckCommand::Nack {
            multiple: false,
            requeue,
        })
    }

    /// Rejects a single delivery.
    fn reject(&self, requeue: bool) -> BoxFuture<'_, MqResult<()>> {
        self.settle(AckCommand::Reject { requeue })
    }

    /// Requests broker recovery semantics.
    fn recover(&self, requeue: bool) -> BoxFuture<'_, MqResult<()>> {
        self.settle(AckCommand::Recover { requeue })
    }
}

type AckCallback = Arc<dyn Fn(AckCommand) -> MqResult<()> + Send + Sync>;

/// Deterministic acknowledgment used by the local broker and conformance tests.
#[derive(Clone)]
pub struct InMemoryAcknowledgment {
    delivery_tag: u64,
    message_id: String,
    correlation_id: Option<String>,
    broker: BrokerType,
    state: Arc<AtomicU8>,
    callback: AckCallback,
}

impl InMemoryAcknowledgment {
    /// Creates a settlement port with a broker callback.
    pub fn new(
        delivery_tag: u64,
        message_id: impl Into<String>,
        correlation_id: Option<String>,
        broker: BrokerType,
        callback: AckCallback,
    ) -> Self {
        Self {
            delivery_tag,
            message_id: message_id.into(),
            correlation_id,
            broker,
            state: Arc::new(AtomicU8::new(AckState::Pending.as_u8())),
            callback,
        }
    }

    fn target_state(command: AckCommand) -> AckState {
        match command {
            AckCommand::Ack { .. } => AckState::Acknowledged,
            AckCommand::Nack { requeue: true, .. } | AckCommand::Reject { requeue: true } => {
                AckState::Requeued
            }
            AckCommand::Nack { requeue: false, .. } | AckCommand::Reject { requeue: false } => {
                AckState::Discarded
            }
            AckCommand::Recover { .. } => AckState::Recovered,
        }
    }
}

impl std::fmt::Debug for InMemoryAcknowledgment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InMemoryAcknowledgment")
            .field("delivery_tag", &self.delivery_tag)
            .field("message_id", &self.message_id)
            .field("broker", &self.broker)
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}

impl Acknowledgment for InMemoryAcknowledgment {
    fn delivery_tag(&self) -> u64 {
        self.delivery_tag
    }

    fn message_id(&self) -> &str {
        &self.message_id
    }

    fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    fn broker_type(&self) -> BrokerType {
        self.broker
    }

    fn is_open(&self) -> bool {
        self.state() == AckState::Pending
    }

    fn state(&self) -> AckState {
        AckState::from_u8(self.state.load(Ordering::Acquire))
    }

    fn settle(&self, command: AckCommand) -> BoxFuture<'_, MqResult<()>> {
        Box::pin(async move {
            let target = Self::target_state(command);
            self.state
                .compare_exchange(
                    AckState::Pending.as_u8(),
                    target.as_u8(),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .map_err(|_| MqError::AlreadySettled {
                    message_id: self.message_id.clone(),
                })?;
            if let Err(error) = (self.callback)(command) {
                self.state
                    .store(AckState::Pending.as_u8(), Ordering::Release);
                return Err(error);
            }
            Ok(())
        })
    }
}
