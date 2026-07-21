# ddd4r 中文说明

`ddd4r` 是 `ddd4j` 的 Rust 语义移植项目，固定归属
[`ddd-4-rust/ddd4r`](https://github.com/ddd-4-rust/ddd4r)。项目使用 Rust 1.97.1、
Edition 2024 和 Tokio，并以 `MIT OR Apache-2.0` 双许可证发布。

当前版本为 `0.1.0-alpha.1`。仓库已经建立 82 个 Maven Reactor 项与 Cargo package
的一一映射，但未完成的包会明确标记为 `scaffolded`，不会作为稳定能力发布。

## 当前可用能力

- `DomainModel`、`Entity`、`ValueObject` 与派生宏。
- `AggregateRoot`、领域事件缓冲区和 UUIDv7 事件信封。
- `Repository`、`RepositoryRegistry` 与 Active Record 兼容门面。
- Tokio task-local 优先、全局 Registry 兜底的上下文查找。
- Query AST、分页、排序、CommandBus、Projection。
- `UnitOfWork`、`OutboxStore` 和内存 Outbox 状态机。
- 应用级 Cache、TTL、CAS 与统计。
- Cargo Metadata + `syn` 驱动的 Clean/COLA 架构规则检查。
- 三后端共享数据契约，以及通过 SQLite 实际执行验证的 `ddd4r-data-sqlx` 纵切。

SQLx 当前只完成 SQLite 下的 CRUD、批量、条件、排序、分页与乐观锁基线；能力清单中
逻辑删除、租户、数据权限、审计、Event Sourcing 和事务 Outbox 仍为 `false`，不能作为
稳定后端发布。RBatis 与 SeaORM 必须分别执行同一套 conformance suite，不能继承 SQLx 的证据。

## 权威入口

- [移植状态](./PORT_STATUS.md)
- [82 项机器清单](./port-manifest.toml)
- [架构文档](./ARCHITECTURE.md)
- [依赖等价表](./DEPENDENCY_EQUIVALENCE.md)
- [从 ddd4j 迁移](./MIGRATION_FROM_DDD4J.md)
- [RBatis 扩展边界](./RBATIS_EXTENSION_GUIDE.md)
- [兼容矩阵](./COMPATIBILITY_MATRIX.md)
- [安全模型](./SECURITY_MODEL.md)

`1.0.0` 只有在 82 项全部有实现、测试和兼容性证据，并通过发布门禁后才会发布。
