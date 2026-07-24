//! Diagnostics security audit contract.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::DiagnosticOperation;

/// Outcome of one authorization or execution event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticAuditOutcome {
    /// The operation was authorized.
    Authorized,
    /// Authorization was rejected.
    Denied,
    /// The profiler operation completed.
    Completed,
    /// The profiler backend failed.
    Failed,
}

/// Secret-free audit event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticAuditEvent {
    /// Authenticated operator identity.
    pub subject: String,
    /// Requested diagnostic capability.
    pub operation: DiagnosticOperation,
    /// Authorization or execution outcome.
    pub outcome: DiagnosticAuditOutcome,
    /// Stable machine-readable reason code.
    pub reason_code: &'static str,
    /// Event time in Unix milliseconds.
    pub occurred_at_ms: u128,
}

impl DiagnosticAuditEvent {
    pub(crate) fn new(
        subject: impl Into<String>,
        operation: DiagnosticOperation,
        outcome: DiagnosticAuditOutcome,
        reason_code: &'static str,
    ) -> Self {
        Self {
            subject: subject.into(),
            operation,
            outcome,
            reason_code,
            occurred_at_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |duration| duration.as_millis()),
        }
    }
}

/// Sink implemented by audit storage or security event adapters.
pub trait DiagnosticAuditSink: Send + Sync {
    /// Records one secret-free event.
    fn record(&self, event: DiagnosticAuditEvent);
}

/// Default audit sink emitting structured tracing events.
#[derive(Debug, Default)]
pub struct TracingDiagnosticAuditSink;

impl DiagnosticAuditSink for TracingDiagnosticAuditSink {
    fn record(&self, event: DiagnosticAuditEvent) {
        tracing::info!(
            target: "ddd4r.security.diagnostics",
            subject = %event.subject,
            operation = ?event.operation,
            outcome = ?event.outcome,
            reason_code = event.reason_code,
            occurred_at_ms = event.occurred_at_ms,
            "privileged diagnostic audit"
        );
    }
}
