//! Machine-readable module metadata used during the 82-project migration.

/// Current maturity of a mapped ddd4r package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleMaturity {
    /// Package shape exists but behavior has not been ported.
    Scaffolded,
    /// Behavior is being implemented and cannot be published as stable.
    InProgress,
    /// Source, behavior and test evidence passed the compatibility gate.
    Complete,
}

/// Identity and migration state of one ddd4j-compatible package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleDescriptor {
    /// Source Maven artifact.
    pub java_artifact: &'static str,
    /// Rust package name.
    pub rust_package: &'static str,
    /// Capability group.
    pub group: &'static str,
    /// Current implementation maturity.
    pub maturity: ModuleMaturity,
}
