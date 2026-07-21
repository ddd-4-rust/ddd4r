//! Shiro migration adapter conformance tests.

use std::sync::Arc;

use ddd4r_auth::SubjectProvider;
use ddd4r_auth::testkit::assert_subject_conformance;
use ddd4r_auth_shiro::ShiroSubjectProvider;

#[tokio::test]
async fn shiro_adapter_passes_shared_subject_contract() {
    let provider = ShiroSubjectProvider::default();
    let first = provider.subject_for_realm(Some("admin"));
    let second = provider.subject_for_realm(Some("admin"));
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(first.adapter_name(), "shiro");
    assert_subject_conformance(first).await.unwrap();
}
