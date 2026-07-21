//! Security migration adapter conformance tests.

use std::sync::Arc;

use ddd4r_auth::SubjectProvider;
use ddd4r_auth::testkit::assert_subject_conformance;
use ddd4r_auth_security::SecuritySubjectProvider;

#[tokio::test]
async fn security_adapter_passes_shared_subject_contract() {
    let provider = SecuritySubjectProvider::default();
    let first = provider.subject();
    let second = provider.subject();
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(first.adapter_name(), "security");
    assert_subject_conformance(first).await.unwrap();
}
