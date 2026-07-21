//! CQRS read-model projection contracts.

use futures::future::BoxFuture;

use crate::event::EventEnvelope;
use crate::{DddError, DddResult};

/// Marker contract for query-side read models.
pub trait ReadModel: Clone + Send + Sync + 'static {}

impl<T> ReadModel for T where T: Clone + Send + Sync + 'static {}

/// One ordered chunk from an event stream.
#[derive(Debug, Clone, PartialEq)]
pub struct EventChunk {
    /// Inclusive position of the first returned event.
    pub from: u64,
    /// Position following the last returned event.
    pub next: u64,
    /// Ordered events.
    pub events: Vec<EventEnvelope>,
}

impl EventChunk {
    /// Creates an empty chunk without advancing the stream position.
    pub const fn empty(position: u64) -> Self {
        Self {
            from: position,
            next: position,
            events: Vec::new(),
        }
    }
}

/// Reads ordered event chunks for projections.
pub trait EventChunkReader: Send + Sync + 'static {
    /// Reads at most `limit` events from an inclusive position.
    fn read(&self, from: u64, limit: usize) -> BoxFuture<'_, DddResult<EventChunk>>;
}

/// Persists per-projection checkpoints.
pub trait ProjectionPositionRepository: Send + Sync + 'static {
    /// Reads a projection checkpoint.
    fn load<'a>(&'a self, projection: &'a str) -> BoxFuture<'a, DddResult<u64>>;

    /// Saves a projection checkpoint after successful handling.
    fn save<'a>(&'a self, projection: &'a str, position: u64) -> BoxFuture<'a, DddResult<()>>;
}

/// Applies events to one read model.
pub trait Projection: Send + Sync + 'static {
    /// Stable projection name.
    fn name(&self) -> &'static str;

    /// Applies one event idempotently.
    fn apply<'a>(&'a self, event: &'a EventEnvelope) -> BoxFuture<'a, DddResult<()>>;
}

/// Drives a projection with checkpoint-after-apply semantics.
pub struct ProjectionRunner<R, P> {
    reader: R,
    positions: P,
    batch_size: usize,
}

impl<R, P> ProjectionRunner<R, P>
where
    R: EventChunkReader,
    P: ProjectionPositionRepository,
{
    /// Creates a runner. A zero batch size is rejected during execution.
    pub const fn new(reader: R, positions: P, batch_size: usize) -> Self {
        Self {
            reader,
            positions,
            batch_size,
        }
    }

    /// Processes one event chunk and returns whether the checkpoint advanced.
    pub async fn run_once<T>(&self, projection: &T) -> DddResult<bool>
    where
        T: Projection,
    {
        if self.batch_size == 0 {
            return Err(DddError::Adapter {
                adapter: "projection",
                message: "batch size must be greater than zero".to_owned(),
            });
        }
        let position = self.positions.load(projection.name()).await?;
        let chunk = self.reader.read(position, self.batch_size).await?;
        if chunk.events.is_empty() || chunk.next <= position {
            return Ok(false);
        }
        for event in &chunk.events {
            projection.apply(event).await?;
        }
        self.positions.save(projection.name(), chunk.next).await?;
        Ok(true)
    }
}
