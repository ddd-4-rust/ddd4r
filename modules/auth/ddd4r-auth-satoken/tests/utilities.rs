//! Temporary-token, API-key, mixed-login and typed accessor contracts.

use std::collections::BTreeSet;
use std::sync::Arc;

use ddd4r_auth::{
    AuthError, AuthId, AuthPrincipal, AuthRequest, Subject, SubjectEngine, SubjectScope,
};
use ddd4r_auth_satoken::{
    ApiKeyKit, InMemoryApiKeyStore, SaInternalCheck, SaInternalCheckHandler, SaMixCheckLogin,
    SaMixCheckLoginHandler, SaTempKit, SaTempToken, StpKit,
};
use serde_json::json;

fn api_keys() -> ApiKeyKit {
    ApiKeyKit::new(
        Arc::new(InMemoryApiKeyStore::new()),
        Vec::from(*b"0123456789abcdef0123456789abcdef"),
    )
    .unwrap()
}

fn temporary_payload(login_id: &str) -> SaTempToken {
    SaTempToken {
        auth_type: Some("qrcode".to_owned()),
        login_id: Some(login_id.to_owned()),
        app_id: Some("app-1".to_owned()),
        app_channel: Some("web".to_owned()),
        device_type: Some("desktop".to_owned()),
        device_id: Some("device-1".to_owned()),
        ..SaTempToken::default()
    }
}

#[tokio::test]
async fn temporary_tokens_are_namespaced_revocable_and_never_store_raw_keys() {
    let kit = SaTempKit::default();
    let token = kit
        .create_token(temporary_payload("user-1"), -1)
        .await
        .unwrap();
    assert_eq!(kit.get_timeout(&token).await.unwrap(), -1);
    assert_eq!(
        kit.parse_token(&token)
            .await
            .unwrap()
            .unwrap()
            .login_id
            .as_deref(),
        Some("user-1")
    );
    assert!(kit.delete_token(&token).await.unwrap());
    assert_eq!(kit.get_timeout(&token).await.unwrap(), -2);
    assert_eq!(
        kit.check_temp_token(&token).await,
        Err(AuthError::InvalidToken)
    );

    assert!(matches!(
        kit.create_token(SaTempToken::default(), 60).await,
        Err(AuthError::InvalidLoginId)
    ));
}

#[tokio::test]
async fn api_keys_require_all_scopes_and_support_revocation() {
    let kit = api_keys();
    let (secret, record) = kit
        .issue(["internal".to_owned(), "mq".to_owned()], 60)
        .await
        .unwrap();
    assert!(!format!("{record:?}").contains(&secret));
    assert_eq!(
        kit.check(&secret, &["internal".to_owned(), "mq".to_owned()])
            .await
            .unwrap()
            .key_id,
        record.key_id
    );
    assert!(matches!(
        kit.check(&secret, &["admin".to_owned()]).await,
        Err(AuthError::AccessDenied { .. })
    ));

    let handler = SaInternalCheckHandler::new(kit.clone());
    handler
        .check(
            Some(&secret),
            &SaInternalCheck {
                scopes: vec!["internal".to_owned()],
            },
        )
        .await
        .unwrap();
    assert!(kit.revoke(&secret).await.unwrap());
    assert!(matches!(
        handler
            .check(Some(&secret), &SaInternalCheck::default())
            .await,
        Err(AuthError::AccessDenied { .. })
    ));
}

#[tokio::test]
async fn mixed_login_promotes_reusable_tokens_and_consumes_throwaway_tokens() {
    let temp_tokens = SaTempKit::default();
    let subject: Arc<dyn Subject> = Arc::new(SubjectEngine::default());
    let handler = SaMixCheckLoginHandler::new(temp_tokens.clone(), Arc::clone(&subject));

    SubjectScope::run(async {
        let reusable = temp_tokens
            .create_token(temporary_payload("user-1"), 60)
            .await
            .unwrap();
        let outcome = handler
            .check(
                Some(&reusable),
                &SaMixCheckLogin {
                    account_type: Some("admin".to_owned()),
                    ..SaMixCheckLogin::default()
                },
            )
            .await
            .unwrap();
        assert!(outcome.session_token.is_some());
        assert!(!outcome.consumed);
        assert!(subject.is_authenticated().await.unwrap());
        assert_eq!(
            subject
                .principal()
                .await
                .unwrap()
                .unwrap()
                .profile
                .get("auth_type"),
            Some(&json!("qrcode"))
        );
        subject.logout().await.unwrap();

        let throwaway = temp_tokens
            .create_token(temporary_payload("user-2"), 60)
            .await
            .unwrap();
        let outcome = handler
            .check(
                Some(&throwaway),
                &SaMixCheckLogin {
                    throwaway: true,
                    login: true,
                    ..SaMixCheckLogin::default()
                },
            )
            .await
            .unwrap();
        assert!(outcome.consumed);
        assert!(outcome.session_token.is_none());
        assert_eq!(
            handler
                .check(Some(&throwaway), &SaMixCheckLogin::default())
                .await,
            Err(AuthError::InvalidToken)
        );
    })
    .await;
}

#[tokio::test]
async fn stp_kit_uses_real_principal_fields_and_typed_extras() {
    let subject: Arc<dyn Subject> = Arc::new(SubjectEngine::default());
    let stp = StpKit::new(Arc::clone(&subject));
    SubjectScope::run(async {
        let principal = AuthPrincipal {
            login_id: Some(AuthId::from("42")),
            user_id: Some(AuthId::from("100")),
            org_id: Some(AuthId::from("200")),
            role_id: Some(AuthId::from("300")),
            permissions: BTreeSet::new(),
            ..AuthPrincipal::default()
        };
        subject
            .login(
                AuthRequest::new("42")
                    .with_principal(principal)
                    .with_extra("info_id", json!(400))
                    .with_extra("xxdm", json!("school-1")),
            )
            .await
            .unwrap();

        assert_eq!(stp.login_id_as::<u64>().await.unwrap(), 42);
        assert_eq!(stp.user_id().await.unwrap(), Some(AuthId::from("100")));
        assert_eq!(stp.org_id().await.unwrap(), Some(AuthId::from("200")));
        assert_eq!(stp.role_id().await.unwrap(), Some(AuthId::from("300")));
        assert_eq!(stp.info_id_as::<u64>().await.unwrap(), Some(400));
        assert_eq!(
            stp.school_code().await.unwrap().as_deref(),
            Some("school-1")
        );
    })
    .await;
}
