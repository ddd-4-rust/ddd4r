//! Shiro Realm and `SessionDAO` bridge tests.

use std::sync::Arc;

use ddd4r_auth::{
    AuthError, AuthId, AuthPrincipal, AuthRequest, InMemorySessionStore, SessionStore,
    SubjectEngine, SubjectProvider, SubjectScope,
};
use ddd4r_auth_shiro::{
    ShiroAuthenticationToken, ShiroRealmAuthenticator, ShiroSessionDao, ShiroSubjectProvider,
};
use futures::future::BoxFuture;

struct TestRealm;

impl ShiroRealmAuthenticator for TestRealm {
    fn authenticate<'a>(
        &'a self,
        token: &'a ShiroAuthenticationToken,
    ) -> BoxFuture<'a, ddd4r_auth::AuthResult<AuthPrincipal>> {
        Box::pin(async move {
            if token.verify_credential("secret", |presented, expected| presented == expected) {
                Ok(AuthPrincipal::for_login(token.login_id().clone()))
            } else {
                Err(AuthError::BadCredentials)
            }
        })
    }
}

#[tokio::test]
async fn realm_authenticates_before_session_creation_and_dao_can_revoke() {
    let store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());
    let engine = Arc::new(SubjectEngine::new(Arc::clone(&store)));
    let provider = ShiroSubjectProvider::with_authenticator(engine, Arc::new(TestRealm));
    let subject = provider.subject_for_realm(Some("admin"));
    let dao = ShiroSessionDao::new(store);

    SubjectScope::run(async {
        assert_eq!(
            subject
                .login(AuthRequest::new("user-1").with_extra("credential", "bad"))
                .await,
            Err(AuthError::BadCredentials)
        );
        let token = subject
            .login(AuthRequest::new("user-1").with_extra("credential", "secret"))
            .await
            .unwrap();
        assert_eq!(
            dao.read(&token).await.unwrap().unwrap().principal.login_id,
            Some(AuthId::from("user-1"))
        );
        assert!(dao.delete(&token).await.unwrap().is_some());
        assert_eq!(subject.verify(&token).await, Err(AuthError::InvalidToken));
    })
    .await;
}

#[test]
fn authentication_token_redacts_credentials() {
    let token = ShiroAuthenticationToken::new(
        AuthId::from("user-1"),
        "super-secret",
        Some("admin".to_owned()),
        true,
    );
    assert_eq!(token.realm(), Some("admin"));
    assert!(token.remember_me());
    assert!(!format!("{token:?}").contains("super-secret"));
}
