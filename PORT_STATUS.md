# ddd4j → ddd4r 移植状态

权威机器清单为 [`port-manifest.toml`](./port-manifest.toml)，由
[`tools/generate_port_manifest.py`](./tools/generate_port_manifest.py) 从 ddd4j Reactor 生成。

完整分阶段实施计划（含 `crates/` monorepo 收敛、鉴权深化与 1.0.0 门禁）见
[`docs/IMPLEMENTATION_PLAN.md`](./docs/IMPLEMENTATION_PLAN.md)。

当前版本：`0.1.0-alpha.1`。**整体未完成**（manifest：`complete` 4 / `in_progress` 15 / `scaffolded` 63，
artifact 级严格完成率 **4.88%**）。
Phase 0–6 按实施计划推进；
状态只按当前仓库中的可执行代码和测试证据计算。
当前 `Cargo.toml` 与 `Cargo.lock` 不一致，workspace 的 `--locked` 发布级测试会在解析阶段失败；
这项可复现性缺口必须在 Phase 0 首先闭合。

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
| 应用级 Cache | 进行中 | 对象安全异步 SPI、CacheKit 注册门面、TTL/TTI/persist、容量淘汰、值与版本 CAS、整数/浮点计数、库存返回码、singleflight、typed JSON、owner lease 锁与并发契约均通过；Redis/Redisson/Memcached/多级缓存适配待补 |
| 数据后端共享契约 | 进行中 | 三后端独立执行统一 CRUD/query/page/optimistic-lock 与 aggregate/outbox 原子性 conformance suite |
| SQLx | 进行中 | SQLx 0.9 + SQLite 仓储、显式事务、事务 Outbox 原子提交/回滚已通过；其余数据库和高级能力未完成 |
| RBatis | 进行中 | RBatis 4.9.6 + 组织内固定 fork `bc904e9a`；RBDC SQLite 仓储、显式事务、事务 Outbox 与原生有界行流已通过；禁止未授权的外部 PR/Issue/评论 |
| SeaORM | 进行中 | SeaORM 2.0 + SQLite 仓储、显式事务、事务 Outbox 原子提交/回滚已通过；高级能力未完成 |
| RBatis 扩展族 | 进行中 | 独立仓库均固定 revision；rbatis-plus 原生 Mapper 已完成事务化批量更新/混合 Upsert、fail-closed 安全装配及认证型 SM4/HMAC-SM3 Provider；SQLite 回滚、安全管线和 Outbox 契约通过；四数据库及 Redis Cluster/Sentinel 拓扑仍未完成 |
| Auth 核心 | artifact 账本 complete，生态未闭环 | 对象安全异步 `Subject`、可替换 `SessionStore`、Tokio task-local `SubjectScope`、RBAC、Bearer、禁用/超时/轮换和并发登录策略已有测试；Subject 全重载、remember-me、Redis/Web 接线仍待计划 Phase 3–4 |
| Sa-Token/Security/Shiro 迁移入口 | artifact 账本 complete | 4/4 Auth 映射标记 `complete`；共享 conformance、临时令牌、API Key、mixed login、StpKit、AuthUserDetails、异常映射、Realm 与 SessionDAO 契约已有证据，Phase 0 仍需方法级复审 |
| 供应链与发布证据 | 已建立 | `cargo-deny 0.19.4`、`cargo-audit 0.22.2`；84 包 CycloneDX 1.5 聚合 SBOM及官方 CLI schema 校验；tag 构建生成 SHA-256、SLSA provenance 与 SBOM attestation |
| MQ Core/Disruptor | 进行中 | UUIDv7 信封、标准 headers、对象安全 ack/publisher/handler/store、路由与 SQL-92 selector、幂等 claim、at-least-once 重试/DLQ，以及 Tokio 有界本地 Broker 端到端契约通过；其余 Broker 适配器仍为 scaffolded |
| Web/Runtime | 计划中 | 逐模块移植 |

任何没有实现和测试证据的包都不能标记为 `complete`，也不能计入 `1.0.0` 发布门禁。
