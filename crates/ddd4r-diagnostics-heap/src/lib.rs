//! Authorized jemalloc heap profiling.
//!
//! This crate is Linux-only. Depending on it explicitly selects jemalloc.
//! Applications must start with
//! `MALLOC_CONF=prof:true,prof_active:false,lg_prof_sample:19`.

#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use ddd4r_diagnostics::{DiagnosticError, DiagnosticGrant, DiagnosticOperation, DiagnosticResult};

#[cfg(target_os = "linux")]
#[global_allocator]
static ALLOCATOR: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(target_os = "linux")]
static ACTIVE: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "linux")]
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Authorized heap profiler control.
#[derive(Debug, Default)]
pub struct HeapProfiler;

impl HeapProfiler {
    /// Whether a Linux heap profiler implementation was compiled.
    pub const fn compiled() -> bool {
        cfg!(target_os = "linux")
    }

    /// Activates allocation sampling.
    #[cfg_attr(not(target_os = "linux"), allow(clippy::unused_async))]
    pub async fn activate(grant: DiagnosticGrant) -> DiagnosticResult<()> {
        grant.ensure(DiagnosticOperation::HeapProfile)?;
        #[cfg(target_os = "linux")]
        {
            if ACTIVE.swap(true, Ordering::AcqRel) {
                return Err(DiagnosticError::AlreadyActive("heap profile"));
            }
            let control = jemalloc_pprof::PROF_CTL.as_ref().ok_or_else(|| {
                ACTIVE.store(false, Ordering::Release);
                grant.failed("jemalloc_profiling_disabled");
                DiagnosticError::Unavailable(
                    "jemalloc profiling requires MALLOC_CONF=prof:true".to_owned(),
                )
            })?;
            control.lock().await.activate().map_err(|error| {
                ACTIVE.store(false, Ordering::Release);
                grant.failed("jemalloc_activation_failed");
                DiagnosticError::Backend(error.to_string())
            })?;
            let generation = GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
            let duration = grant.duration();
            tokio::spawn(async move {
                tokio::time::sleep(duration).await;
                if GENERATION.load(Ordering::Acquire) == generation
                    && ACTIVE.swap(false, Ordering::AcqRel)
                {
                    let _ = control.lock().await.deactivate();
                }
            });
            grant.completed();
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        {
            grant.failed("unsupported_platform");
            Err(DiagnosticError::Unavailable(
                "jemalloc heap profiling is supported only on Linux".to_owned(),
            ))
        }
    }

    /// Deactivates allocation sampling and resets accumulated samples.
    #[cfg_attr(not(target_os = "linux"), allow(clippy::unused_async))]
    pub async fn deactivate(grant: DiagnosticGrant) -> DiagnosticResult<()> {
        grant.ensure(DiagnosticOperation::HeapProfile)?;
        #[cfg(target_os = "linux")]
        {
            if !ACTIVE.swap(false, Ordering::AcqRel) {
                grant.failed("jemalloc_profiling_inactive");
                return Err(DiagnosticError::Unavailable(
                    "jemalloc heap profiling is not active".to_owned(),
                ));
            }
            GENERATION.fetch_add(1, Ordering::AcqRel);
            let control = jemalloc_pprof::PROF_CTL.as_ref().ok_or_else(|| {
                grant.failed("jemalloc_profiling_disabled");
                DiagnosticError::Unavailable(
                    "jemalloc profiling requires MALLOC_CONF=prof:true".to_owned(),
                )
            })?;
            control.lock().await.deactivate().map_err(|error| {
                grant.failed("jemalloc_deactivation_failed");
                DiagnosticError::Backend(error.to_string())
            })?;
            grant.completed();
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        {
            grant.failed("unsupported_platform");
            Err(DiagnosticError::Unavailable(
                "jemalloc heap profiling is supported only on Linux".to_owned(),
            ))
        }
    }

    /// Returns a gzipped protobuf pprof heap snapshot.
    #[cfg_attr(not(target_os = "linux"), allow(clippy::unused_async))]
    pub async fn snapshot(grant: DiagnosticGrant) -> DiagnosticResult<Vec<u8>> {
        grant.ensure(DiagnosticOperation::HeapProfile)?;
        #[cfg(target_os = "linux")]
        {
            if !ACTIVE.load(Ordering::Acquire) {
                grant.failed("jemalloc_profiling_inactive");
                return Err(DiagnosticError::Unavailable(
                    "jemalloc heap profiling is not active".to_owned(),
                ));
            }
            let control = jemalloc_pprof::PROF_CTL.as_ref().ok_or_else(|| {
                grant.failed("jemalloc_profiling_disabled");
                DiagnosticError::Unavailable(
                    "jemalloc profiling requires MALLOC_CONF=prof:true".to_owned(),
                )
            })?;
            let mut control = control.lock().await;
            if !control.activated() {
                ACTIVE.store(false, Ordering::Release);
                grant.failed("jemalloc_profiling_inactive");
                return Err(DiagnosticError::Unavailable(
                    "jemalloc profiling was compiled but is not active".to_owned(),
                ));
            }
            let profile = control.dump_pprof().map_err(|error| {
                grant.failed("jemalloc_snapshot_failed");
                DiagnosticError::Backend(error.to_string())
            })?;
            grant.completed();
            Ok(profile)
        }
        #[cfg(not(target_os = "linux"))]
        {
            grant.failed("unsupported_platform");
            Err(DiagnosticError::Unavailable(
                "jemalloc heap profiling is supported only on Linux".to_owned(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    use std::time::Duration;

    #[cfg(target_os = "linux")]
    use ddd4r_diagnostics::{
        DiagnosticGrant, DiagnosticOperation, DiagnosticPrincipal, DiagnosticRequest,
        DiagnosticsGate,
    };

    #[test]
    fn platform_capability_is_explicit() {
        assert_eq!(super::HeapProfiler::compiled(), cfg!(target_os = "linux"));
    }

    #[cfg(target_os = "linux")]
    fn grant() -> DiagnosticGrant {
        DiagnosticsGate::default()
            .authorize(&DiagnosticRequest {
                principal: DiagnosticPrincipal::new(
                    "ci-operator",
                    [DiagnosticOperation::HeapProfile.permission()],
                ),
                operation: DiagnosticOperation::HeapProfile,
                peer_ip: "127.0.0.1".parse().expect("valid loopback IP"),
                requested_duration: Duration::from_secs(30),
            })
            .expect("authorized heap profiling")
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn explicitly_enabled_heap_profile_round_trip() {
        if std::env::var("DDD4R_TEST_HEAP_PROFILER").as_deref() != Ok("1") {
            return;
        }

        super::HeapProfiler::activate(grant())
            .await
            .expect("activate heap profiling");
        let allocations = (0..32)
            .map(|index| vec![u8::try_from(index).expect("bounded index"); 512 * 1_024])
            .collect::<Vec<_>>();
        std::hint::black_box(&allocations);
        let profile = super::HeapProfiler::snapshot(grant())
            .await
            .expect("capture heap profile");
        assert!(!profile.is_empty());
        super::HeapProfiler::deactivate(grant())
            .await
            .expect("deactivate heap profiling");
    }
}
