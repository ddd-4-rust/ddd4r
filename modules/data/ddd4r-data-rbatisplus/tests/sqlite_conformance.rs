//! RBatis-Plus compatibility repository conformance.

use ddd4r_data::{
    DataBackend, verify_repository_conformance, verify_transactional_outbox_conformance,
};
use ddd4r_data_rbatisplus::{RbatisPlusBackend, RbatisPlusRepository};
use rbatis::RBatis;
use rbdc_sqlite::SqliteDriver;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ddd4r_data_rbatisplus::PlusModel)]
#[rbatis_plus(
    table_name = "plus_orders",
    id_column = "id",
    version_column = "version",
    logic_delete_column = "deleted",
    crate_path = "ddd4r_data_rbatisplus"
)]
struct PlusOrderPo {
    id: i64,
    name: String,
    version: i64,
    deleted: i64,
}

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

#[tokio::test]
async fn native_rbatis_plus_mapper_executes_optimistic_and_logical_delete_contracts() {
    use ddd4r_data_rbatisplus::{BaseMapper, Column, QueryWrapper, RbatisMapper};

    let rbatis = RBatis::new();
    rbatis
        .link(SqliteDriver {}, "sqlite://:memory:")
        .await
        .unwrap();
    rbatis
        .exec(
            "CREATE TABLE plus_orders (id INTEGER PRIMARY KEY, name TEXT NOT NULL, \
             version INTEGER NOT NULL, deleted INTEGER NOT NULL)",
            vec![],
        )
        .await
        .unwrap();
    let mapper = RbatisMapper::<PlusOrderPo, i64>::new(rbatis).unwrap();
    let inserted = PlusOrderPo {
        id: 7,
        name: "created".to_owned(),
        version: 0,
        deleted: 0,
    };
    mapper.insert(inserted.clone()).await.unwrap();

    let updated = mapper
        .update_by_id(PlusOrderPo {
            name: "paid".to_owned(),
            ..inserted.clone()
        })
        .await
        .unwrap();
    assert_eq!(updated.version, 1);
    let stale = mapper.update_by_id(inserted).await.unwrap_err();
    assert!(stale.to_string().contains("optimistic lock conflict"));

    let name = Column::<PlusOrderPo>::new("name");
    let found = mapper
        .select_list(QueryWrapper::default().eq(&name, "paid").unwrap())
        .await
        .unwrap();
    assert_eq!(found, [updated]);
    assert!(mapper.delete_by_id(7).await.unwrap());
    assert!(mapper.select_by_id(7).await.unwrap().is_none());
}

#[tokio::test]
async fn native_mapper_executes_transactional_upsert_and_batch_rollback_contracts() {
    use ddd4r_data_rbatisplus::{BaseMapper, RbatisMapper};

    let rbatis = RBatis::new();
    rbatis
        .link(SqliteDriver {}, "sqlite://:memory:")
        .await
        .unwrap();
    rbatis
        .exec(
            "CREATE TABLE plus_orders (id INTEGER PRIMARY KEY, name TEXT NOT NULL, \
             version INTEGER NOT NULL, deleted INTEGER NOT NULL)",
            vec![],
        )
        .await
        .unwrap();
    let mapper = RbatisMapper::<PlusOrderPo, i64>::new(rbatis).unwrap();

    let inserted = mapper
        .save_or_update_batch(vec![
            PlusOrderPo {
                id: 1,
                name: "first".to_owned(),
                version: 0,
                deleted: 0,
            },
            PlusOrderPo {
                id: 2,
                name: "second".to_owned(),
                version: 0,
                deleted: 0,
            },
        ])
        .await
        .unwrap();
    assert_eq!(inserted.len(), 2);

    let mixed = mapper
        .save_or_update_batch(vec![
            PlusOrderPo {
                id: 1,
                name: "updated".to_owned(),
                version: 0,
                deleted: 0,
            },
            PlusOrderPo {
                id: 3,
                name: "new".to_owned(),
                version: 0,
                deleted: 0,
            },
        ])
        .await
        .unwrap();
    assert_eq!(mixed[0].version, 1);
    assert_eq!(mixed[1].version, 0);

    let error = mapper
        .update_batch_by_id(vec![
            PlusOrderPo {
                id: 1,
                name: "must-rollback".to_owned(),
                version: 1,
                deleted: 0,
            },
            PlusOrderPo {
                id: 99,
                name: "missing".to_owned(),
                version: 0,
                deleted: 0,
            },
        ])
        .await
        .unwrap_err();
    assert!(error.to_string().contains("optimistic lock conflict"));
    let unchanged = mapper.select_by_id(1).await.unwrap().unwrap();
    assert_eq!(unchanged.name, "updated");
    assert_eq!(unchanged.version, 1);

    let stale_error = mapper
        .update_batch_by_id(vec![
            PlusOrderPo {
                id: 2,
                name: "temporary".to_owned(),
                version: 0,
                deleted: 0,
            },
            PlusOrderPo {
                id: 2,
                name: "stale".to_owned(),
                version: 0,
                deleted: 0,
            },
        ])
        .await
        .unwrap_err();
    assert!(stale_error.to_string().contains("optimistic lock conflict"));
    let second = mapper.select_by_id(2).await.unwrap().unwrap();
    assert_eq!(second.name, "second");
    assert_eq!(second.version, 0);
}

