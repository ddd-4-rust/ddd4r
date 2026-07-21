//! `RBatis` `SQLite` shared repository conformance.

use ddd4r_data::{
    ConformanceAggregate, DataBackend, verify_repository_conformance,
    verify_transactional_outbox_conformance,
};
use ddd4r_data_rbatis::{RbatisBackend, RbatisRepository};
use rbs::Value;

#[tokio::test]
async fn sqlite_passes_the_shared_crud_query_page_and_lock_contract() {
    let repository = RbatisRepository::connect_memory().await.unwrap();
    let report = verify_repository_conformance(&repository).await.unwrap();
    assert!(report.crud_and_batch);
    assert!(report.query_and_page);
    assert!(report.optimistic_lock);
    assert!(!RbatisBackend.capabilities().is_complete());
}

#[tokio::test]
async fn sqlite_commits_aggregate_and_outbox_atomically() {
    let repository = RbatisRepository::connect_memory().await.unwrap();
    let report = verify_transactional_outbox_conformance(&repository)
        .await
        .unwrap();
    assert!(report.atomic_commit);
    assert!(report.atomic_rollback);
    assert!(report.event_buffer_lifecycle);
}

#[tokio::test]
async fn explicit_transactions_commit_and_rollback_on_the_same_executor() {
    let repository = RbatisRepository::<ConformanceAggregate>::connect_memory()
        .await
        .unwrap();
    let tx = repository.begin().await.unwrap();
    tx.exec(
        "INSERT INTO ddd4r_aggregate \
         (aggregate_type, aggregate_id, version, payload) VALUES (?, ?, ?, ?)",
        vec![
            Value::String("tx-test".to_owned()),
            Value::String("rollback".to_owned()),
            Value::I64(1),
            Value::String("{}".to_owned()),
        ],
    )
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    let rolled_back: i64 = repository
        .rbatis()
        .exec_decode(
            "SELECT COUNT(*) FROM ddd4r_aggregate WHERE aggregate_type = ?",
            vec![Value::String("tx-test".to_owned())],
        )
        .await
        .unwrap();
    assert_eq!(rolled_back, 0);

    let tx = repository.begin().await.unwrap();
    tx.exec(
        "INSERT INTO ddd4r_aggregate \
         (aggregate_type, aggregate_id, version, payload) VALUES (?, ?, ?, ?)",
        vec![
            Value::String("tx-test".to_owned()),
            Value::String("commit".to_owned()),
            Value::I64(1),
            Value::String("{}".to_owned()),
        ],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let committed: i64 = repository
        .rbatis()
        .exec_decode(
            "SELECT COUNT(*) FROM ddd4r_aggregate WHERE aggregate_type = ?",
            vec![Value::String("tx-test".to_owned())],
        )
        .await
        .unwrap();
    assert_eq!(committed, 1);
}
