//! Permission, network, duration, and grant enforcement.

use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::{
    DiagnosticAuditEvent, DiagnosticAuditOutcome, DiagnosticAuditSink, DiagnosticError,
    DiagnosticResult, TracingDiagnosticAuditSink,
};

/// Privileged diagnostic operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DiagnosticOperation {
    /// Temporarily change the tracing filter.
    TraceReload,
    /// Collect a CPU pprof profile.
    CpuProfile,
    /// Expose Tokio task diagnostics.
    AsyncTasks,
    /// Collect a jemalloc heap profile.
    HeapProfile,
    /// Read external profiler guidance and symbol metadata.
    ExternalToolAdvice,
}

impl DiagnosticOperation {
    /// Required application permission.
    pub const fn permission(self) -> &'static str {
        match self {
            Self::TraceReload => "ddd4r:diagnostics:trace",
            Self::CpuProfile => "ddd4r:diagnostics:cpu",
            Self::AsyncTasks => "ddd4r:diagnostics:tasks",
            Self::HeapProfile => "ddd4r:diagnostics:heap",
            Self::ExternalToolAdvice => "ddd4r:diagnostics:external",
        }
    }
}

/// Authenticated operator supplied by the management transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticPrincipal {
    subject: String,
    permissions: BTreeSet<String>,
}

impl DiagnosticPrincipal {
    /// Builds a principal from an already authenticated identity.
    pub fn new(
        subject: impl Into<String>,
        permissions: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            subject: subject.into(),
            permissions: permissions.into_iter().map(Into::into).collect(),
        }
    }

    /// Stable operator identity.
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// Returns whether the exact permission is present.
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }
}

/// Authorization request created by a management adapter.
#[derive(Debug, Clone)]
pub struct DiagnosticRequest {
    /// Authenticated operator.
    pub principal: DiagnosticPrincipal,
    /// Requested profiler/control operation.
    pub operation: DiagnosticOperation,
    /// Peer IP of the management connection.
    pub peer_ip: IpAddr,
    /// Maximum lifetime requested for the operation.
    pub requested_duration: Duration,
}

/// Bounded diagnostics policy.
#[derive(Debug, Clone)]
pub struct DiagnosticPolicy {
    loopback_only: bool,
    maximum_durations: BTreeMap<DiagnosticOperation, Duration>,
}

impl Default for DiagnosticPolicy {
    fn default() -> Self {
        Self {
            loopback_only: true,
            maximum_durations: BTreeMap::from([
                (DiagnosticOperation::TraceReload, Duration::from_mins(10)),
                (DiagnosticOperation::CpuProfile, Duration::from_mins(1)),
                (DiagnosticOperation::AsyncTasks, Duration::from_hours(1)),
                (DiagnosticOperation::HeapProfile, Duration::from_mins(1)),
                (
                    DiagnosticOperation::ExternalToolAdvice,
                    Duration::from_mins(1),
                ),
            ]),
        }
    }
}

impl DiagnosticPolicy {
    /// Allows a secured remote management transport.
    pub fn allow_remote(mut self) -> Self {
        self.loopback_only = false;
        self
    }

    /// Overrides the maximum duration for one capability.
    pub fn with_maximum(mut self, operation: DiagnosticOperation, maximum: Duration) -> Self {
        self.maximum_durations.insert(operation, maximum);
        self
    }

    fn maximum(&self, operation: DiagnosticOperation) -> Duration {
        self.maximum_durations
            .get(&operation)
            .copied()
            .unwrap_or(Duration::from_secs(30))
    }
}

/// Sole issuer of profiler grants.
#[derive(Clone)]
pub struct DiagnosticsGate {
    policy: DiagnosticPolicy,
    audit: Arc<dyn DiagnosticAuditSink>,
}

impl std::fmt::Debug for DiagnosticsGate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DiagnosticsGate")
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl Default for DiagnosticsGate {
    fn default() -> Self {
        Self {
            policy: DiagnosticPolicy::default(),
            audit: Arc::new(TracingDiagnosticAuditSink),
        }
    }
}

impl DiagnosticsGate {
    /// Uses the supplied policy and audit sink.
    pub fn new(policy: DiagnosticPolicy, audit: Arc<dyn DiagnosticAuditSink>) -> Self {
        Self { policy, audit }
    }

    /// Authorizes and bounds one operation.
    pub fn authorize(&self, request: &DiagnosticRequest) -> DiagnosticResult<DiagnosticGrant> {
        let denied = |error: DiagnosticError, reason_code: &'static str| {
            self.audit.record(DiagnosticAuditEvent::new(
                request.principal.subject(),
                request.operation,
                DiagnosticAuditOutcome::Denied,
                reason_code,
            ));
            Err(error)
        };

