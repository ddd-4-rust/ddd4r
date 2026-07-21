//! Public facade for the ddd4r workspace.

#![forbid(unsafe_code)]

pub use ddd4r_core as core;
pub use ddd4r_core::*;
pub use ddd4r_kit as kit;

#[cfg(feature = "macros")]
pub use ddd4r_annotation::{DomainModel, Entity, ValueObject};

#[cfg(feature = "cache")]
pub use ddd4r_cache as cache;

#[cfg(feature = "outbox")]
pub use ddd4r_outbox as outbox;

/// Common imports for applications.
pub mod prelude {
    pub use ddd4r_core::prelude::*;

    #[cfg(feature = "macros")]
    pub use ddd4r_annotation::{DomainModel, Entity, ValueObject};
}
