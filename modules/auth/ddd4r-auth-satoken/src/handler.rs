//! Rust policy objects corresponding to Sa-Token annotations and handlers.

use std::collections::BTreeMap;
use std::sync::Arc;

use ddd4r_auth::{AuthError, AuthId, AuthPrincipal, AuthRequest, AuthResult, Subject};
use serde_json::Value;
use time::OffsetDateTime;

use crate::{ApiKeyKit, SaTempKit, SaTempToken};

/// Marker corresponding to `SaAdminCheckLogin`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SaAdminCheckLogin;

/// Marker corresponding to `SaUserCheckLogin`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SaUserCheckLogin;

/// Required internal API-key scopes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaInternalCheck {
    /// Every scope that the key must grant.
    pub scopes: Vec<String>,
}

impl Default for SaInternalCheck {
    fn default() -> Self {
        Self {
            scopes: vec!["internal".to_owned()],
        }
    }
}

/// Mixed temporary-token or existing-login policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaMixCheckLogin {
    /// Account realm selected when a temporary token is promoted.
    pub account_type: Option<String>,
    /// Whether the temporary token is consumed after one attempt.
    pub throwaway: bool,
    /// Whether a reusable temporary token creates a full session.
    pub login: bool,
}

impl Default for SaMixCheckLogin {
    fn default() -> Self {
        Self {
            account_type: None,
            throwaway: false,
            login: true,
        }
    }
}

/// Validates internal service API keys.
#[derive(Debug, Clone)]
pub struct SaInternalCheckHandler {
    api_keys: ApiKeyKit,
}

impl SaInternalCheckHandler {
    /// Creates an internal-call handler.
    pub const fn new(api_keys: ApiKeyKit) -> Self {
        Self { api_keys }
    }

    /// Validates the supplied key and all policy scopes.
    pub async fn check(&self, api_key: Option<&str>, policy: &SaInternalCheck) -> AuthResult<()> {
        let api_key = api_key.ok_or_else(|| AuthError::AccessDenied {
            authority: "internal-api-key".to_owned(),
        })?;
        self.api_keys.check(api_key, &policy.scopes).await?;
        Ok(())
    }
}

/// Result of a mixed-login check.
#[derive(Debug, Clone, PartialEq)]
pub struct MixCheckOutcome {
    /// Verified or promoted principal.
    pub principal: AuthPrincipal,
    /// Full session token when promotion was requested.
    pub session_token: Option<String>,
    /// Whether a temporary token was consumed.
    pub consumed: bool,
}

/// Handles temporary-token promotion and existing-session fallback.
#[derive(Clone)]
pub struct SaMixCheckLoginHandler {
    temp_tokens: SaTempKit,
    subject: Arc<dyn Subject>,
}

impl std::fmt::Debug for SaMixCheckLoginHandler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SaMixCheckLoginHandler")
            .field("adapter", &self.subject.adapter_name())
            .finish_non_exhaustive()
    }
}

impl SaMixCheckLoginHandler {
    /// Creates a handler over temporary-token and Subject facades.
    pub const fn new(temp_tokens: SaTempKit, subject: Arc<dyn Subject>) -> Self {
        Self {
            temp_tokens,
            subject,
        }
    }

    /// Checks a temporary token when present, otherwise requires an existing session.
    pub async fn check(
        &self,
        temp_token: Option<&str>,
        policy: &SaMixCheckLogin,
    ) -> AuthResult<MixCheckOutcome> {
        let Some(token) = temp_token.filter(|token| !token.trim().is_empty()) else {
            let principal = self
                .subject
                .principal()
                .await?
                .ok_or(AuthError::NotAuthenticated)?;
            return Ok(MixCheckOutcome {
                principal,
                session_token: None,
                consumed: false,
            });
        };

        let checked = self.temp_tokens.check_temp_token(token).await;
        let consumed = if policy.throwaway {
            self.temp_tokens.delete_token(token).await?
        } else {
            false
        };
        let value = checked?;
        let principal = principal_from_temp(&value)?;
        let session_token = if policy.login && !policy.throwaway {
            let login_id = principal
                .login_id
                .clone()
                .ok_or(AuthError::InvalidLoginId)?;
            let mut request = AuthRequest::new(login_id).with_principal(principal.clone());
            if let Some(realm) = policy
                .account_type
                .as_deref()
                .filter(|realm| !realm.trim().is_empty())
            {
                request.realm = Some(realm.to_owned());
            }
            request.session.device_type.clone_from(&value.device_type);
            request.session.device_id.clone_from(&value.device_id);
            request.extra = payload_from_temp(&value)?;
            Some(self.subject.login(request).await?)
        } else {
            None
        };
        Ok(MixCheckOutcome {
            principal,
            session_token,
            consumed,
        })
    }

    /// Extracts claims promoted to the full token.
    pub fn token_payload(value: &SaTempToken) -> BTreeMap<String, Value> {
        BTreeMap::from([
            (
                "auth_type".to_owned(),
                value.auth_type.clone().map_or(Value::Null, Value::String),
            ),
            (
                "iat".to_owned(),
                Value::from(OffsetDateTime::now_utc().unix_timestamp()),
            ),
            (
                "sub".to_owned(),
                value.login_id.clone().map_or(Value::Null, Value::String),
            ),
        ])
    }

    /// Extracts terminal and device claims.
    pub fn terminal_payload(value: &SaTempToken) -> BTreeMap<String, Value> {
        [
            ("app_id", value.app_id.as_ref()),
            ("app_channel", value.app_channel.as_ref()),
            ("app_version", value.app_version.as_ref()),
            ("device_type", value.device_type.as_ref()),
            ("device_id", value.device_id.as_ref()),
        ]
        .into_iter()
        .map(|(key, value)| {
            (
                key.to_owned(),
                value.cloned().map_or(Value::Null, Value::String),
            )
        })
        .collect()
    }
}

fn principal_from_temp(value: &SaTempToken) -> AuthResult<AuthPrincipal> {
    value.validate()?;
    let login_id = AuthId::from(
        value
            .login_id
            .as_ref()
            .ok_or(AuthError::InvalidLoginId)?
            .as_str(),
    );
    Ok(AuthPrincipal {
        openid: value.openid.clone(),
        union_id: value.union_id.clone(),
        login_id: Some(login_id.clone()),
        user_id: Some(login_id),
        app_id: value.app_id.clone(),
        app_channel: value.app_channel.clone(),
        app_version: value.app_version.clone(),
        ip_address: value.ip_address.clone(),
        device_type: value.device_type.clone(),
        device_id: value.device_id.clone(),
        user_agent: value.user_agent.clone(),
        ..AuthPrincipal::default()
    })
}

fn payload_from_temp(value: &SaTempToken) -> AuthResult<BTreeMap<String, Value>> {
    let serialized = serde_json::to_value(value).map_err(|error| AuthError::Infrastructure {
        message: format!("temporary-token payload conversion failed: {error}"),
    })?;
    let Value::Object(values) = serialized else {
        return Err(AuthError::Infrastructure {
            message: "temporary-token payload must be an object".to_owned(),
        });
    };
    Ok(values.into_iter().collect())
}
