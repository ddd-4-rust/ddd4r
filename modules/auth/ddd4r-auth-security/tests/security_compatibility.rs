//! Security user-details and exception-mapping compatibility tests.

use ddd4r_auth::{AuthError, AuthPrincipal};
use ddd4r_auth_security::{AuthUserDetails, SecurityExceptionHandler};

#[test]
fn user_details_carry_principal_and_redact_credentials() {
    let principal = AuthPrincipal::for_login("user-1");
    let details = AuthUserDetails::new(
        "user-1",
        "argon2-hash",
        true,
        ["ROLE_ADMIN".to_owned()],
        principal.clone(),
    );
    assert_eq!(details.username(), "user-1");
    assert!(details.enabled());
    assert_eq!(details.auth_principal(), &principal);
    assert!(details.verify_credential("secret", |presented, hash| {
        presented == "secret" && hash == "argon2-hash"
    }));
    assert!(!format!("{details:?}").contains("argon2-hash"));
    assert!(
        !serde_json::to_string(&details)
            .unwrap()
            .contains("argon2-hash")
    );
}

#[test]
fn exception_handler_preserves_401_403_and_safe_500_boundaries() {
    assert_eq!(
        SecurityExceptionHandler::map(&AuthError::BadCredentials).status,
        401
    );
    assert_eq!(
        SecurityExceptionHandler::map(&AuthError::AccountDisabled {
            login_id: "user-1".to_owned(),
        })
        .status,
        403
    );
    assert_eq!(
        SecurityExceptionHandler::map(&AuthError::Store {
            message: "database secret".to_owned(),
        })
        .message,
        "认证服务暂不可用"
    );
    assert_eq!(SecurityExceptionHandler::access_denied(false).status, 401);
    assert_eq!(SecurityExceptionHandler::access_denied(true).status, 403);
}
