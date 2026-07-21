//! COLA Architecture compatibility checker for Cargo workspaces.

#![forbid(unsafe_code)]

use std::path::Path;

use ddd4r_core::module::{ModuleDescriptor, ModuleMaturity};
use ddd4r_ddd_rules::{ArchitectureChecker, ArchitecturePolicy};
pub use ddd4r_ddd_rules::{ArchitectureReport, CheckerError, Layer, Violation};

/// Machine-readable migration descriptor.
pub const MODULE: ModuleDescriptor = ModuleDescriptor {
    java_artifact: "ddd4j-ddd-rules-cola",
    rust_package: "ddd4r-ddd-rules-cola",
    group: "ddd-rules",
    maturity: ModuleMaturity::InProgress,
};

/// COLA Architecture facade aligned with ddd4j's checker entry point.
#[derive(Debug, Clone)]
pub struct ColaArchitectureChecker {
    checker: ArchitectureChecker,
}

impl Default for ColaArchitectureChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl ColaArchitectureChecker {
    /// Creates a strict COLA four-layer checker.
    pub fn new() -> Self {
        Self {
            checker: ArchitectureChecker::new(ArchitecturePolicy::cola()),
        }
    }

    /// Checks the Cargo workspace containing the supplied manifest.
    pub fn check(
        &self,
        manifest_path: impl AsRef<Path>,
    ) -> Result<ArchitectureReport, CheckerError> {
        self.checker.check(manifest_path)
    }

    /// Returns the conventional COLA package layout.
    pub const fn expected_structure() -> &'static str {
        "crates/{domain,application,adapter,infrastructure}-* with application/{executor,query,service}"
    }
}
