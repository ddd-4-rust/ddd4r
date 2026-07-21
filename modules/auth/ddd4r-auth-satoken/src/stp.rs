//! Typed Subject facade corresponding to ddd4j `StpKit`.

use std::sync::Arc;

use ddd4r_auth::{AuthError, AuthId, AuthResult, Subject, SubjectScope};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{PAYLOAD_IDENTITY_ID, PAYLOAD_INFO_ID, PAYLOAD_SCHOOL_CODE, PAYLOAD_XQ_ORG_ID};

/// Typed accessors for the current Subject and token extras.
#[derive(Clone)]
pub struct StpKit {
    subject: Arc<dyn Subject>,
}

impl std::fmt::Debug for StpKit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StpKit")
            .field("adapter", &self.subject.adapter_name())
            .finish_non_exhaustive()
    }
}

impl StpKit {
    /// Creates typed accessors for one Subject facade.
    pub fn new(subject: Arc<dyn Subject>) -> Self {
        Self { subject }
    }

    /// Returns the current login identifier.
    pub async fn login_id(&self) -> AuthResult<AuthId> {
        self.subject
            .principal()
            .await?
            .and_then(|principal| principal.login_id)
            .ok_or(AuthError::NotAuthenticated)
    }

    /// Parses the current login identifier into a scalar.
    pub async fn login_id_as<T>(&self) -> AuthResult<T>
    where
        T: std::str::FromStr,
        T::Err: std::fmt::Display,
    {
        self.login_id()
            .await?
            .parse()
            .map_err(|error| AuthError::Infrastructure {
                message: format!("login ID conversion failed: {error}"),
            })
    }

    /// Returns the current business user identifier.
    pub async fn user_id(&self) -> AuthResult<Option<AuthId>> {
        Ok(self
            .subject
            .principal()
            .await?
            .and_then(|principal| principal.user_id))
    }

    /// Returns the current organization identifier.
    pub async fn org_id(&self) -> AuthResult<Option<AuthId>> {
        Ok(self
            .subject
            .principal()
            .await?
            .and_then(|principal| principal.org_id))
    }

    /// Returns the current primary role identifier.
    pub async fn role_id(&self) -> AuthResult<Option<AuthId>> {
        Ok(self
            .subject
            .principal()
            .await?
            .and_then(|principal| principal.role_id))
    }

    /// Reads a current-token extra.
    pub async fn extra(&self, key: &str) -> AuthResult<Option<Value>> {
        let token = SubjectScope::current_token().ok_or(AuthError::NotAuthenticated)?;
        self.subject.get_extra(&token, key).await
    }

    /// Reads and deserializes a current-token extra.
    pub async fn extra_as<T>(&self, key: &str) -> AuthResult<Option<T>>
    where
        T: DeserializeOwned,
    {
        self.extra(key)
            .await?
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| AuthError::Infrastructure {
                message: format!("token extra conversion failed for {key}: {error}"),
            })
    }

    /// Reads an extra from an explicitly supplied token.
    pub async fn extra_by_token<T>(&self, token: &str, key: &str) -> AuthResult<Option<T>>
    where
        T: DeserializeOwned,
    {
        self.subject
            .get_extra(token, key)
            .await?
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| AuthError::Infrastructure {
                message: format!("token extra conversion failed for {key}: {error}"),
            })
    }

    /// Returns the campus organization identifier extra.
    pub async fn campus_org_id_as<T>(&self) -> AuthResult<Option<T>>
    where
        T: DeserializeOwned,
    {
        self.extra_as(PAYLOAD_XQ_ORG_ID).await
    }

    /// Returns the information-entry identifier extra.
    pub async fn info_id_as<T>(&self) -> AuthResult<Option<T>>
    where
        T: DeserializeOwned,
    {
        self.extra_as(PAYLOAD_INFO_ID).await
    }

    /// Returns the identity identifier extra.
    pub async fn identity_id_as<T>(&self) -> AuthResult<Option<T>>
    where
        T: DeserializeOwned,
    {
        self.extra_as(PAYLOAD_IDENTITY_ID).await
    }

    /// Returns the school or campus code.
    pub async fn school_code(&self) -> AuthResult<Option<String>> {
        self.extra_as(PAYLOAD_SCHOOL_CODE).await
    }
}
