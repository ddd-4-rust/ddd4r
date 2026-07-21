//! Repository contracts, registry and aggregate compatibility facade.

use std::any::type_name;
use std::sync::Arc;

use futures::future::BoxFuture;

use crate::context::{Contexts, Registry};
use crate::domain::AggregateRoot;
use crate::query::{Page, Query};
use crate::{DddError, DddResult};

/// Persistence port for an aggregate root.
pub trait Repository<A>: Send + Sync + 'static
where
    A: AggregateRoot,
{
    /// Finds an aggregate by identifier.
    fn find_by_id<'a>(&'a self, id: &'a A::Id) -> BoxFuture<'a, DddResult<Option<A>>>;

    /// Inserts or updates an aggregate.
    fn save<'a>(&'a self, aggregate: &'a mut A) -> BoxFuture<'a, DddResult<()>>;

    /// Updates an aggregate by identifier.
    fn update_by_id<'a>(&'a self, aggregate: &'a mut A) -> BoxFuture<'a, DddResult<()>> {
        self.save(aggregate)
    }

    /// Deletes an aggregate by identifier.
    fn delete_by_id<'a>(&'a self, _id: &'a A::Id) -> BoxFuture<'a, DddResult<()>> {
        Box::pin(async { Err(DddError::unsupported("Repository::delete_by_id")) })
    }

    /// Finds all aggregates.
    fn find_all(&self) -> BoxFuture<'_, DddResult<Vec<A>>> {
        Box::pin(async { Err(DddError::unsupported("Repository::find_all")) })
    }

    /// Finds the first aggregate matching a query.
    fn find_first<'a>(&'a self, _query: &'a Query<A>) -> BoxFuture<'a, DddResult<Option<A>>> {
        Box::pin(async { Err(DddError::unsupported("Repository::find_first")) })
    }

    /// Lists aggregates matching a query.
    fn find_list<'a>(&'a self, _query: &'a Query<A>) -> BoxFuture<'a, DddResult<Vec<A>>> {
        Box::pin(async { Err(DddError::unsupported("Repository::find_list")) })
    }

    /// Pages aggregates matching a query.
    fn page<'a>(&'a self, _query: &'a Query<A>) -> BoxFuture<'a, DddResult<Page<A>>> {
        Box::pin(async { Err(DddError::unsupported("Repository::page")) })
    }

    /// Counts aggregates matching a query.
    fn count<'a>(&'a self, query: &'a Query<A>) -> BoxFuture<'a, DddResult<u64>> {
        Box::pin(async move {
            let count = self.find_list(query).await?.len();
            u64::try_from(count).map_err(|error| DddError::Adapter {
                adapter: "repository",
                message: error.to_string(),
            })
        })
    }
}

/// Typed repository registration and lookup.
pub struct RepositoryRegistry;

impl RepositoryRegistry {
    fn key<A: AggregateRoot>() -> String {
        format!("ddd4r.repository.{}", type_name::<A>())
    }

    /// Registers a process-wide repository.
    pub fn register<A>(repository: Arc<dyn Repository<A>>) -> DddResult<()>
    where
        A: AggregateRoot,
    {
        Contexts::register(Self::key::<A>(), repository)
    }

    /// Registers a repository in an explicit task-local registry.
    pub fn register_in<A>(registry: &Registry, repository: Arc<dyn Repository<A>>) -> DddResult<()>
    where
        A: AggregateRoot,
    {
        registry.register(Self::key::<A>(), repository)
    }

    /// Resolves a repository with task-local-first semantics.
    pub fn repository<A>() -> DddResult<Arc<dyn Repository<A>>>
    where
        A: AggregateRoot,
    {
        Contexts::get::<dyn Repository<A>>(&Self::key::<A>())?.ok_or(DddError::RepositoryNotFound {
            aggregate: type_name::<A>(),
        })
    }

    /// Removes a process-wide repository, primarily for lifecycle cleanup.
    pub fn unregister<A>() -> DddResult<Option<Arc<dyn Repository<A>>>>
    where
        A: AggregateRoot,
    {
        Contexts::global().remove::<dyn Repository<A>>(&Self::key::<A>())
    }
}

/// ddd4j-compatible aggregate persistence methods.
pub trait AggregateRootExt: AggregateRoot {
    /// Inserts or updates this aggregate.
    fn save(&mut self) -> BoxFuture<'_, DddResult<()>> {
        Box::pin(async move {
            let repository = RepositoryRegistry::repository::<Self>()?;
            repository.save(self).await
        })
    }

    /// Updates this aggregate by identifier.
    fn update(&mut self) -> BoxFuture<'_, DddResult<()>> {
        Box::pin(async move {
            let repository = RepositoryRegistry::repository::<Self>()?;
            repository.update_by_id(self).await
        })
    }

    /// Deletes this aggregate.
    fn delete(&self) -> BoxFuture<'_, DddResult<()>> {
        Box::pin(async move {
            let repository = RepositoryRegistry::repository::<Self>()?;
            repository.delete_by_id(self.id()).await
        })
    }

    /// Starts a strongly typed query for this aggregate type.
    fn query() -> Query<Self> {
        Query::new()
    }
}

impl<T> AggregateRootExt for T where T: AggregateRoot {}
