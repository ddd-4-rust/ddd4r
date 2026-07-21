//! `SQLx` `SQLite` shared repository conformance.

use ddd4r_data::{DataBackend, verify_repository_conformance};
use ddd4r_data_sqlx::{SqlxBackend, SqlxRepository};

#[tokio::test]
async fn sqlite_passes_the_shared_crud_query_page_and_lock_contract() {
    let repository = SqlxRepository::connect_memory().await.unwrap();
    let report = verify_repository_conformance(&repository).await.unwrap();
    assert!(report.crud_and_batch);
    assert!(report.query_and_page);
    assert!(report.optimistic_lock);
    assert!(!SqlxBackend.capabilities().is_complete());
}