        if !request
            .principal
            .has_permission(request.operation.permission())
        {
            return denied(
                DiagnosticError::PermissionDenied {
                    operation: request.operation,
                },
                "permission_denied",
            );
        }
        if self.policy.loopback_only && !request.peer_ip.is_loopback() {
            return denied(DiagnosticError::LoopbackRequired, "loopback_required");
        }
        if request.requested_duration.is_zero() {
            return denied(DiagnosticError::InvalidDuration, "invalid_duration");
        }
        let maximum = self.policy.maximum(request.operation);
        if request.requested_duration > maximum {
            return denied(
                DiagnosticError::DurationExceeded {
                    requested: request.requested_duration,
                    maximum,
                },
                "duration_exceeded",
            );
        }

        self.audit.record(DiagnosticAuditEvent::new(
            request.principal.subject(),
            request.operation,
            DiagnosticAuditOutcome::Authorized,
            "authorized",
        ));
        Ok(DiagnosticGrant {
            subject: request.principal.subject().to_owned(),
            operation: request.operation,
            duration: request.requested_duration,
            expires_at: Instant::now() + request.requested_duration,
            audit: self.audit.clone(),
        })
    }
}

/// Unforgeable authorization proof required by profiler adapters.
pub struct DiagnosticGrant {
    subject: String,
    operation: DiagnosticOperation,
    duration: Duration,
    expires_at: Instant,
    audit: Arc<dyn DiagnosticAuditSink>,
}

impl std::fmt::Debug for DiagnosticGrant {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DiagnosticGrant")
            .field("subject", &self.subject)
            .field("operation", &self.operation)
            .field("duration", &self.duration)
            .finish_non_exhaustive()
    }
}

impl DiagnosticGrant {
    /// Validates that a backend is consuming the correct, unexpired grant.
    pub fn ensure(&self, operation: DiagnosticOperation) -> DiagnosticResult<()> {
        if self.operation != operation {
            return Err(DiagnosticError::OperationMismatch { operation });
        }
        if Instant::now() >= self.expires_at {
            return Err(DiagnosticError::GrantExpired);
        }
        Ok(())
    }

    /// Authorized duration.
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    /// Records profiler completion without exposing the audit sink.
    pub fn completed(&self) {
        self.audit.record(DiagnosticAuditEvent::new(
            &self.subject,
            self.operation,
            DiagnosticAuditOutcome::Completed,
            "completed",
        ));
    }

    /// Records a backend failure.
    pub fn failed(&self, reason_code: &'static str) {
        self.audit.record(DiagnosticAuditEvent::new(
            &self.subject,
            self.operation,
            DiagnosticAuditOutcome::Failed,
            reason_code,
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct AuditLog(Mutex<Vec<DiagnosticAuditEvent>>);

    impl DiagnosticAuditSink for AuditLog {
        fn record(&self, event: DiagnosticAuditEvent) {
            self.0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(event);
        }
    }

    fn request(operation: DiagnosticOperation) -> DiagnosticRequest {
        DiagnosticRequest {
            principal: DiagnosticPrincipal::new("operator-1", [operation.permission()]),
            operation,
            peer_ip: "127.0.0.1".parse().expect("valid loopback IP"),
            requested_duration: Duration::from_secs(10),
        }
    }

    #[test]
    fn valid_loopback_permission_issues_bound_grant() {
        let audit = Arc::new(AuditLog::default());
        let gate = DiagnosticsGate::new(DiagnosticPolicy::default(), audit.clone());
        let grant = gate
            .authorize(&request(DiagnosticOperation::CpuProfile))
            .expect("authorized request");

        grant
            .ensure(DiagnosticOperation::CpuProfile)
            .expect("matching grant");
        assert_eq!(
            audit
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)[0]
                .outcome,
            DiagnosticAuditOutcome::Authorized
        );
    }

    #[test]
    fn remote_and_wrong_permission_are_denied() {
        let gate = DiagnosticsGate::default();
        let mut remote = request(DiagnosticOperation::HeapProfile);
        remote.peer_ip = "192.0.2.1".parse().expect("valid remote IP");
        assert_eq!(
            gate.authorize(&remote).expect_err("remote request denied"),
            DiagnosticError::LoopbackRequired
        );

        let mut wrong = request(DiagnosticOperation::HeapProfile);
        wrong.principal = DiagnosticPrincipal::new("operator-1", ["other"]);
        assert_eq!(
            gate.authorize(&wrong).expect_err("wrong permission denied"),
            DiagnosticError::PermissionDenied {
                operation: DiagnosticOperation::HeapProfile
            }
        );
    }

    #[test]
    fn duration_and_operation_are_enforced() {
        let gate = DiagnosticsGate::default();
        let mut too_long = request(DiagnosticOperation::CpuProfile);
        too_long.requested_duration = Duration::from_secs(61);
        assert!(matches!(
            gate.authorize(&too_long),
            Err(DiagnosticError::DurationExceeded { .. })
        ));

        let grant = gate
            .authorize(&request(DiagnosticOperation::CpuProfile))
            .expect("authorized request");
        assert_eq!(
            grant
                .ensure(DiagnosticOperation::HeapProfile)
                .expect_err("grant cannot cross operations"),
            DiagnosticError::OperationMismatch {
                operation: DiagnosticOperation::HeapProfile
            }
        );
    }
}
