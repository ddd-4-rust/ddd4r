//! Task-local and global service registries.

use std::any::{Any, TypeId, type_name};
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};

use dashmap::DashMap;

use crate::{DddError, DddResult};

type ErasedService = Arc<dyn Any + Send + Sync>;

#[derive(Clone, Debug, Eq)]
struct ServiceKey {
    name: Arc<str>,
    type_id: TypeId,
}

impl ServiceKey {
    fn of<T: ?Sized + 'static>(name: impl Into<Arc<str>>) -> Self {
        Self {
            name: name.into(),
            type_id: TypeId::of::<T>(),
        }
    }
}

impl PartialEq for ServiceKey {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.type_id == other.type_id
    }
}

impl Hash for ServiceKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.type_id.hash(state);
    }
}

/// A cloneable, thread-safe collection of typed services.
#[derive(Clone, Default)]
pub struct Registry {
    services: Arc<DashMap<ServiceKey, ErasedService>>,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Registry")
            .field("service_count", &self.services.len())
            .finish()
    }
}

impl Registry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a service exactly once.
    pub fn register<T>(&self, key: impl Into<Arc<str>>, service: Arc<T>) -> DddResult<()>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        let key = ServiceKey::of::<T>(key);
        let display_key = key.name.to_string();
        let erased: ErasedService = Arc::new(service);
        match self.services.entry(key) {
            dashmap::mapref::entry::Entry::Occupied(_) => Err(DddError::ServiceAlreadyRegistered {
                key: display_key,
                type_name: type_name::<T>(),
            }),
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(erased);
                Ok(())
            }
        }
    }

    /// Replaces a service, returning the previous value when present.
    pub fn replace<T>(&self, key: impl Into<Arc<str>>, service: Arc<T>) -> DddResult<Option<Arc<T>>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        let key = ServiceKey::of::<T>(key);
        let display_key = key.name.to_string();
        let erased: ErasedService = Arc::new(service);
        self.services
            .insert(key, erased)
            .map(|previous| {
                previous
                    .downcast_ref::<Arc<T>>()
                    .cloned()
                    .ok_or(DddError::ServiceTypeMismatch {
                        key: display_key,
                        expected: type_name::<T>(),
                    })
            })
            .transpose()
    }

    /// Returns a cloned service handle.
    pub fn get<T>(&self, key: &str) -> DddResult<Option<Arc<T>>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        let service_key = ServiceKey::of::<T>(Arc::<str>::from(key));
        self.services
            .get(&service_key)
            .map(|entry| {
                entry
                    .value()
                    .downcast_ref::<Arc<T>>()
                    .cloned()
                    .ok_or_else(|| DddError::ServiceTypeMismatch {
                        key: key.to_owned(),
                        expected: type_name::<T>(),
                    })
            })
            .transpose()
    }

    /// Removes a service from this registry.
    pub fn remove<T>(&self, key: &str) -> DddResult<Option<Arc<T>>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        let service_key = ServiceKey::of::<T>(Arc::<str>::from(key));
        self.services
            .remove(&service_key)
            .map(|(_, value)| {
                value.downcast_ref::<Arc<T>>().cloned().ok_or_else(|| {
                    DddError::ServiceTypeMismatch {
                        key: key.to_owned(),
                        expected: type_name::<T>(),
                    }
                })
            })
            .transpose()
    }

    /// Returns an isolated shallow copy of all service handles.
    pub fn snapshot(&self) -> Self {
        let snapshot = Self::new();
        for item in self.services.iter() {
            snapshot
                .services
                .insert(item.key().clone(), Arc::clone(item.value()));
        }
        snapshot
    }

    /// Returns the number of registered services.
    pub fn len(&self) -> usize {
        self.services.len()
    }

    /// Returns whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.services.is_empty()
    }

    /// Clears this registry. Intended for lifecycle shutdown and tests.
    pub fn clear(&self) {
        self.services.clear();
    }
}

static GLOBAL_REGISTRY: OnceLock<Registry> = OnceLock::new();

tokio::task_local! {
    static TASK_REGISTRY: Registry;
}

/// Facade implementing task-local-first, global-second lookup.
pub struct Contexts;

impl Contexts {
    /// Returns the process-wide registry.
    pub fn global() -> &'static Registry {
        GLOBAL_REGISTRY.get_or_init(Registry::new)
    }

    /// Registers a global service.
    pub fn register<T>(key: impl Into<Arc<str>>, service: Arc<T>) -> DddResult<()>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        Self::global().register(key, service)
    }

    /// Resolves a service from the current task before the global registry.
    pub fn get<T>(key: &str) -> DddResult<Option<Arc<T>>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        if let Ok(task_value) = TASK_REGISTRY.try_with(|registry| registry.get::<T>(key))
            && let Some(value) = task_value?
        {
            return Ok(Some(value));
        }
        Self::global().get(key)
    }

    /// Resolves a service or returns a stable error.
    pub fn get_or_err<T>(key: &str) -> DddResult<Arc<T>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        Self::get(key)?.ok_or_else(|| DddError::ServiceNotFound {
            key: key.to_owned(),
            type_name: type_name::<T>(),
        })
    }

    /// Creates a snapshot of the current effective task registry.
    pub fn current_snapshot() -> Registry {
        TASK_REGISTRY
            .try_with(Registry::snapshot)
            .unwrap_or_else(|_| Registry::new())
    }
}

/// Runs futures inside a Tokio task-local registry scope.
pub struct ContextScope;

impl ContextScope {
    /// Runs a future with the supplied task-local registry.
    pub async fn run<F>(registry: Registry, future: F) -> F::Output
    where
        F: Future,
    {
        TASK_REGISTRY.scope(registry, future).await
    }

    /// Runs a nested scope that inherits the current task's services.
    pub async fn nested<F, C>(configure: C, future: F) -> DddResult<F::Output>
    where
        F: Future,
        C: FnOnce(&Registry) -> DddResult<()>,
    {
        let registry = Contexts::current_snapshot();
        configure(&registry)?;
        Ok(Self::run(registry, future).await)
    }
}
