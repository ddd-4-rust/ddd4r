//! Privileged diagnostics authorization, auditing, and capability metadata.
//!
//! Profiler adapters require a [`DiagnosticGrant`] issued by
//! [`DiagnosticsGate`]. They do not accept raw credentials and cannot create a
//! grant themselves.

#![forbid(unsafe_code)]

mod audit;
mod authorization;
mod error;
mod external;

pub use audit::{
    DiagnosticAuditEvent, DiagnosticAuditOutcome, DiagnosticAuditSink, TracingDiagnosticAuditSink,
};
pub use authorization::{
    DiagnosticGrant, DiagnosticOperation, DiagnosticPolicy, DiagnosticPrincipal, DiagnosticRequest,
    DiagnosticsGate,
};
pub use error::{DiagnosticError, DiagnosticResult};
pub use external::{ExternalDiagnosticTool, ExternalToolKind, ExternalToolchain};

/// Common imports for management and profiler adapters.
pub mod prelude {
    pub use crate::{
        DiagnosticAuditEvent, DiagnosticAuditOutcome, DiagnosticAuditSink, DiagnosticError,
        DiagnosticGrant, DiagnosticOperation, DiagnosticPolicy, DiagnosticPrincipal,
        DiagnosticRequest, DiagnosticResult, DiagnosticsGate, ExternalDiagnosticTool,
        ExternalToolKind, ExternalToolchain, TracingDiagnosticAuditSink,
    };
}
