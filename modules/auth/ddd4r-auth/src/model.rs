//! Authentication value objects shared by all adapters.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

/// Canonical account identifier used at framework boundaries.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuthId(String);

impl AuthId {
    /// Creates an identifier after trimming surrounding whitespace.
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into().trim().to_owned();
        (!value.is_empty()).then_some(Self(value))
    }

    /// Returns the canonical string representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parses the identifier into a domain-specific scalar.
    pub fn parse<T>(&self) -> Result<T, T::Err>
    where
        T: FromStr,
    {
        self.0.parse()
    }
}

impl Display for AuthId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<String> for AuthId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for AuthId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<Uuid> for AuthId {
    fn from(value: Uuid) -> Self {
        Self(value.to_string())
    }
}

macro_rules! impl_numeric_auth_id {
    ($($number:ty),+ $(,)?) => {
        $(
            impl From<$number> for AuthId {
                fn from(value: $number) -> Self {
                    Self(value.to_string())
                }
            }
        )+
    };
}

impl_numeric_auth_id!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
);

/// One role attached to an authenticated principal.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RolePair {
    /// Role database identifier.
    pub role_id: Option<AuthId>,
    /// Stable business code.
    pub role_code: Option<String>,
    /// Human-readable name.
    pub role_name: Option<String>,
    /// Whether the role requires multi-factor verification.
    pub verify: bool,
}

/// Complete authenticated identity and request-source metadata.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AuthPrincipal {
    /// Owning organization.
    pub org_id: Option<AuthId>,
    /// Per-application open identifier.
    pub openid: Option<String>,
    /// Cross-application union identifier.
    pub union_id: Option<String>,
    /// Login account identifier.
    pub login_id: Option<AuthId>,
    /// Business user identifier.
    pub user_id: Option<AuthId>,
    /// Internal user code.
    pub user_code: Option<String>,
    /// Business user type.
    pub user_type: Option<String>,
    /// Primary role identifier.
    pub role_id: Option<AuthId>,
    /// Primary role code.
    pub role_code: Option<String>,
    /// All roles.
    pub roles: Vec<RolePair>,
    /// Permission markers.
    pub permissions: BTreeSet<String>,
    /// Extensible profile values.
    pub profile: BTreeMap<String, Value>,
    /// Whether external identity binding is complete.
    pub bound: bool,
    /// Whether the profile is initialized.
    pub initial: bool,
    /// Whether multi-factor verification is required.
    pub verify: bool,
    /// Client application identifier.
    pub app_id: Option<String>,
    /// Client channel code.
    pub app_channel: Option<String>,
    /// Client version.
    pub app_version: Option<String>,
    /// Request IP address.
    pub ip_address: Option<String>,
    /// Client device type.
    pub device_type: Option<String>,
    /// Client device identifier.
    pub device_id: Option<String>,
    /// Request user-agent.
    pub user_agent: Option<String>,
}

impl AuthPrincipal {
    /// Creates a principal with matching login and user identifiers.
    pub fn for_login(login_id: impl Into<AuthId>) -> Self {
        let login_id = login_id.into();
        Self {
            login_id: Some(login_id.clone()),
            user_id: Some(login_id),
            ..Self::default()
        }
    }

    /// Returns all role IDs and role codes recognized by RBAC checks.
    pub fn role_identifiers(&self) -> BTreeSet<String> {
        let mut identifiers = BTreeSet::new();
        if let Some(role_id) = &self.role_id {
            identifiers.insert(role_id.to_string());
        }
        if let Some(role_code) = &self.role_code
            && !role_code.trim().is_empty()
        {
            identifiers.insert(role_code.clone());
        }
        for role in &self.roles {
            if let Some(role_id) = &role.role_id {
                identifiers.insert(role_id.to_string());
            }
            if let Some(role_code) = &role.role_code
                && !role_code.trim().is_empty()
            {
                identifiers.insert(role_code.clone());
            }
        }
        identifiers
    }
}

/// Browser cookie policy independent of a web framework.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthCookieConfig {
    /// Cookie name selected by the adapter.
    pub name: Option<String>,
    /// Cookie domain.
    pub domain: Option<String>,
    /// Cookie path.
    pub path: String,
    /// Restricts transport to HTTPS.
    pub secure: bool,
    /// Prevents JavaScript access.
    pub http_only: bool,
    /// `SameSite` policy.
    pub same_site: String,
    /// Browser lifetime in seconds; `-1` means session cookie.
    pub max_age_seconds: i64,
}

impl Default for AuthCookieConfig {
    fn default() -> Self {
        Self {
            name: None,
            domain: None,
            path: "/".to_owned(),
            secure: false,
            http_only: true,
            same_site: "Lax".to_owned(),
            max_age_seconds: -1,
        }
    }
}

/// Which side is terminated when concurrent login is disabled.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthReplacedLoginExitMode {
    /// Reject the newly arriving device.
    #[default]
    NewDevice,
    /// Revoke the previous device and admit the new one.
    OldDevice,
}

/// Scope used when replacing sessions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthReplacedRange {
    /// Replace sessions of the same device type only.
    #[default]
    CurrentDeviceType,
    /// Replace sessions for every device type.
    AllDeviceTypes,
}

