# ddd4r

`ddd4r` 是 [`ddd4j`](https://github.com/ddd-4-java/ddd4j) 的 Rust 语义移植，提供框架无关的
DDD、CQRS、事件溯源、Repository、Unit of Work 和 Outbox 基础能力。

当前仓库处于 `0.1.0-alpha` 移植阶段。稳定版 `1.0.0` 只有在
[`port-manifest.toml`](./port-manifest.toml) 中全部 82 个 Reactor 项通过行为验证后才会发布。

## 设计约束

- Rust 1.97.1、Edition 2024、Tokio。
- task-local 上下文优先，全局 Registry 兜底。
- 保留 `aggregate.save().await?`、`query.list().await?`、`event.publish().await?` 兼容门面。
- 内部使用显式 Repository、Unit of Work 和 Transactional Outbox。
- 自有核心代码禁止 `unsafe`。
- `MIT OR Apache-2.0` 双许可证。

架构与迁移状态见 [`docs/architecture/README.md`](./docs/architecture/README.md) 和
[`PORT_STATUS.md`](./PORT_STATUS.md)。
