//! `SeaORM` `SQLite` shared repository conformance.

use ddd4r_data::{ConformanceAggregate, DataBackend, verify_repository_conformance};
use ddd4r_data_seaorm::{SeaOrmBackend, SeaOrmRepository};
use sea_orm::{ConnectionTrait, DbBackend, Statement};

#[tokio::test]
async fn sqlite_passes_the_shared_crud_query_page_and_lock_contract() {
    let repository = SeaOrmRepository::connect_memory().await.unwrap();
    let report = verify_repository_conformance(&repository).await.unwrap();
    assert!(report.crud_and_batch);
    assert!(report.query_and_page);
    assert!(report.optimistic_lock);
    assert!(!SeaOrmBackend.capabilities().is_complete());
}

#[tokio::test]
async fn explicit_transactions_commit_and_rollback_on_the_same_connection() {
    let repository = SeaOrmRepository::<ConformanceAggregate>::connect_memory()
        .await
        .unwrap();
    let tx = repository.begin().await.unwrap();
    tx.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO ddd4r_aggregate \
         (aggregate_type, aggregate_id, version, payload) VALUES (?, ?, ?, ?)",
        [
            "tx-test".into(),
            "rollback".into(),
            1_i64.into(),
            "{}".into(),
        ],
    ))
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    let rolled_back = repository
        .database()
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM ddd4r_aggregate WHERE aggregate_type = ?",
            ["tx-test".into()],
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get_by::<i64, _>("count")
        .unwrap();
    assert_eq!(rolled_back, 0);

    let tx = repository.begin().await.unwrap();
    tx.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO ddd4r_aggregate \
         (aggregate_type, aggregate_id, version, payload) VALUES (?, ?, ?, ?)",
        ["tx-test".into(), "commit".into(), 1_i64.into(), "{}".into()],
    ))
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let committed = repository
        .database()
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM ddd4r_aggregate WHERE aggregate_type = ?",
            ["tx-test".into()],
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get_by::<i64, _>("count")
        .unwrap();
    assert_eq!(committed, 1);
}
