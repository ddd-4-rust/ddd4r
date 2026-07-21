//! Compatibility boundary for `ddd4j-extension-qlexpress`.
//!
//! This package is scaffolded and cannot be published until its entry in
//! `port-manifest.toml` contains API and test evidence.

#![forbid(unsafe_code)]

use ddd4r_core::module::{ModuleDescriptor, ModuleMaturity};

/// Machine-readable migration descriptor.
pub const MODULE: ModuleDescriptor = ModuleDescriptor {
    java_artifact: "ddd4j-extension-qlexpress",
    rust_package: "ddd4r-extension-qlexpress",
    group: "extensions",
    maturity: ModuleMaturity::Scaffolded,
};
