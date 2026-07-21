//! Runtime service registration facade.

use std::sync::Arc;

use crate::DddResult;
use crate::context::{Contexts, Registry};

/// Registers and resolves framework/runtime services with task-local-first semantics.
pub struct RuntimeRegistry;

impl RuntimeRegistry {
    /// Builds a stable runtime SPI key.
    pub fn key(name: &str) -> String {
        format!("ddd4r.runtime.{name}")
    }

    /// Registers a process-wide runtime service exactly once.
    pub fn register<T>(name: &str, service: Arc<T>) -> DddResult<()>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        Contexts::register(Self::key(name), service)
    }

    /// Registers a service in an explicit task-local registry.
    pub fn register_in<T>(registry: &Registry, name: &str, service: Arc<T>) -> DddResult<()>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        registry.register(Self::key(name), service)
    }

    /// Resolves the task-local override or process-wide default.
    pub fn get<T>(name: &str) -> DddResult<Option<Arc<T>>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        Contexts::get(&Self::key(name))
    }

    /// Resolves a runtime service or returns a stable typed error.
    pub fn get_or_err<T>(name: &str) -> DddResult<Arc<T>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        Contexts::get_or_err(&Self::key(name))
    }

    /// Removes a process-wide runtime service.
    pub fn unregister<T>(name: &str) -> DddResult<Option<Arc<T>>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        Contexts::global().remove(&Self::key(name))
    }
}
