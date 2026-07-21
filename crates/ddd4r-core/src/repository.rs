//! Repository contracts, registry and aggregate compatibility facade.

use std::any::type_name;
use std::sync::Arc;

use futures::future::BoxFuture;
use serde_json::{Map, Value};

use crate::context::{Contexts, Registry};
use crate::domain::AggregateRoot;
use crate::query::{Page, Query};
use crate::{DddError, DddResult};

/// Framework-neutral row returned by projection/map queries.
pub type RepositoryRow = Map<String, Value>;

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

    /// Inserts a new aggregate or updates an existing aggregate.
    fn insert_or_update<'a>(&'a self, aggregate: &'a mut A) -> BoxFuture<'a, DddResult<()>> {
        self.save(aggregate)
    }

    /// Returns whether an aggregate exists for the supplied identifier.
    fn exists_by_id<'a>(&'a self, id: &'a A::Id) -> BoxFuture<'a, DddResult<bool>> {
        Box::pin(async move { Ok(self.find_by_id(id).await?.is_some()) })
    }

    /// Deletes an aggregate by identifier.
    fn delete_by_id<'a>(&'a self, _id: &'a A::Id) -> BoxFuture<'a, DddResult<()>> {
        Box::pin(async { Err(DddError::unsupported("Repository::delete_by_id")) })
    }

    /// Deletes an aggregate using its identifier.
    fn delete<'a>(&'a self, aggregate: &'a A) -> BoxFuture<'a, DddResult<()>> {
        self.delete_by_id(aggregate.id())
    }

    /// Deletes multiple aggregates by identifier.
    fn delete_by_ids<'a>(&'a self, ids: &'a [A::Id]) -> BoxFuture<'a, DddResult<u64>> {
        Box::pin(async move {
            for id in ids {
                self.delete_by_id(id).await?;
            }
            u64::try_from(ids.len()).map_err(|error| DddError::Adapter {
                adapter: "repository",
                message: error.to_string(),
            })
        })
    }

    /// Finds multiple aggregates by identifier while preserving input order.
    fn find_by_ids<'a>(&'a self, ids: &'a [A::Id]) -> BoxFuture<'a, DddResult<Vec<A>>> {
        Box::pin(async move {
            let mut aggregates = Vec::with_capacity(ids.len());
            for id in ids {
                if let Some(aggregate) = self.find_by_id(id).await? {
                    aggregates.push(aggregate);
                }
            }
            Ok(aggregates)
        })
    }

    /// Saves multiple aggregates in input order.
    fn save_batch<'a>(&'a self, aggregates: &'a mut [A]) -> BoxFuture<'a, DddResult<()>> {
        Box::pin(async move {
            for aggregate in aggregates {
                self.save(aggregate).await?;
            }
            Ok(())
        })
    }

    /// Updates multiple aggregates by identifier.
    fn update_batch_by_id<'a>(&'a self, aggregates: &'a mut [A]) -> BoxFuture<'a, DddResult<u64>> {
        Box::pin(async move {
            let count = aggregates.len();
            for aggregate in aggregates {
                self.update_by_id(aggregate).await?;
            }
            u64::try_from(count).map_err(|error| DddError::Adapter {
                adapter: "repository",
                message: error.to_string(),
            })
        })
    }

    /// Inserts or updates multiple aggregates.
    fn insert_or_update_batch<'a>(
        &'a self,
        aggregates: &'a mut [A],
    ) -> BoxFuture<'a, DddResult<u64>> {
        Box::pin(async move {
            let count = aggregates.len();
            for aggregate in aggregates {
                self.insert_or_update(aggregate).await?;
            }
            u64::try_from(count).map_err(|error| DddError::Adapter {
                adapter: "repository",
                message: error.to_string(),
            })
        })
    }

    /// Finds all aggregates.
    fn find_all(&self) -> BoxFuture<'_, DddResult<Vec<A>>> {
        Box::pin(async { Err(DddError::unsupported("Repository::find_all")) })
    }

    /// Finds the first aggregate without a query predicate.
    fn find_first_all(&self) -> BoxFuture<'_, DddResult<Option<A>>> {
        Box::pin(async move { Ok(self.find_all().await?.into_iter().next()) })
    }

    /// Counts all aggregates.
    fn count_all(&self) -> BoxFuture<'_, DddResult<u64>> {
        Box::pin(async move {
            let count = self.find_all().await?.len();
            u64::try_from(count).map_err(|error| DddError::Adapter {
                adapter: "repository",
                message: error.to_string(),
            })
        })
    }

    /// Returns whether the repository contains any aggregate.
    fn exists_all(&self) -> BoxFuture<'_, DddResult<bool>> {
        Box::pin(async move { Ok(self.count_all().await? > 0) })
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

    /// Returns whether any aggregate matches a query.
    fn exists<'a>(&'a self, query: &'a Query<A>) -> BoxFuture<'a, DddResult<bool>> {
        Box::pin(async move { Ok(self.count(query).await? > 0) })
    }

    /// Returns map-shaped rows for selected/grouped queries.
    fn maps<'a>(&'a self, _query: &'a Query<A>) -> BoxFuture<'a, DddResult<Vec<RepositoryRow>>> {
        Box::pin(async { Err(DddError::unsupported("Repository::maps")) })
    }

    /// Updates aggregates matching a query.
    fn update_where<'a>(
        &'a self,
        _aggregate: &'a A,
        _query: &'a Query<A>,
    ) -> BoxFuture<'a, DddResult<bool>> {
        Box::pin(async { Err(DddError::unsupported("Repository::update_where")) })
    }

    /// Deletes aggregates matching a query.
    fn delete_by_query<'a>(&'a self, _query: &'a Query<A>) -> BoxFuture<'a, DddResult<bool>> {
        Box::pin(async { Err(DddError::unsupported("Repository::delete_by_query")) })
    }

    /// Fills one aggregate from another data source.
    fn fill<'a>(
        &'a self,
        _query: &'a Query<A>,
        _aggregate: &'a mut A,
    ) -> BoxFuture<'a, DddResult<()>> {
        Box::pin(async { Ok(()) })
    }

    /// Fills multiple aggregates from another data source.
    fn fill_many<'a>(
        &'a self,
        query: &'a Query<A>,
        aggregates: &'a mut [A],
    ) -> BoxFuture<'a, DddResult<()>> {
        Box::pin(async move {
            for aggregate in aggregates {
                self.fill(query, aggregate).await?;
            }
            Ok(())
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

    /// Inserts or updates this aggregate.
    fn save_or_update(&mut self) -> BoxFuture<'_, DddResult<()>> {
        Box::pin(async move {
            let repository = RepositoryRegistry::repository::<Self>()?;
            repository.insert_or_update(self).await
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

    /// Finds one aggregate by identifier.
    fn get(id: &Self::Id) -> BoxFuture<'_, DddResult<Option<Self>>> {
        Box::pin(async move {
            let repository = RepositoryRegistry::repository::<Self>()?;
            repository.find_by_id(id).await
        })
    }

    /// Lists all aggregates.
    fn list_all() -> BoxFuture<'static, DddResult<Vec<Self>>> {
        Box::pin(async move {
            let repository = RepositoryRegistry::repository::<Self>()?;
            repository.find_all().await
        })
    }

    /// Counts all aggregates.
    fn count_all() -> BoxFuture<'static, DddResult<u64>> {
        Box::pin(async move {
            let repository = RepositoryRegistry::repository::<Self>()?;
            repository.count_all().await
        })
    }

    /// Saves multiple aggregates through the effective repository.
    fn save_batch(aggregates: &mut [Self]) -> BoxFuture<'_, DddResult<()>> {
        Box::pin(async move {
            let repository = RepositoryRegistry::repository::<Self>()?;
            repository.save_batch(aggregates).await
        })
    }
}

impl<T> AggregateRootExt for T where T: AggregateRoot {}
