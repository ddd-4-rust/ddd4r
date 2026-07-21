//! Stable error types shared by the core contracts.

use thiserror::Error;

/// Result type returned by ddd4r core APIs.
pub type DddResult<T> = Result<T, DddError>;

/// Framework-neutral error model.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DddError {
    /// A service was not found in task-local or global context.
    #[error("service not found: key={key}, type={type_name}")]
    ServiceNotFound {
        /// Stable service key.
        key: String,
        /// Requested Rust type name.
        type_name: &'static str,
    },
    /// A service was registered more than once in the same registry.
    #[error("service already registered: key={key}, type={type_name}")]
    ServiceAlreadyRegistered {
        /// Stable service key.
        key: String,
        /// Registered Rust type name.
        type_name: &'static str,
    },
    /// An erased service did not contain the expected concrete value.
    #[error("service type mismatch: key={key}, expected={expected}")]
    ServiceTypeMismatch {
        /// Stable service key.
        key: String,
        /// Expected Rust type name.
        expected: &'static str,
    },
    /// No repository was registered for an aggregate.
    #[error("repository not found for aggregate {aggregate}")]
    RepositoryNotFound {
        /// Aggregate Rust type name.
        aggregate: &'static str,
    },
    /// An adapter does not implement an optional operation.
    #[error("operation is not supported by this adapter: {operation}")]
    UnsupportedOperation {
        /// Stable operation name.
        operation: &'static str,
    },
    /// No command executor accepted the command type.
    #[error("no executor found for command {command}")]
    CommandExecutorNotFound {
        /// Command Rust type name.
        command: &'static str,
    },
    /// More than one executor was registered for a command type.
    #[error("multiple executors found for command {command}")]
    MultipleCommandExecutors {
        /// Command Rust type name.
        command: &'static str,
    },
    /// A command result could not be converted to the requested type.
    #[error("command result type mismatch: expected {expected}")]
    CommandResultTypeMismatch {
        /// Expected Rust type name.
        expected: &'static str,
    },
    /// Optimistic locking rejected a stale aggregate version.
    #[error("optimistic lock conflict for {aggregate}: expected={expected}, actual={actual}")]
    OptimisticLockConflict {
        /// Aggregate Rust type name.
        aggregate: &'static str,
        /// Expected version.
        expected: u64,
        /// Persisted version.
        actual: u64,
    },
    /// Serialization failed at a framework boundary.
    #[error("serialization failed: {message}")]
    Serialization {
        /// Sanitized serialization error.
        message: String,
    },
    /// An adapter reported an implementation-specific failure.
    #[error("adapter {adapter} failed: {message}")]
    Adapter {
        /// Stable adapter name.
        adapter: &'static str,
        /// Sanitized error message.
        message: String,
    },
}

impl DddError {
    /// Creates an unsupported-operation error.
    pub const fn unsupported(operation: &'static str) -> Self {
        Self::UnsupportedOperation { operation }
    }
}

impl From<serde_json::Error> for DddError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serialization {
            message: value.to_string(),
        }
    }
}
