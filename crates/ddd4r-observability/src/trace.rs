//! Runtime tracing filter control.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::registry::Registry;
use tracing_subscriber::reload;

use crate::{ObservabilityError, ObservabilityResult};

/// Reloadable tracing filter with bounded temporary overrides.
#[derive(Clone)]
pub struct TraceController {
    handle: reload::Handle<EnvFilter, Registry>,
    current: Arc<RwLock<String>>,
    generation: Arc<AtomicU64>,
}

impl std::fmt::Debug for TraceController {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TraceController")
            .field("current", &self.current_filter())
            .field("generation", &self.generation.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl TraceController {
    pub(crate) fn new(handle: reload::Handle<EnvFilter, Registry>, initial_filter: String) -> Self {
        Self {
            handle,
            current: Arc::new(RwLock::new(initial_filter)),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Permanently replaces the active filter.
    pub fn reload(&self, directive: &str) -> ObservabilityResult<u64> {
        let filter = parse_filter(directive)?;
        self.handle
            .reload(filter)
            .map_err(|_| ObservabilityError::TracingAlreadyInstalled)?;
        directive.clone_into(&mut write_unpoisoned(&self.current));
        Ok(self.generation.fetch_add(1, Ordering::AcqRel) + 1)
    }

    /// Applies a filter and automatically restores the previous value.
    ///
    /// A newer reload supersedes the pending restoration, preventing an older
    /// timer from overwriting a more recent operator decision.
    pub fn reload_for(&self, directive: &str, ttl: Duration) -> ObservabilityResult<u64> {
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| ObservabilityError::TokioRuntimeUnavailable)?;
        let previous = self.current_filter();
        let generation = self.reload(directive)?;
        let controller = self.clone();
        runtime.spawn(async move {
            tokio::time::sleep(ttl).await;
            if controller.generation.load(Ordering::Acquire) == generation {
                let _ = controller.reload(&previous);
            }
        });
        Ok(generation)
    }

    /// Returns the current filter directive without exposing subscriber types.
    pub fn current_filter(&self) -> String {
        read_unpoisoned(&self.current).clone()
    }
}

pub(crate) fn parse_filter(directive: &str) -> ObservabilityResult<EnvFilter> {
    EnvFilter::try_new(directive)
        .map_err(|error| ObservabilityError::InvalidFilter(error.to_string()))
}

fn read_unpoisoned<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write_unpoisoned<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
