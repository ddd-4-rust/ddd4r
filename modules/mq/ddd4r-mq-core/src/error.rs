//! Stable messaging errors.

use thiserror::Error;

/// Result returned by messaging contracts.
pub type MqResult<T> = Result<T, MqError>;

/// Broker-neutral error model.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MqError {
    /// Required configuration or message data is invalid.
    #[error("invalid MQ input: {message}")]
    InvalidInput {
        /// Sanitized reason.
        message: String,
    },
    /// A broker operation failed.
    #[error("MQ broker {broker} failed during {operation}: {message}")]
    Broker {
        /// Stable broker configuration value.
        broker: String,
        /// Stable operation name.
        operation: &'static str,
        /// Sanitized reason.
        message: String,
    },
    /// Acknowledgment was already settled.
    #[error("message acknowledgment is already settled: message_id={message_id}")]
    AlreadySettled {
        /// Stable message identifier.
        message_id: String,
    },
    /// A broker cannot represent the requested acknowledgment operation.
    #[error("acknowledgment operation {operation} is unsupported by broker {broker}")]
    UnsupportedAcknowledgment {
        /// Stable broker configuration value.
        broker: String,
        /// Requested operation.
        operation: &'static str,
    },
    /// Serialization or deserialization failed.
    #[error("MQ serialization failed: {message}")]
    Serialization {
        /// Sanitized reason.
        message: String,
    },
    /// A message handler failed.
    #[error("MQ handler failed: {message}")]
    Handler {
        /// Sanitized reason.
        message: String,
    },
}

impl From<serde_json::Error> for MqError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serialization {
            message: value.to_string(),
        }
    }
}
