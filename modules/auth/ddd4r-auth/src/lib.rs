//! Framework-neutral authentication, session and RBAC contracts.

#![forbid(unsafe_code)]

mod context;
mod error;
mod model;
mod provider;
mod store;
mod subject;

#[cfg(feature = "testkit")]
pub mod testkit;

pub use context::SubjectScope;
pub use error::{AuthError, AuthResult};
pub use model::{
    AuthCookieConfig, AuthEvent, AuthId, AuthLogoutMode, AuthPrincipal, AuthReplacedLoginExitMode,
    AuthReplacedRange, AuthRequest, AuthSessionConfig, RolePair, SessionContext,
};
pub use provider::{SubjectProvider, SubjectProviders};
pub use store::{InMemorySessionStore, SessionRecord, SessionStore};
pub use subject::{
    AuthEventPublisher, BearerAuthenticator, DefaultSubjectDataProvider, DefaultSubjectStrategy,
    NoopAuthEventPublisher, Subject, SubjectDataProvider, SubjectEngine, SubjectStrategy,
};

use ddd4r_core::module::{ModuleDescriptor, ModuleMaturity};

/// Machine-readable migration descriptor.
pub const MODULE: ModuleDescriptor = ModuleDescriptor {
    java_artifact: "ddd4j-auth",
    rust_package: "ddd4r-auth",
    group: "auth",
    maturity: ModuleMaturity::InProgress,
};

/// Common imports for applications using ddd4r authentication.
pub mod prelude {
    pub use crate::{
        AuthCookieConfig, AuthError, AuthId, AuthPrincipal, AuthRequest, AuthResult,
        AuthSessionConfig, BearerAuthenticator, InMemorySessionStore, RolePair, Subject,
        SubjectEngine, SubjectProvider, SubjectProviders, SubjectScope,
    };
}
