# ddd4j → ddd4r 移植状态

权威机器清单为 [`port-manifest.toml`](./port-manifest.toml)，由
[`tools/generate_port_manifest.py`](./tools/generate_port_manifest.py) 从 ddd4j Reactor 生成。

当前版本：`0.1.0-alpha.1`。Phase 0 与 Phase 1 已完成，Phase 2 正在实现；
状态只按当前仓库中的可执行代码和测试证据计算。

| 能力 | 状态 | 当前证据 |
|---|---|---|
| ddd4j 冻结基线 | 已完成 | `ddd4r-port-baseline-2026-07-21` → `b8bc9547c60b5b49def44e81abf5ba520236fc44`；82/82 Reactor 测试成功 |
| 82 项 Reactor 清单 | 已完成 | 82 个唯一映射；1,297 个公开类型、4,591 个公开方法、550 个直接依赖、135 个 Java 测试 |
| Rust workspace/toolchain | 已完成 | 84 packages（82 映射 + 2 原生扩展）；Rust 1.97.1、Edition 2024、Tokio；GitHub `ddd-4-rust/ddd4r` |
| API 审计清单 | 已建立 | 31 个核心契约，区分直接类型、语义适配和 Rust 原生扩展；CI 强制校验 |
| DomainModel/Entity/ValueObject | 进行中 | 核心 traits 和 derive macros |
| AggregateRoot Active Record 门面 | 进行中 | save/update/delete/query/batch/fill 契约测试 |
| Context/Repository/Runtime Registry | 进行中 | task-local 优先、全局兜底、嵌套/取消/panic/并发隔离测试 |
| Query AST/CQRS | 进行中 | 强类型 Condition/Order/Page、CommandBus、ProjectionRunner |
| Event Envelope/Publisher | 进行中 | UUIDv7 信封和 task-local publisher |
| Event Sourcing/Mapper | 进行中 | 异步 Repository SPI、历史版本读取、映射契约和乐观版本测试 |
| UnitOfWork/Outbox | 进行中 | SPI、内存 Outbox claim/publish/retry/dead-letter 状态机 |
| DDD Rules/Clean/COLA | 进行中 | Cargo Metadata 建图、`syn` 源码引用核验、分层和框架依赖违规测试 |
| 应用级 Cache | 进行中 | 内存 Cache、TTL、CAS、Stats |
| 数据后端共享契约 | 进行中 | 三后端独立执行统一 CRUD/query/page/optimistic-lock 与 aggregate/outbox 原子性 conformance suite |
| SQLx | 进行中 | SQLx 0.9 + SQLite 仓储、显式事务、事务 Outbox 原子提交/回滚已通过；其余数据库和高级能力未完成 |
| RBatis | 进行中 | RBatis 4.9.6 + 固定 fork `bc904e9a`；RBDC SQLite 仓储、显式事务、事务 Outbox 与原生有界行流已通过；[上游 PR #623](https://github.com/rbatis/rbatis/pull/623) 待合并 |
| SeaORM | 进行中 | SeaORM 2.0 + SQLite 仓储、显式事务、事务 Outbox 原子提交/回滚已通过；高级能力未完成 |
| RBatis 扩展族 | 进行中 | 独立仓库均固定 revision；rbatis-plus 原生 Mapper 已完成事务化批量更新/混合 Upsert、fail-closed 安全装配及认证型 SM4/HMAC-SM3 Provider；SQLite 回滚、安全管线和 Outbox 契约通过；四数据库及 Redis Cluster/Sentinel 拓扑仍未完成 |
| 供应链与发布证据 | 已建立 | `cargo-deny 0.19.4`、`cargo-audit 0.22.2`；84 包 CycloneDX 1.5 聚合 SBOM及官方 CLI schema 校验；tag 构建生成 SHA-256、SLSA provenance 与 SBOM attestation |
| Web/Auth/MQ/Runtime | 计划中 | 逐模块移植 |

任何没有实现和测试证据的包都不能标记为 `complete`，也不能计入 `1.0.0` 发布门禁。
