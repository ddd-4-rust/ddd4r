//! Compatibility boundary for `ddd4j-runtime-micronaut`.
//!
//! This package is scaffolded and cannot be published until its entry in
//! `port-manifest.toml` contains API and test evidence.

#![forbid(unsafe_code)]

use ddd4r_core::module::{ModuleDescriptor, ModuleMaturity};

/// Machine-readable migration descriptor.
pub const MODULE: ModuleDescriptor = ModuleDescriptor {
    java_artifact: "ddd4j-runtime-micronaut",
    rust_package: "ddd4r-runtime-micronaut",
    group: "runtime",
    maturity: ModuleMaturity::Scaffolded,
};
