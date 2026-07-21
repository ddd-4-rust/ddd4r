//! Apache Shiro migration facade backed by the ddd4r Subject engine.

#![forbid(unsafe_code)]

use std::sync::Arc;

use ddd4r_auth::{Subject, SubjectEngine, SubjectProvider};
use ddd4r_core::module::{ModuleDescriptor, ModuleMaturity};

/// Shiro-compatible session facade.
#[derive(Debug, Clone)]
pub struct ShiroSubject {
    engine: Arc<SubjectEngine>,
    realm: Option<String>,
}

impl Subject for ShiroSubject {
    fn engine(&self) -> &SubjectEngine {
        &self.engine
    }

    fn adapter_name(&self) -> &'static str {
        "shiro"
    }

    fn default_realm(&self) -> Option<&str> {
        self.realm.as_deref()
    }
}

/// Creates fresh Shiro facades that share one session engine.
#[derive(Debug, Clone)]
pub struct ShiroSubjectProvider {
    engine: Arc<SubjectEngine>,
}

impl ShiroSubjectProvider {
    /// Creates a provider over an application-owned engine.
    pub const fn new(engine: Arc<SubjectEngine>) -> Self {
        Self { engine }
    }
}

impl Default for ShiroSubjectProvider {
    fn default() -> Self {
        Self::new(Arc::new(SubjectEngine::default()))
    }
}

impl SubjectProvider for ShiroSubjectProvider {
    fn subject(&self) -> Arc<dyn Subject> {
        Arc::new(ShiroSubject {
            engine: Arc::clone(&self.engine),
            realm: Some("default".to_owned()),
        })
    }

    fn subject_for_realm(&self, realm: Option<&str>) -> Arc<dyn Subject> {
        Arc::new(ShiroSubject {
            engine: Arc::clone(&self.engine),
            realm: realm.map(str::to_owned),
        })
    }
}

/// Machine-readable migration descriptor.
pub const MODULE: ModuleDescriptor = ModuleDescriptor {
    java_artifact: "ddd4j-auth-shiro",
    rust_package: "ddd4r-auth-shiro",
    group: "auth",
    maturity: ModuleMaturity::InProgress,
};
