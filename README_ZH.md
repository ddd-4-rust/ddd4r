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
- Auth 纵切提供对象安全异步 `Subject`/`SessionStore` SPI、UUIDv7 opaque token、Tokio
  task-local 会话上下文、权限/角色检查、Bearer 认证、会话轮换、超时、禁用和并发登录策略；
  Sa-Token、Security、Shiro 三个迁移 Provider 均执行同一份 conformance suite；临时令牌、
  scoped API Key、mixed login、StpKit、AuthUserDetails、异常映射、Realm 和 SessionDAO bridge
  均有独立契约测试。
- 三后端共享数据契约，以及分别通过 SQLite 实际执行验证的 SQLx 0.9、RBatis 4.9.6
  和 SeaORM 2.0 纵切。
- 三个适配器都独立通过 CRUD、批量、条件、排序、分页、乐观锁和显式事务测试。
- `TransactionalEventRepository` 保证聚合与事件信封同库原子提交；重复事件导致写入失败时，
  聚合更新回滚、事件缓冲保留，三后端执行同一份原子性测试。
- `rbatis-memcached` 提供 generation、CAS、TTL、一致性哈希和健康检查，并通过真实
  Memcached 1.6 容器契约。
- `rbatis-r2dbc` 将 RBDC 原生行流提升为强类型、有界预取、取消可传播的 `Stream`，并提供
  commit/rollback/Drop 回滚事务作用域；RBatis Hook 已提交上游 PR #623。
- `rbatis-plus` 已建立 core/macros/extension/codegen/facade 五 crate 纵切，原生 Mapper 已执行
  CRUD、事务化批量插入/更新/混合 Upsert、分页、强类型 Query/Update Wrapper、乐观锁与逻辑删除；
  ddd4r 适配包同时通过该 Mapper 的真实 SQLite 回滚验收和共享 Repository/事务 Outbox 契约。
- `rbatis-plus` 安全管线实现 AES-256-GCM 随机 nonce 字段加密、上下文 blind index、HMAC 行签名、
  密钥轮换、`REJECT_PARTIAL`/`DEFERRED_RESIGN` 和审计填充；`RbatisMapper` 已在真实 SQLite
  执行中接通写前变换、SQL 重写、先验签后解密与最终观测；`SecurePipelineBuilder` 固定关键阶段、
  按 statement ID 限定参数加密，并拒绝缺失签名列或解密策略的 fail-open 装配。
- 国密 Provider 使用 SM4-CBC/PKCS#7、随机 128-bit IV 和 HMAC-SM3 Encrypt-then-MAC，支持
  上下文绑定、独立 blind-index key 与在线密钥轮换，并通过 SM3/SM4 标准已知答案测试。

三套后端当前都只完成 SQLite 纵切；事务 Outbox 原子写入已为 `Supported`，但 claim、重试
和死信调度仍由后续持久化 `OutboxStore` 补齐。三套 Repository 统一层面的逻辑删除、租户、
数据权限、审计和 Event Sourcing 仍为 `Planned`。PostgreSQL、MySQL、SQL Server 及这些高级能力全部通过前，任何
适配器都不能作为稳定后端发布。

Auth 三个迁移入口不运行对应 Java 框架；它们保留 ddd4j 的 Subject 调用语义并共享 Rust 原生
安全状态机。Java runtime annotation 映射为显式 Rust policy value，Web 响应构造留给后续 Web
adapter；这两项属于已审计的语义适配，不是缺失实现。

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

每个 `v*` tag 都必须经过完整测试、文档、许可证和 advisory 门禁，并产出源码归档、SHA-256、
覆盖全部 84 个 Cargo package 的 CycloneDX 1.5 SBOM，以及 GitHub Sigstore 构建来源和 SBOM
attestation。供应链策略见 [`SECURITY_MODEL.md`](./SECURITY_MODEL.md)，依赖例外和固定 revision
见 [`COMPATIBILITY_MATRIX.md`](./COMPATIBILITY_MATRIX.md)。
