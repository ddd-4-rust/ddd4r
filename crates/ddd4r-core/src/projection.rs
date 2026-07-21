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
    fn read<'a>(
        &'a self,
        stream_id: &'a str,
        from: u64,
        limit: usize,
        event_types: &'a [&'a str],
    ) -> BoxFuture<'a, DddResult<EventChunk>>;
}

/// Immutable checkpoint value aligned with ddd4j's `ProjectionPosition`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionPosition {
    /// Event stream read by the projection.
    pub stream_id: String,
    /// Next inclusive event number to load.
    pub next_event_number: u64,
}

impl ProjectionPosition {
    /// Returns a copy advanced to the supplied next event number.
    pub fn with_next_event_number(&self, next_event_number: u64) -> Self {
        Self {
            stream_id: self.stream_id.clone(),
            next_event_number,
        }
    }
}

/// Persists per-projection checkpoints.
pub trait ProjectionPositionRepository: Send + Sync + 'static {
    /// Reads a projection checkpoint.
    fn load<'a>(&'a self, projection: &'a str) -> BoxFuture<'a, DddResult<u64>>;

    /// Saves a projection checkpoint after successful handling.
    fn save<'a>(&'a self, projection: &'a str, position: u64) -> BoxFuture<'a, DddResult<()>>;
}

/// ddd4j-compatible projection checkpoint service.
#[derive(Debug, Clone)]
pub struct ProjectionService<P> {
    positions: P,
}

impl<P> ProjectionService<P>
where
    P: ProjectionPositionRepository,
{
    /// Creates a service backed by a checkpoint repository.
    pub const fn new(positions: P) -> Self {
        Self { positions }
    }

    /// Resets a projection to the beginning of its stream.
    pub async fn reset_projection_position(&self, stream_id: &str) -> DddResult<()> {
        self.positions.save(stream_id, 0).await
    }

    /// Reads the next event number, defaulting behavior to the repository implementation.
    pub async fn read_projection_position(&self, stream_id: &str) -> DddResult<u64> {
        self.positions.load(stream_id).await
    }

    /// Persists and returns an immutable checkpoint value.
    pub async fn update_projection_position(
        &self,
        stream_id: &str,
        next_event_number: u64,
    ) -> DddResult<ProjectionPosition> {
        self.positions.save(stream_id, next_event_number).await?;
        Ok(ProjectionPosition {
            stream_id: stream_id.to_owned(),
            next_event_number,
        })
    }
}

/// Applies events to one read model.
pub trait Projection: Send + Sync + 'static {
    /// Stable projection name.
    fn name(&self) -> &'static str;

    /// Event stream identifier; defaults to the stable projection name.
    fn stream_id(&self) -> &'static str {
        self.name()
    }

    /// Event types consumed by this projection. Empty means adapter-defined/all.
    fn event_types(&self) -> &'static [&'static str] {
        &[]
    }

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
        let stream_id = projection.stream_id();
        let position = self.positions.load(stream_id).await?;
        let chunk = self
            .reader
            .read(
                stream_id,
                position,
                self.batch_size,
                projection.event_types(),
            )
            .await?;
        if chunk.events.is_empty() || chunk.next <= position {
            return Ok(false);
        }
        for event in &chunk.events {
            projection.apply(event).await?;
        }
        self.positions.save(stream_id, chunk.next).await?;
        Ok(true)
    }
}
