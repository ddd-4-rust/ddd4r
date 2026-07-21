//! Authentication model, task-scope and security contract tests.

use std::collections::BTreeSet;
use std::sync::Arc;

use ddd4r_auth::{
    AuthError, AuthId, AuthPrincipal, AuthReplacedLoginExitMode, AuthRequest, BearerAuthenticator,
    Subject, SubjectEngine, SubjectScope,
};

#[test]
fn model_defaults_and_java_compatible_identity_semantics_are_stable() {
    let request_a = AuthRequest::new("user-1").with_realm("admin");
    let request_b = AuthRequest::new("user-1").with_timeout(600);
    assert_eq!(request_a, request_b);
    assert_eq!(AuthId::from(123_u64).as_str(), "123");
    assert_eq!(AuthId::from("123").parse::<u64>(), Ok(123));

    let principal = AuthPrincipal::default();
    assert!(!principal.bound);
    assert!(!principal.initial);
    assert!(!principal.verify);
    assert!(principal.permissions.is_empty());
}

#[tokio::test]
async fn subject_scope_is_nested_and_task_isolated() {
    SubjectScope::run_with_token("outer", async {
        assert_eq!(SubjectScope::current_token().as_deref(), Some("outer"));
        SubjectScope::nested(async {
            assert_eq!(SubjectScope::current_token().as_deref(), Some("outer"));
            SubjectScope::bind("inner").unwrap();
            assert_eq!(SubjectScope::current_token().as_deref(), Some("inner"));
        })
        .await;
        assert_eq!(SubjectScope::current_token().as_deref(), Some("outer"));

        let child = tokio::spawn(SubjectScope::run_with_token("child", async {
            SubjectScope::current_token()
        }));
        assert_eq!(child.await.unwrap().as_deref(), Some("child"));
        assert_eq!(SubjectScope::current_token().as_deref(), Some("outer"));
    })
    .await;
    assert_eq!(SubjectScope::current_token(), None);
}

#[tokio::test]
async fn subject_scope_is_cleaned_after_cancellation_and_panic() {
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let cancelled = tokio::spawn(SubjectScope::run_with_token("cancelled", async move {
        started_tx.send(()).unwrap();
        std::future::pending::<()>().await;
    }));
    started_rx.await.unwrap();
    cancelled.abort();
    assert!(cancelled.await.unwrap_err().is_cancelled());

    let panicked = tokio::spawn(SubjectScope::run_with_token("panicked", async {
        panic!("scope cleanup contract");
    }));
    assert!(panicked.await.unwrap_err().is_panic());

    assert_eq!(
        tokio::spawn(async { SubjectScope::current_token() })
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn bearer_authentication_verifies_before_binding() {
    let subject = SubjectEngine::default();
    SubjectScope::run(async {
        let token = subject.login(AuthRequest::new("user-1")).await.unwrap();
        SubjectScope::clear();
        let principal =
            BearerAuthenticator::authenticate(&subject, Some(&format!("Bearer {token}")))
                .await
                .unwrap();
        assert_eq!(principal.login_id, Some(AuthId::from("user-1")));
        assert_eq!(
            SubjectScope::current_token().as_deref(),
            Some(token.as_str())
        );

        SubjectScope::clear();
        assert_eq!(
            BearerAuthenticator::authenticate(&subject, Some("Basic value")).await,
            Err(AuthError::InvalidToken)
        );
        assert_eq!(SubjectScope::current_token(), None);
    })
    .await;
}

#[tokio::test]
async fn rbac_is_fail_closed_for_missing_or_blank_authorities() {
    let subject = SubjectEngine::default();
    SubjectScope::run(async {
        assert!(!subject.is_permitted("order:read").await.unwrap());
        let mut principal = AuthPrincipal::for_login("user-1");
        principal.permissions = BTreeSet::from(["order:read".to_owned()]);
        subject
            .login(AuthRequest::new("user-1").with_principal(principal))
            .await
            .unwrap();
        assert!(!subject.is_permitted("").await.unwrap());
        assert!(!subject.is_permitted_all(&[]).await.unwrap());
        assert!(!subject.has_all_roles(&[]).await.unwrap());
    })
    .await;
}

#[tokio::test]
async fn concurrency_policy_can_reject_or_replace_existing_session() {
    let subject = Arc::new(SubjectEngine::default());
    let first = Arc::clone(&subject);
    let token = SubjectScope::run(async move {
        let mut request = AuthRequest::new("user-1").with_device_type("desktop");
        request.session.share = false;
        first.login(request).await.unwrap()
    })
    .await;

    let mut rejected = AuthRequest::new("user-1").with_device_type("desktop");
    rejected.session.share = false;
    rejected.session.concurrent = false;
    assert!(matches!(
        subject.login(rejected).await,
        Err(AuthError::ConcurrentLoginRejected { .. })
    ));

    let mut replacing = AuthRequest::new("user-1").with_device_type("desktop");
    replacing.session.share = false;
    replacing.session.concurrent = false;
    replacing.session.replaced_login_exit_mode = AuthReplacedLoginExitMode::OldDevice;
    let replacement = subject.login(replacing).await.unwrap();
    assert_ne!(replacement, token);
    assert_eq!(subject.verify(&token).await, Err(AuthError::InvalidToken));
}

#[tokio::test]
async fn absolute_timeout_and_disable_are_fail_closed() {
    let subject = SubjectEngine::default();
    SubjectScope::run(async {
        let token = subject
            .login(AuthRequest::new("user-1").with_timeout(0))
            .await
            .unwrap();
        assert!(matches!(
            subject.verify(&token).await,
            Err(AuthError::SessionExpired | AuthError::InvalidToken)
        ));

        let login_id = AuthId::from("user-2");
        subject.disable(&login_id, -1).await.unwrap();
        assert!(matches!(
            subject.login(AuthRequest::new(login_id)).await,
            Err(AuthError::AccountDisabled { .. })
        ));
    })
    .await;
}