#[test]
fn security_pipeline_is_reexported_and_fails_closed_on_tampering() {
    use ddd4r_data_rbatisplus::{
        AesGcmKeyRing, FieldCipher, PartialRowPolicy, RowSignatureService, SignatureScope,
        VerificationOutcome,
    };

    let cipher =
        AesGcmKeyRing::new("current", [("current".to_owned(), [9; 32])], [11; 32]).unwrap();
    let envelope = cipher.encrypt(b"sensitive", b"orders.secret").unwrap();
    assert_eq!(
        cipher.decrypt(&envelope, b"orders.secret").unwrap(),
        b"sensitive"
    );
    assert!(cipher.decrypt(&envelope, b"users.secret").is_err());

    let signer =
        RowSignatureService::new("current", [("current".to_owned(), vec![5; 32])]).unwrap();
    let row = json!({"id": 7, "secret": envelope});
    let signature = signer
        .sign(&row, &["id", "secret"], SignatureScope::FullRow)
        .unwrap();
    assert_eq!(
        signer
            .verify(
                &row,
                &["id", "secret"],
                &["id", "secret"],
                SignatureScope::FullRow,
                &signature,
                PartialRowPolicy::RejectPartial,
            )
            .unwrap(),
        VerificationOutcome::Verified
    );
    let tampered = json!({"id": 7, "secret": "modified"});
    assert!(
        signer
            .verify(
                &tampered,
                &["id", "secret"],
                &["id", "secret"],
                SignatureScope::FullRow,
                &signature,
                PartialRowPolicy::RejectPartial,
            )
            .is_err()
    );
}

#[tokio::test]
async fn native_mapper_applies_installed_parameter_interceptors() {
    use ddd4r_data_rbatisplus::{
        BaseMapper, EncryptedParameter, FieldEncryptionInterceptor, InterceptorChain, RbatisMapper,
    };
    use std::sync::Arc;

    let rbatis = RBatis::new();
    rbatis
        .link(SqliteDriver {}, "sqlite://:memory:")
        .await
        .unwrap();
    rbatis
        .exec(
            "CREATE TABLE plus_orders (id INTEGER PRIMARY KEY, name TEXT NOT NULL, \
             version INTEGER NOT NULL, deleted INTEGER NOT NULL)",
            vec![],
        )
        .await
        .unwrap();
    let cipher = Arc::new(
        ddd4r_data_rbatisplus::AesGcmKeyRing::new(
            "current",
            [("current".to_owned(), [9; 32])],
            [11; 32],
        )
        .unwrap(),
    );
    let chain = Arc::new(InterceptorChain::new(vec![Arc::new(
        FieldEncryptionInterceptor::new(
            cipher,
            vec![EncryptedParameter {
                index: 1,
                context: b"plus_orders.name".to_vec(),
            }],
        ),
    )]));
    let mapper = RbatisMapper::<PlusOrderPo, i64>::new(rbatis)
        .unwrap()
        .with_interceptors(chain);
    mapper
        .insert(PlusOrderPo {
            id: 9,
            name: "sensitive".to_owned(),
            version: 0,
            deleted: 0,
        })
        .await
        .unwrap();
    let stored: String = mapper
        .rbatis()
        .exec_decode("SELECT name FROM plus_orders WHERE id = 9", vec![])
        .await
        .unwrap();
    assert!(stored.starts_with("v1.current."));
}
