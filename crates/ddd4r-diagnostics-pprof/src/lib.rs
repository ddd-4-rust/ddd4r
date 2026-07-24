//! Authorized process CPU profiling.
//!
//! Applications opt in by depending on this crate. A profile session can only
//! start with a CPU grant issued by `ddd4r-diagnostics`.

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicBool, Ordering};

use ddd4r_diagnostics::{DiagnosticError, DiagnosticGrant, DiagnosticOperation, DiagnosticResult};

#[cfg(unix)]
use pprof::ProfilerGuard;
#[cfg(unix)]
use pprof::protos::Message;

static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Factory for bounded CPU profile sessions.
#[derive(Debug, Default)]
pub struct CpuProfiler;

impl CpuProfiler {
    /// Captures a process-wide profile for exactly the authorized duration.
    ///
    /// The frequency is limited to 1–1000 Hz. Only one session may be active.
    #[cfg_attr(not(unix), allow(clippy::unused_async))]
    pub async fn capture(grant: DiagnosticGrant, frequency_hz: u32) -> DiagnosticResult<Vec<u8>> {
        grant.ensure(DiagnosticOperation::CpuProfile)?;
        if !(1..=1_000).contains(&frequency_hz) {
            return Err(DiagnosticError::InvalidConfiguration(
                "CPU sampling frequency must be between 1 and 1000 Hz".to_owned(),
            ));
        }

        #[cfg(unix)]
        {
            let duration = grant.duration();
            let session = Self::start(grant, frequency_hz)?;
            tokio::time::sleep(duration).await;
            session.finish()
        }

        #[cfg(not(unix))]
        {
            grant.failed("unsupported_platform");
            Err(DiagnosticError::Unavailable(
                "pprof is supported by ddd4r only on Unix targets".to_owned(),
            ))
        }
    }

    #[cfg(unix)]
    fn start(grant: DiagnosticGrant, frequency_hz: u32) -> DiagnosticResult<CpuProfileSession> {
        if ACTIVE.swap(true, Ordering::AcqRel) {
            return Err(DiagnosticError::AlreadyActive("CPU profile"));
        }

        let frequency = i32::try_from(frequency_hz).map_err(|error| {
            ACTIVE.store(false, Ordering::Release);
            DiagnosticError::InvalidConfiguration(error.to_string())
        })?;
        let guard = pprof::ProfilerGuardBuilder::default()
            .frequency(frequency)
            .build()
            .map_err(|error| {
                ACTIVE.store(false, Ordering::Release);
                grant.failed("pprof_start_failed");
                DiagnosticError::Backend(error.to_string())
            })?;
        Ok(CpuProfileSession {
            grant,
            guard: Some(guard),
            finalized: false,
        })
    }
}

/// Active process-wide CPU profile.
#[cfg(unix)]
struct CpuProfileSession {
    grant: DiagnosticGrant,
    guard: Option<ProfilerGuard<'static>>,
    finalized: bool,
}

#[cfg(unix)]
impl std::fmt::Debug for CpuProfileSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CpuProfileSession")
            .field("grant", &self.grant)
            .field("finalized", &self.finalized)
            .finish_non_exhaustive()
    }
}

#[cfg(unix)]
impl CpuProfileSession {
    /// Stops sampling and returns a protobuf pprof profile.
    pub fn finish(mut self) -> DiagnosticResult<Vec<u8>> {
        let result = self.build_profile();
        self.finalized = true;
        self.guard.take();
        ACTIVE.store(false, Ordering::Release);
        match result {
            Ok(profile) => {
                self.grant.completed();
                Ok(profile)
            }
            Err(error) => {
                self.grant.failed("pprof_report_failed");
                Err(error)
            }
        }
    }

    fn build_profile(&self) -> DiagnosticResult<Vec<u8>> {
        let guard = self.guard.as_ref().ok_or_else(|| {
            DiagnosticError::Backend("CPU profiler guard is unavailable".to_owned())
        })?;
        let report = guard
            .report()
            .build()
            .map_err(|error| DiagnosticError::Backend(error.to_string()))?;
        let profile = report
            .pprof()
            .map_err(|error| DiagnosticError::Backend(error.to_string()))?;
        let mut bytes = Vec::new();
        profile
            .encode(&mut bytes)
            .map_err(|error| DiagnosticError::Backend(error.to_string()))?;
        Ok(bytes)
    }
}

#[cfg(unix)]
impl Drop for CpuProfileSession {
    fn drop(&mut self) {
        self.guard.take();
        ACTIVE.store(false, Ordering::Release);
        if !self.finalized {
            self.grant.failed("pprof_session_cancelled");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ddd4r_diagnostics::{
        DiagnosticOperation, DiagnosticPrincipal, DiagnosticRequest, DiagnosticsGate,
    };

    use super::*;

    fn grant() -> DiagnosticGrant {
        DiagnosticsGate::default()
            .authorize(&DiagnosticRequest {
                principal: DiagnosticPrincipal::new(
                    "operator",
                    [DiagnosticOperation::CpuProfile.permission()],
                ),
                operation: DiagnosticOperation::CpuProfile,
                peer_ip: "127.0.0.1".parse().expect("valid IP"),
                requested_duration: Duration::from_millis(100),
            })
            .expect("authorized")
    }

    #[tokio::test]
    async fn rejects_unsafe_sampling_frequency_before_starting() {
        assert!(matches!(
            CpuProfiler::capture(grant(), 0).await,
            Err(DiagnosticError::InvalidConfiguration(_))
        ));
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn authorized_capture_produces_pprof_bytes_and_stops_at_ttl() {
        let started = std::time::Instant::now();
        let workload = tokio::task::spawn_blocking(|| {
            let deadline = std::time::Instant::now() + Duration::from_millis(80);
            let mut value = 0_u64;
            while std::time::Instant::now() < deadline {
                value = std::hint::black_box(value.wrapping_add(1));
            }
            value
        });
        let profile = CpuProfiler::capture(grant(), 99)
            .await
            .expect("bounded pprof capture");
        assert!(!profile.is_empty());
        assert!(started.elapsed() >= Duration::from_millis(100));
        std::hint::black_box(workload.await.expect("workload completes"));
    }
}
