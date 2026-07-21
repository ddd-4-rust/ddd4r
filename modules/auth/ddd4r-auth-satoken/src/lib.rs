//! Sa-Token migration facade backed by the framework-neutral ddd4r Subject engine.

#![forbid(unsafe_code)]

use std::sync::Arc;

use ddd4r_auth::{Subject, SubjectEngine, SubjectProvider};
use ddd4r_core::module::{ModuleDescriptor, ModuleMaturity};

/// Sa-Token-compatible Subject facade.
#[derive(Debug, Clone)]
pub struct SaTokenSubject {
    engine: Arc<SubjectEngine>,
    realm: Option<String>,
}

impl Subject for SaTokenSubject {
    fn engine(&self) -> &SubjectEngine {
        &self.engine
    }

    fn adapter_name(&self) -> &'static str {
        "satoken"
    }

    fn default_realm(&self) -> Option<&str> {
        self.realm.as_deref()
    }
}

/// Creates fresh Sa-Token facades that share one session engine.
#[derive(Debug, Clone)]
pub struct SaTokenSubjectProvider {
    engine: Arc<SubjectEngine>,
}

impl SaTokenSubjectProvider {
    /// Creates a provider over an application-owned engine.
    pub const fn new(engine: Arc<SubjectEngine>) -> Self {
        Self { engine }
    }
}

impl Default for SaTokenSubjectProvider {
    fn default() -> Self {
        Self::new(Arc::new(SubjectEngine::default()))
    }
}

impl SubjectProvider for SaTokenSubjectProvider {
    fn subject(&self) -> Arc<dyn Subject> {
        Arc::new(SaTokenSubject {
            engine: Arc::clone(&self.engine),
            realm: Some("default".to_owned()),
        })
    }

    fn subject_for_realm(&self, realm: Option<&str>) -> Arc<dyn Subject> {
        Arc::new(SaTokenSubject {
            engine: Arc::clone(&self.engine),
            realm: realm.map(str::to_owned),
        })
    }
}

/// Machine-readable migration descriptor.
pub const MODULE: ModuleDescriptor = ModuleDescriptor {
    java_artifact: "ddd4j-auth-satoken",
    rust_package: "ddd4r-auth-satoken",
    group: "auth",
    maturity: ModuleMaturity::InProgress,
};
