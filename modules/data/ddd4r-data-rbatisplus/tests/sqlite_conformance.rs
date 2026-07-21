//! RBatis-Plus compatibility repository conformance.

use ddd4r_data::{
    DataBackend, verify_repository_conformance, verify_transactional_outbox_conformance,
};
use ddd4r_data_rbatisplus::{RbatisPlusBackend, RbatisPlusRepository};
use rbatis::RBatis;
use rbdc_sqlite::SqliteDriver;
use serde::{Deserialize, Serialize};

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
