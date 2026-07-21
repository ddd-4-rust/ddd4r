//! Executable compatibility contract for the frozen ddd4j cache module.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use ddd4r_cache::{
    Cache, CacheConfig, CacheKey, CacheKit, InMemoryCache, STOCK_ILLEGAL_ARGUMENT,
    STOCK_NOT_ENOUGH, STOCK_NOT_INITIALIZED, STOCK_ZERO,
};
use ddd4r_core::DddResult;

#[tokio::test]
async fn ttl_tti_persist_capacity_and_stats_are_enforced() {
    let config = CacheConfig::builder("policy")
        .unwrap()
        .maximum_size(2)
        .expire_after_access(Duration::from_millis(30))
        .record_stats(true)
        .build()
        .unwrap();
    let cache = InMemoryCache::with_config(config);
    let first = CacheKey::new("policy", "first");
    cache
        .put(first.clone(), b"one".to_vec(), Some(Duration::from_secs(2)))
        .await
        .unwrap();
    assert!(cache.persist(&first).await.unwrap());
    assert_eq!(cache.remaining_ttl(&first).await.unwrap(), Some(None));
    cache
        .put(CacheKey::new("policy", "second"), vec![2], None)
        .await
        .unwrap();
    cache
        .put(CacheKey::new("policy", "third"), vec![3], None)
        .await
        .unwrap();
    assert_eq!(cache.estimated_size(), 2);
    assert_eq!(cache.stats().evictions, 1);

    tokio::time::sleep(Duration::from_millis(35)).await;
    assert!(
        cache
            .get(&CacheKey::new("policy", "third"))
            .await
            .unwrap()
            .is_none()
    );
    assert!(cache.stats().misses >= 1);
}

#[tokio::test]
async fn cas_counter_and_stock_operations_are_atomic_and_typed() {
    let cache = InMemoryCache::new();
    let key = CacheKey::new("numbers", "counter");
    assert!(
        cache
            .put_if_absent(key.clone(), b"10".to_vec(), None)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        cache
            .put_if_absent(key.clone(), b"20".to_vec(), None)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(cache.increment(key.clone(), 5, None).await.unwrap(), 15);
    assert!(
        cache
            .replace(key.clone(), b"15".to_vec(), b"8".to_vec(), None)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(cache.stock_decrement(key.clone(), 3).await.unwrap(), 5);
    assert_eq!(
        cache.stock_decrement(key.clone(), 6).await.unwrap(),
        STOCK_NOT_ENOUGH
    );
    assert_eq!(
        cache.stock_decrement(key.clone(), 0).await.unwrap(),
        STOCK_ILLEGAL_ARGUMENT
    );
    assert_eq!(
        cache
            .stock_decrement(CacheKey::new("numbers", "missing"), 1)
            .await
            .unwrap(),
        STOCK_NOT_INITIALIZED
    );
    assert_eq!(
        cache
            .stock_increment(CacheKey::new("numbers", "new-stock"), 9)
            .await
            .unwrap(),
        9
    );
    cache
        .replace(key.clone(), b"5".to_vec(), b"0".to_vec(), None)
        .await
        .unwrap();
    assert_eq!(cache.stock_decrement(key, 1).await.unwrap(), STOCK_ZERO);
}

#[tokio::test]
async fn loading_cache_uses_singleflight_and_typed_json_helpers() {
    let kit = CacheKit::new();
    let loads = Arc::new(AtomicUsize::new(0));
    let loader_loads = loads.clone();
    let loader = Arc::new(move |_key: CacheKey| {
        let loader_loads = loader_loads.clone();
        Box::pin(async move {
            loader_loads.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok(Some(serde_json::to_vec(&vec!["loaded"]).unwrap()))
        }) as futures::future::BoxFuture<'static, DddResult<Option<Vec<u8>>>>
    });
    kit.build_loading(
        CacheConfig::builder("singleflight")
            .unwrap()
            .build()
            .unwrap(),
        loader,
    )
    .unwrap();

    let (left, right) = tokio::join!(
        kit.get_typed::<Vec<String>>("singleflight", "key"),
        kit.get_typed::<Vec<String>>("singleflight", "key")
    );
    assert_eq!(left.unwrap().unwrap(), vec!["loaded"]);
    assert_eq!(right.unwrap().unwrap(), vec!["loaded"]);
    assert_eq!(loads.load(Ordering::SeqCst), 1);
    assert_eq!(kit.stats("singleflight").unwrap().loads, 1);
}

#[tokio::test]
async fn lock_leases_enforce_owner_and_expire_after_panic_or_abandonment() {
    let cache = InMemoryCache::new();
    let key = CacheKey::new("locks", "order-1");
    let first = cache
        .try_lock(key.clone(), Duration::ZERO, Duration::from_millis(25))
        .await
        .unwrap()
        .unwrap();
    assert!(
        cache
            .try_lock(key.clone(), Duration::ZERO, Duration::from_secs(1))
            .await
            .unwrap()
            .is_none()
    );
    let mut forged = first.clone();
    forged.owner = uuid::Uuid::now_v7();
    assert!(!cache.unlock(&forged).await.unwrap());
    tokio::time::sleep(Duration::from_millis(30)).await;
    let second = cache
        .try_lock(key, Duration::ZERO, Duration::from_secs(1))
        .await
        .unwrap()
        .unwrap();
    assert!(cache.unlock(&second).await.unwrap());
    assert!(!cache.unlock(&first).await.unwrap());
}

#[tokio::test]
async fn registry_facade_covers_namespaces_refresh_counters_and_locks() {
    let kit = CacheKit::new();
    kit.build(CacheConfig::builder("orders").unwrap().build().unwrap())
        .unwrap();
    kit.put("orders", "one", b"5".to_vec(), None).await.unwrap();
    assert_eq!(kit.increment("orders", "one", 2).await.unwrap(), 7);
    assert_eq!(kit.decrement("orders", "one", 3).await.unwrap(), 4);
    assert_eq!(kit.decrement("orders", "missing", 3).await.unwrap(), -3);
    let incremented = kit.increment_float("orders", "ratio", 1.5).await.unwrap();
    assert!((incremented - 1.5).abs() < f64::EPSILON);
    let decremented = kit.decrement_float("orders", "ratio", 0.25).await.unwrap();
    assert!((decremented - 1.25).abs() < f64::EPSILON);
    let result = kit
        .with_lock(
            "orders",
            "settle",
            Duration::ZERO,
            Duration::from_secs(1),
            || async { Ok::<_, ddd4r_core::DddError>("done") },
        )
        .await
        .unwrap();
    assert_eq!(result, Some("done"));
    assert_eq!(kit.cache_names(), vec!["orders"]);
    assert_eq!(kit.invalidate_all("orders").await.unwrap(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_counters_do_not_lose_updates() {
    let cache = Arc::new(InMemoryCache::new());
    let key = CacheKey::new("concurrent", "counter");
    let mut tasks = Vec::new();
    for _ in 0..100 {
        let cache = cache.clone();
        let key = key.clone();
        tasks.push(tokio::spawn(async move {
            cache.increment(key, 1, None).await.unwrap();
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }
    assert_eq!(
        cache.get(&key).await.unwrap().unwrap().bytes,
        b"100".to_vec()
    );
}
