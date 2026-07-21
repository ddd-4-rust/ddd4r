//! Apache Shiro migration facade backed by the ddd4r Subject engine.

#![forbid(unsafe_code)]

use std::sync::Arc;

use ddd4r_auth::{AuthError, AuthRequest, Subject, SubjectEngine, SubjectProvider};
use ddd4r_core::module::{ModuleDescriptor, ModuleMaturity};

mod bridge;

pub use bridge::{ShiroAuthenticationToken, ShiroRealmAuthenticator, ShiroSessionDao};

/// Shiro-compatible session facade.
#[derive(Clone)]
pub struct ShiroSubject {
    engine: Arc<SubjectEngine>,
    realm: Option<String>,
    authenticator: Option<Arc<dyn ShiroRealmAuthenticator>>,
}

impl std::fmt::Debug for ShiroSubject {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ShiroSubject")
            .field("realm", &self.realm)
            .field("authenticator", &self.authenticator.is_some())
            .finish_non_exhaustive()
    }
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

    fn login(
        &self,
        mut request: AuthRequest,
    ) -> futures::future::BoxFuture<'_, ddd4r_auth::AuthResult<String>> {
        let authenticator = self.authenticator.clone();
        let engine = Arc::clone(&self.engine);
        let default_realm = self.realm.clone();
        Box::pin(async move {
            if request
                .realm
                .as_deref()
                .is_none_or(|realm| realm.trim().is_empty())
            {
                request.realm.clone_from(&default_realm);
            }
            if let Some(authenticator) = authenticator {
                let login_id = request.login_id.clone().ok_or(AuthError::InvalidLoginId)?;
                let credential = request
                    .extra
                    .remove("credential")
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .ok_or(AuthError::BadCredentials)?;
                let token = ShiroAuthenticationToken::new(
                    login_id,
                    credential,
                    request.realm.clone(),
                    false,
                );
                request.principal = Some(authenticator.authenticate(&token).await?);
            }
            <SubjectEngine as Subject>::login(engine.as_ref(), request).await
        })
    }
}

/// Creates fresh Shiro facades that share one session engine.
#[derive(Clone)]
pub struct ShiroSubjectProvider {
    engine: Arc<SubjectEngine>,
    authenticator: Option<Arc<dyn ShiroRealmAuthenticator>>,
}

impl std::fmt::Debug for ShiroSubjectProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ShiroSubjectProvider")
            .field("authenticator", &self.authenticator.is_some())
            .finish_non_exhaustive()
    }
}

impl ShiroSubjectProvider {
    /// Creates a provider over an application-owned engine.
    pub const fn new(engine: Arc<SubjectEngine>) -> Self {
        Self {
            engine,
            authenticator: None,
        }
    }

    /// Creates a provider that authenticates credentials through a Shiro Realm bridge.
    pub fn with_authenticator(
        engine: Arc<SubjectEngine>,
        authenticator: Arc<dyn ShiroRealmAuthenticator>,
    ) -> Self {
        Self {
            engine,
            authenticator: Some(authenticator),
        }
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
            authenticator: self.authenticator.clone(),
        })
    }

    fn subject_for_realm(&self, realm: Option<&str>) -> Arc<dyn Subject> {
        Arc::new(ShiroSubject {
            engine: Arc::clone(&self.engine),
            realm: realm.map(str::to_owned),
            authenticator: self.authenticator.clone(),
        })
    }
}

/// Machine-readable migration descriptor.
pub const MODULE: ModuleDescriptor = ModuleDescriptor {
    java_artifact: "ddd4j-auth-shiro",
    rust_package: "ddd4r-auth-shiro",
    group: "auth",
    maturity: ModuleMaturity::Complete,
};
