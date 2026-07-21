//! RBatis-Plus compatibility repository conformance.

use ddd4r_data::{
    DataBackend, verify_repository_conformance, verify_transactional_outbox_conformance,
};
use ddd4r_data_rbatisplus::{RbatisPlusBackend, RbatisPlusRepository};

#[tokio::test]
async fn sqlite_repository_is_an_executable_plus_backend() {
    let repository = RbatisPlusRepository::connect_memory().await.unwrap();
    let report = verify_repository_conformance(&repository).await.unwrap();
    assert!(report.crud_and_batch);
    assert!(report.query_and_page);
    assert!(report.optimistic_lock);
    assert!(!RbatisPlusBackend.capabilities().is_complete());
}

#[tokio::test]
async fn sqlite_repository_preserves_atomic_outbox_semantics() {
    let repository = RbatisPlusRepository::connect_memory().await.unwrap();
    let report = verify_transactional_outbox_conformance(&repository)
        .await
        .unwrap();
    assert!(report.atomic_commit);
    assert!(report.atomic_rollback);
    assert!(report.event_buffer_lifecycle);
}

#[test]
fn plus_wrapper_surface_is_available() {
    struct Order;
    let id = ddd4r_data_rbatisplus::Column::<Order>::new("id");
    let wrapper = ddd4r_data_rbatisplus::QueryWrapper::default()
        .eq(&id, 42)
        .unwrap();
    assert_eq!(wrapper.predicates()[0].column, "id");
}
