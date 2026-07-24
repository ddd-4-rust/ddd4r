//! Diagnostics error contract.

use std::time::Duration;

use thiserror::Error;

use crate::DiagnosticOperation;

/// Errors returned before or during privileged diagnostics.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DiagnosticError {
    /// The authenticated principal lacks the operation permission.
    #[error("diagnostic permission denied for {operation:?}")]
    PermissionDenied {
        /// Requested operation.
        operation: DiagnosticOperation,
    },
    /// The request did not originate from a loopback management channel.
    #[error("diagnostic operation requires a loopback management channel")]
    LoopbackRequired,
    /// The requested duration exceeds the bounded policy.
    #[error("diagnostic duration {requested:?} exceeds maximum {maximum:?}")]
    DurationExceeded {
        /// Requested duration.
        requested: Duration,
        /// Maximum allowed duration.
        maximum: Duration,
    },
    /// Zero-duration diagnostics are never useful and often indicate misuse.
    #[error("diagnostic duration must be greater than zero")]
    InvalidDuration,
    /// A grant was issued for another operation.
    #[error("diagnostic grant is not valid for {operation:?}")]
    OperationMismatch {
        /// Operation attempted by an adapter.
        operation: DiagnosticOperation,
    },
    /// The grant expired before the profiler operation started.
    #[error("diagnostic grant expired")]
    GrantExpired,
    /// Another profiler session already owns the process-wide profiler.
    #[error("a {0} diagnostic session is already active")]
    AlreadyActive(&'static str),
    /// The feature was not compiled or is unsupported on this platform.
    #[error("diagnostic capability is unavailable: {0}")]
    Unavailable(String),
    /// A profiler backend failed.
    #[error("diagnostic backend failed: {0}")]
    Backend(String),
    /// A profiler configuration is outside safe bounds.
    #[error("invalid diagnostic configuration: {0}")]
    InvalidConfiguration(String),
}

/// Result alias used by diagnostics APIs.
pub type DiagnosticResult<T> = Result<T, DiagnosticError>;
