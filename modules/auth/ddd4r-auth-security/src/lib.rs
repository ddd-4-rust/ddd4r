//! Spring Security migration facade backed by the ddd4r Subject engine.

#![forbid(unsafe_code)]

use std::sync::Arc;

use ddd4r_auth::{Subject, SubjectEngine, SubjectProvider};
use ddd4r_core::module::{ModuleDescriptor, ModuleMaturity};

mod details;
mod exception_handler;

pub use details::AuthUserDetails;
pub use exception_handler::{SecurityErrorResponse, SecurityExceptionHandler};

/// Security-compatible facade with real opaque-session verification.
#[derive(Debug, Clone)]
pub struct SecuritySubject {
    engine: Arc<SubjectEngine>,
}

impl Subject for SecuritySubject {
    fn engine(&self) -> &SubjectEngine {
        &self.engine
    }

    fn adapter_name(&self) -> &'static str {
        "security"
    }

    fn default_realm(&self) -> Option<&str> {
        Some("security")
    }
}

/// Creates fresh Security facades sharing one revocable session store.
#[derive(Debug, Clone)]
pub struct SecuritySubjectProvider {
    engine: Arc<SubjectEngine>,
}

impl SecuritySubjectProvider {
    /// Creates a provider over an application-owned engine.
    pub const fn new(engine: Arc<SubjectEngine>) -> Self {
        Self { engine }
    }
}

impl Default for SecuritySubjectProvider {
    fn default() -> Self {
        Self::new(Arc::new(SubjectEngine::default()))
    }
}

impl SubjectProvider for SecuritySubjectProvider {
    fn subject(&self) -> Arc<dyn Subject> {
        Arc::new(SecuritySubject {
            engine: Arc::clone(&self.engine),
        })
    }
}

/// Machine-readable migration descriptor.
pub const MODULE: ModuleDescriptor = ModuleDescriptor {
    java_artifact: "ddd4j-auth-security",
    rust_package: "ddd4r-auth-security",
    group: "auth",
    maturity: ModuleMaturity::Complete,
};
