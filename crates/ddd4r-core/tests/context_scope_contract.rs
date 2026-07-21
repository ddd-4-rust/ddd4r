//! Tokio task-local isolation, nesting and cleanup contracts.

use std::future::pending;
use std::sync::Arc;

use ddd4r_core::context::{ContextScope, Contexts, Registry};
use tokio::sync::{Barrier, oneshot};
use uuid::Uuid;

fn unique_key(suffix: &str) -> String {
    format!("ddd4r.test.{suffix}.{}", Uuid::now_v7())
}

#[tokio::test]
async fn nested_scope_inherits_overrides_and_restores_parent() {
    let key = unique_key("nested");
    Contexts::register(key.as_str(), Arc::new(String::from("global"))).unwrap();

    let outer = Registry::new();
    outer
        .register(key.as_str(), Arc::new(String::from("outer")))
        .unwrap();

    ContextScope::run(outer, async {
        assert_eq!(
            Contexts::get::<String>(&key).unwrap().unwrap().as_str(),
            "outer"
        );

        let nested = ContextScope::nested(
            |registry| {
                registry
                    .replace(key.as_str(), Arc::new(String::from("nested")))
                    .map(|_| ())
            },
            async { Contexts::get::<String>(&key).unwrap().unwrap() },
        )
        .await
        .unwrap();

        assert_eq!(nested.as_str(), "nested");
        assert_eq!(
            Contexts::get::<String>(&key).unwrap().unwrap().as_str(),
            "outer"
        );
    })
    .await;

    assert_eq!(
        Contexts::get::<String>(&key).unwrap().unwrap().as_str(),
        "global"
    );
    Contexts::global().remove::<String>(&key).unwrap();
}

#[tokio::test]
async fn aborted_scope_does_not_leak_task_services() {
    let key = unique_key("cancel");
    Contexts::register(key.as_str(), Arc::new(String::from("global"))).unwrap();
    let local = Registry::new();
    local
        .register(key.as_str(), Arc::new(String::from("cancelled-task")))
        .unwrap();
    let (ready_tx, ready_rx) = oneshot::channel();
    let task_key = key.clone();

    let task = tokio::spawn(ContextScope::run(local, async move {
        assert_eq!(
            Contexts::get::<String>(&task_key)
                .unwrap()
                .unwrap()
                .as_str(),
            "cancelled-task"
        );
        ready_tx.send(()).unwrap();
        pending::<()>().await;
    }));
    ready_rx.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());

    assert_eq!(
        Contexts::get::<String>(&key).unwrap().unwrap().as_str(),
        "global"
    );
    Contexts::global().remove::<String>(&key).unwrap();
}

#[tokio::test]
async fn panicking_scope_does_not_leak_task_services() {
    let key = unique_key("panic");
    Contexts::register(key.as_str(), Arc::new(String::from("global"))).unwrap();
    let local = Registry::new();
    local
        .register(key.as_str(), Arc::new(String::from("panicking-task")))
        .unwrap();
    let task_key = key.clone();

    let task = tokio::spawn(ContextScope::run(local, async move {
        assert_eq!(
            Contexts::get::<String>(&task_key)
                .unwrap()
                .unwrap()
                .as_str(),
            "panicking-task"
        );
        panic!("intentional task-local cleanup test");
    }));
    assert!(task.await.unwrap_err().is_panic());

    assert_eq!(
        Contexts::get::<String>(&key).unwrap().unwrap().as_str(),
        "global"
    );
    Contexts::global().remove::<String>(&key).unwrap();
}

#[tokio::test]
async fn concurrent_scopes_are_isolated() {
    let key = unique_key("concurrent");
    let barrier = Arc::new(Barrier::new(2));

    let run = |value: &'static str| {
        let registry = Registry::new();
        registry
            .register(key.as_str(), Arc::new(value.to_owned()))
            .unwrap();
        let barrier = Arc::clone(&barrier);
        let key = key.clone();
        tokio::spawn(ContextScope::run(registry, async move {
            barrier.wait().await;
            Contexts::get::<String>(&key).unwrap().unwrap()
        }))
    };

    let first = run("first");
    let second = run("second");
    assert_eq!(first.await.unwrap().as_str(), "first");
    assert_eq!(second.await.unwrap().as_str(), "second");
    assert!(Contexts::get::<String>(&key).unwrap().is_none());
}
