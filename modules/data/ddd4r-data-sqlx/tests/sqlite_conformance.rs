//! `SQLx` `SQLite` shared repository conformance.

use ddd4r_data::{ConformanceAggregate, DataBackend, verify_repository_conformance};
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

#[tokio::test]
async fn explicit_transactions_commit_and_rollback_on_the_same_connection() {
    let repository = SqlxRepository::<ConformanceAggregate>::connect_memory()
        .await
        .unwrap();
    let mut tx = repository.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO ddd4r_aggregate \
         (aggregate_type, aggregate_id, version, payload) VALUES (?, ?, ?, ?)",
    )
    .bind("tx-test")
    .bind("rollback")
    .bind(1_i64)
    .bind("{}")
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    let rolled_back: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ddd4r_aggregate WHERE aggregate_type = ?")
            .bind("tx-test")
            .fetch_one(repository.pool())
            .await
            .unwrap();
    assert_eq!(rolled_back, 0);

    let mut tx = repository.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO ddd4r_aggregate \
         (aggregate_type, aggregate_id, version, payload) VALUES (?, ?, ?, ?)",
    )
    .bind("tx-test")
    .bind("commit")
    .bind(1_i64)
    .bind("{}")
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let committed: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ddd4r_aggregate WHERE aggregate_type = ?")
            .bind("tx-test")
            .fetch_one(repository.pool())
            .await
            .unwrap();
    assert_eq!(committed, 1);
}
