//! Event-sourced aggregate repository contracts.

use futures::future::BoxFuture;

use crate::DddResult;
use crate::domain::AggregateRoot;

/// Persistence port for rebuilding and appending event-sourced aggregates.
pub trait EventSourcingRepository<A>: Send + Sync + 'static
where
    A: AggregateRoot,
{
    /// Rebuilds the latest aggregate version, or returns `None` when the stream is absent.
    fn read<'a>(&'a self, aggregate_id: &'a A::Id) -> BoxFuture<'a, DddResult<Option<A>>>;

    /// Rebuilds an aggregate at a specific historical version.
    fn read_at<'a>(
        &'a self,
        aggregate_id: &'a A::Id,
        version: u64,
    ) -> BoxFuture<'a, DddResult<Option<A>>>;

    /// Appends the initial event stream for a new aggregate.
    fn add<'a>(&'a self, aggregate: &'a mut A) -> BoxFuture<'a, DddResult<()>>;

    /// Appends uncommitted events using optimistic concurrency.
    fn update<'a>(&'a self, aggregate: &'a mut A) -> BoxFuture<'a, DddResult<()>>;
}
