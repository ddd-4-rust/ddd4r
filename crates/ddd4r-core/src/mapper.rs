//! Mapping contract between domain models and persistence objects.

use crate::DddResult;

/// Maps infrastructure persistence objects without leaking ORM details into the domain.
pub trait DomainObjectMapper<D, P>: Send + Sync + 'static {
    /// Converts a persistence object into a domain model.
    fn to_model(&self, persistence_object: P) -> DddResult<D>;

    /// Converts a domain model into a persistence object.
    fn to_persistence_object(&self, model: D) -> DddResult<P>;
}