/// Revocation reason used when the maximum login count is exceeded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthLogoutMode {
    /// Ordinary logout.
    #[default]
    Logout,
    /// Administrative kickout.
    Kickout,
    /// Replacement by another session.
    Replaced,
}

/// Framework-independent login session policy.
#[allow(clippy::struct_excessive_bools)] // Mirrors the frozen ddd4j public configuration contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthSessionConfig {
    /// Client device type.
    pub device_type: Option<String>,
    /// Client device identifier.
    pub device_id: Option<String>,
    /// Cookie output policy.
    pub cookie: AuthCookieConfig,
    /// Absolute token lifetime in seconds; `-1` means permanent.
    pub timeout_seconds: i64,
    /// Idle lifetime in seconds; `None` disables idle expiry.
    pub active_timeout_seconds: Option<i64>,
    /// Whether the adapter should eagerly materialize session state.
    pub create_token_session_now: bool,
    /// Whether multiple sessions are permitted.
    pub concurrent: bool,
    /// Whether matching concurrent logins reuse one token.
    pub share: bool,
    /// Maximum concurrent sessions; `-1` means unlimited.
    pub max_login_count: i32,
    /// Replacement side when concurrency is disabled.
    pub replaced_login_exit_mode: AuthReplacedLoginExitMode,
    /// Replacement device scope.
    pub replaced_range: AuthReplacedRange,
    /// Revocation mode for overflow sessions.
    pub overflow_logout_mode: AuthLogoutMode,
    /// Whether a web adapter writes the token to response headers.
    pub write_token_to_header: bool,
    /// Optional caller-supplied opaque token.
    pub preset_token: Option<String>,
}

impl Default for AuthSessionConfig {
    fn default() -> Self {
        Self {
            device_type: None,
            device_id: None,
            cookie: AuthCookieConfig::default(),
            timeout_seconds: -1,
            active_timeout_seconds: None,
            create_token_session_now: false,
            concurrent: true,
            share: true,
            max_login_count: -1,
            replaced_login_exit_mode: AuthReplacedLoginExitMode::NewDevice,
            replaced_range: AuthReplacedRange::CurrentDeviceType,
            overflow_logout_mode: AuthLogoutMode::Logout,
            write_token_to_header: true,
            preset_token: None,
        }
    }
}

/// Login request; equality intentionally follows ddd4j and compares login ID only.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthRequest {
    /// Login account identifier.
    pub login_id: Option<AuthId>,
    /// Optional pre-authenticated principal.
    pub principal: Option<AuthPrincipal>,
    /// Authentication realm.
    pub realm: Option<String>,
    /// Session policy.
    pub session: AuthSessionConfig,
    /// Login-scene and provider-specific values.
    pub extra: BTreeMap<String, Value>,
}

impl AuthRequest {
    /// Creates a request for one login identifier.
    pub fn new(login_id: impl Into<AuthId>) -> Self {
        Self {
            login_id: Some(login_id.into()),
            ..Self::default()
        }
    }

    /// Attaches a principal.
    pub fn with_principal(mut self, principal: AuthPrincipal) -> Self {
        self.principal = Some(principal);
        self
    }

    /// Selects an authentication realm.
    pub fn with_realm(mut self, realm: impl Into<String>) -> Self {
        self.realm = Some(realm.into());
        self
    }

    /// Sets the absolute token lifetime.
    pub const fn with_timeout(mut self, seconds: i64) -> Self {
        self.session.timeout_seconds = seconds;
        self
    }

    /// Selects a client device type.
    pub fn with_device_type(mut self, device_type: impl Into<String>) -> Self {
        self.session.device_type = Some(device_type.into());
        self
    }

    /// Adds one provider-specific value.
    pub fn with_extra(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

impl PartialEq for AuthRequest {
    fn eq(&self, other: &Self) -> bool {
        self.login_id == other.login_id
    }
}

impl Eq for AuthRequest {}

/// Request-scoped identity values used by web and data-scope adapters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionContext {
    /// Tenant boundary.
    pub tenant_id: Option<AuthId>,
    /// `WeChat` user identifier.
    pub wx_user_id: Option<AuthId>,
    /// Client application identifier.
    pub app_id: Option<String>,
    /// Client session key.
    pub session_key: Option<String>,
    /// Open identifier.
    pub open_id: Option<String>,
    /// Business user identifier.
    pub user_id: Option<AuthId>,
    /// Whether the account belongs to an enterprise tenant.
    pub enterprise: bool,
}

/// Auditable lifecycle events emitted by a Subject implementation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AuthEvent {
    /// Successful login.
    LoginSucceeded {
        /// Login account.
        login_id: AuthId,
        /// Issued token. Event sinks must redact it before external logging.
        token: String,
        /// Event time.
        occurred_at: OffsetDateTime,
    },
    /// Failed login.
    LoginFailed {
        /// Login account when supplied.
        login_id: Option<AuthId>,
        /// Stable failure code, never credentials.
        reason: String,
        /// Event time.
        occurred_at: OffsetDateTime,
    },
    /// Session logout or revocation.
    LoggedOut {
        /// Login account.
        login_id: AuthId,
        /// Revocation mode.
        mode: AuthLogoutMode,
        /// Event time.
        occurred_at: OffsetDateTime,
    },
    /// Token rotation.
    TokenRefreshed {
        /// Login account.
        login_id: AuthId,
        /// Event time.
        occurred_at: OffsetDateTime,
    },
}
