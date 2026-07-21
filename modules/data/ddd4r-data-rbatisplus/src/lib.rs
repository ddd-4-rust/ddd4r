//! `ddd4j-data-mybatisplus` compatibility adapter.
//!
//! The first executable vertical slice combines the proven `RBatis` repository
//! and transaction implementation with the independently versioned
//! `rbatis-plus` mapper, service, wrapper, metadata, and interceptor APIs.

#![forbid(unsafe_code)]

use ddd4r_core::module::{ModuleDescriptor, ModuleMaturity};

/// `RBatis` repository used as the persistence engine for this compatibility layer.
pub use ddd4r_data_rbatis::{
    RbatisBackend as RbatisPlusBackend, RbatisRepository as RbatisPlusRepository,
};
/// Public MyBatis-Plus compatible Mapper, Service, Wrapper, macro, and plugin APIs.
pub use rbatis_plus::*;

/// Machine-readable migration descriptor.
pub const MODULE: ModuleDescriptor = ModuleDescriptor {
    java_artifact: "ddd4j-data-mybatisplus",
    rust_package: "ddd4r-data-rbatisplus",
    group: "data",
    maturity: ModuleMaturity::InProgress,
};
