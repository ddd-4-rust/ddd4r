//! OTLP adapter error contract.

use thiserror::Error;

/// Errors returned while configuring or operating OTLP export.
#[derive(Debug, Error)]
pub enum OtlpError {
    /// A bounded OTLP setting is invalid.
    #[error("invalid OTLP configuration: {0}")]
    InvalidConfiguration(String),
    /// An exporter could not be constructed.
    #[error("failed to build OTLP exporter: {0}")]
    ExporterBuild(String),
    /// The local observability runtime could not be installed.
    #[error(transparent)]
    Observability(#[from] ddd4r_observability::ObservabilityError),
    /// One or more providers failed to flush.
    #[error("failed to flush OpenTelemetry providers: {0}")]
    Flush(String),
    /// One or more providers failed to shut down.
    #[error("failed to shut down OpenTelemetry providers: {0}")]
    Shutdown(String),
    /// A tracing span rejected an extracted remote parent.
    #[error("failed to attach the remote trace parent: {0}")]
    ParentContext(String),
}

/// Result alias used by the OTLP adapter.
pub type OtlpResult<T> = Result<T, OtlpError>;
