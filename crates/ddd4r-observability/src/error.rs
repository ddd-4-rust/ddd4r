//! Observability error contract.

use thiserror::Error;

/// Errors returned while installing or controlling observability.
#[derive(Debug, Error)]
pub enum ObservabilityError {
    /// A tracing filter directive is invalid.
    #[error("invalid tracing filter: {0}")]
    InvalidFilter(String),
    /// A process-wide tracing subscriber was already installed.
    #[error("the global tracing subscriber is already installed")]
    TracingAlreadyInstalled,
    /// A process-wide metrics recorder was already installed.
    #[error("the global metrics recorder is already installed")]
    MetricsAlreadyInstalled,
    /// A temporary filter was requested outside a Tokio runtime.
    #[error("temporary tracing filters require an active Tokio runtime")]
    TokioRuntimeUnavailable,
    /// A health component name is invalid.
    #[error("invalid health component name: {0}")]
    InvalidComponentName(String),
}

/// Result alias used by observability APIs.
pub type ObservabilityResult<T> = Result<T, ObservabilityError>;
