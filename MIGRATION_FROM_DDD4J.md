# 从 ddd4j 迁移到 ddd4r

## 兼容目标

ddd4r 移植 ddd4j 的领域和应用语义，不尝试在 Rust 中模拟 JVM、Spring 或 Java 反射。

| ddd4j | ddd4r |
|---|---|
| `ThreadContext` | Tokio `task_local!` + `ContextScope` |
| `BaseContext` | 全局 `Registry` |
| `RepositoryRegistry.repository(Order.class)` | `RepositoryRegistry::repository::<Order>()` |
| `aggregate.save()` | `aggregate.save().await` |
| `query.list()` | `query.list().await` |
| `event.publish()` | `event.publish().await` |
| Java Lambda 属性引用 | proc macro 生成的 `PropertyRef` |
| `CompletableFuture` / Reactor | Rust `Future` / `Stream` |
| ArchUnit | Cargo Metadata + `syn` 架构规则 |

## 聚合示例

```rust,ignore
use ddd4r::prelude::*;

let mut order = Order::create(order_id, total)?;
order.save().await?;

let paid = Order::query()
    .and(OrderFields::STATUS.eq(OrderStatus::Paid)?)
    .page(1, 20)
    .list_page()
    .await?;
```

## 明确差异

- Rust trait 方法需要通过 `ddd4r::prelude::*` 引入。
- Java 注解扫描改为 proc macro 和显式注册。
- 异步上下文跟随 Tokio task，而不是操作系统线程。
- ddd4r 首版补充 Unit of Work 和 Transactional Outbox，以闭合聚合保存到事件投递的链路。
