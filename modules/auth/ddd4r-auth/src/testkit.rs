//! Shared conformance suite for Subject adapters.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde_json::json;

use crate::{
    AuthError, AuthId, AuthPrincipal, AuthRequest, AuthResult, RolePair, Subject, SubjectScope,
};

/// Executes the reusable login, RBAC, session, rotation and disable contract.
///
/// # Panics
///
/// Panics when an adapter violates any conformance assertion.
pub async fn assert_subject_conformance(subject: Arc<dyn Subject>) -> AuthResult<()> {
    SubjectScope::run(async move {
        let login_id = AuthId::from("user-1");
        let mut permissions = BTreeSet::new();
        permissions.insert("order:read".to_owned());
        permissions.insert("order:write".to_owned());
        let principal = AuthPrincipal {
            login_id: Some(login_id.clone()),
            user_id: Some(AuthId::from(100_u64)),
            role_code: Some("admin".to_owned()),
            roles: vec![RolePair {
                role_id: Some(AuthId::from("role-1")),
                role_code: Some("auditor".to_owned()),
                role_name: Some("Auditor".to_owned()),
                verify: false,
            }],
            permissions,
            ..AuthPrincipal::default()
        };
        let mut request = AuthRequest::new(login_id.clone())
            .with_principal(principal)
            .with_device_type("desktop")
            .with_extra("department", json!("engineering"));
        request.session.device_id = Some("device-1".to_owned());

        let token = subject.login(request).await?;
        assert!(!token.is_empty());
        assert_eq!(
            subject.verify(&token).await?.login_id,
            Some(login_id.clone())
        );
        assert!(subject.is_authenticated().await?);
        assert!(subject.is_permitted("order:read").await?);
        assert!(!subject.is_permitted("order:delete").await?);
        assert!(
            subject
                .is_permitted_any(&["order:delete", "order:write"])
                .await?
        );
        assert!(
            subject
                .is_permitted_all(&["order:read", "order:write"])
                .await?
        );
        assert!(!subject.is_permitted_all(&[]).await?);
        assert!(subject.has_role("admin").await?);
        assert!(subject.has_role("role-1").await?);
        assert!(subject.has_any_role(&["guest", "auditor"]).await?);
        assert!(subject.has_all_roles(&["admin", "auditor"]).await?);
        assert!(subject.is_trusted_device("device-1").await?);

        subject
            .set_attribute("trace_id".to_owned(), json!("trace-1"))
            .await?;
        assert_eq!(
            subject.get_attribute("trace_id").await?,
            Some(json!("trace-1"))
        );
        assert_eq!(
            subject.get_extra(&token, "department").await?,
            Some(json!("engineering"))
        );

        let refreshed = subject.refresh().await?;
        assert_ne!(refreshed, token);
        assert_eq!(subject.verify(&token).await, Err(AuthError::InvalidToken));
        assert_eq!(
            subject.verify(&refreshed).await?.login_id,
            Some(login_id.clone())
        );

        subject.logout().await?;
        assert!(!subject.is_authenticated().await?);
        assert_eq!(
            subject.verify(&refreshed).await,
            Err(AuthError::InvalidToken)
        );

        subject.disable(&login_id, -1).await?;
        assert!(subject.is_disabled(&login_id).await?);
        assert!(matches!(
            subject.login(AuthRequest::new(login_id.clone())).await,
            Err(AuthError::AccountDisabled { .. })
        ));
        subject.untie_disable(&login_id).await?;
        assert!(!subject.is_disabled(&login_id).await?);
        let final_token = subject.login(AuthRequest::new(login_id)).await?;
        assert!(!final_token.is_empty());
        subject.logout().await?;
        Ok(())
    })
    .await
}
