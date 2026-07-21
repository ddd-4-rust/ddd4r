//! Subject provider registry.

use std::sync::Arc;

use ddd4r_core::context::Registry;
use ddd4r_core::runtime::RuntimeRegistry;

use crate::{AuthError, AuthResult, Subject};

const SUBJECT_PROVIDER_SERVICE: &str = "auth.subject-provider";

/// Creates framework-specific Subject facades over a shared authentication engine.
pub trait SubjectProvider: Send + Sync {
    /// Returns a fresh facade for the default realm.
    fn subject(&self) -> Arc<dyn Subject>;

    /// Returns a fresh facade for a selected realm.
    fn subject_for_realm(&self, _realm: Option<&str>) -> Arc<dyn Subject> {
        self.subject()
    }
}

/// Task-local-first provider facade equivalent to ddd4j `SubjectKit` registration.
pub struct SubjectProviders;

impl SubjectProviders {
    /// Registers the process-wide provider exactly once.
    pub fn register(provider: Arc<dyn SubjectProvider>) -> AuthResult<()> {
        RuntimeRegistry::register(SUBJECT_PROVIDER_SERVICE, provider).map_err(Into::into)
    }

    /// Registers a provider in an explicit task-local registry.
    pub fn register_in(registry: &Registry, provider: Arc<dyn SubjectProvider>) -> AuthResult<()> {
        RuntimeRegistry::register_in(registry, SUBJECT_PROVIDER_SERVICE, provider)
            .map_err(Into::into)
    }

    /// Resolves a Subject for the default realm.
    pub fn subject() -> AuthResult<Arc<dyn Subject>> {
        Self::provider().map(|provider| provider.subject())
    }

    /// Resolves a Subject for a selected realm.
    pub fn subject_for_realm(realm: Option<&str>) -> AuthResult<Arc<dyn Subject>> {
        Self::provider().map(|provider| provider.subject_for_realm(realm))
    }

    fn provider() -> AuthResult<Arc<dyn SubjectProvider>> {
        RuntimeRegistry::get::<dyn SubjectProvider>(SUBJECT_PROVIDER_SERVICE)?
            .ok_or(AuthError::ProviderNotRegistered)
    }
}
