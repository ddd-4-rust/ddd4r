# 兼容性矩阵

| ddd4r | Rust | Tokio | ddd4j 基线 | RBatis | SQLx | SeaORM | 状态 |
|---|---|---|---|---|---|---|---|
| 0.1.0-alpha.1 | 1.97.1 | 1.x | `ddd4r-port-baseline-2026-07-21` | 4.9.6 + `bc904e9a` 流式 Hook / SQLite 纵切 | 0.9.0 / SQLite 纵切 | 2.0.0 / SQLite 纵切 | 开发中 |

外部扩展未发布前使用精确 Git revision。任何浮动 branch、未审计 fork 或跨组织源码复制都不进入发布分支。

供应链临时例外：`RUSTSEC-2025-0134` 来自 RBatis/RBDC 的 `rustls-pemfile 2.2.0`，该 advisory
为停止维护且没有安全升级版本。例外必须在 `1.0.0` 前或 2026-09-30 前移除（取较早者），
不能无期限延长；替换方案是推动 RBDC 直接使用 `rustls-pki-types::PemObject`。

`RUSTSEC-2026-0002` 不设例外：ddd4r 通过 `[patch.crates-io]` 固定
`rbatis-plus/rbdc@10696c3bd5da7390275e4724e9d22c0da83c45d3`，将 `lru` 提升到首个安全版本
`0.16.3` 以上；补丁已提交 [rbatis/rbdc#13](https://github.com/rbatis/rbdc/pull/13)，上游发布后移除 fork。

| RBatis 扩展 | 版本 | 固定 revision | 状态 |
|---|---|---|---|
| rbatis-cache-core | 0.1.0-alpha.1 | `8028de4ceee63d98cf7f5ef4695ba59d0c4656e6` | SPI 与契约测试通过 |
| rbatis-caffeine | 0.1.0-alpha.1 | `34921f2c79c10bf1237fce767bda9af18368bb5f` | Moka 后端测试通过 |
| rbatis-redis | 0.1.0-alpha.1 | `761e4d1b77b651b15eb50556333b8fdf1c84fe2d` | Standalone Redis 8.4 集成测试通过；Cluster/Sentinel 拓扑待验证 |
| rbatis-memcached | 0.1.0-alpha.1 | `d2549b050c7271d0136f4d5d892faa114ffcfcd3` | 真实 Memcached 1.6 契约通过；一致性哈希、CAS、TTL、generation、健康检查 |
| rbatis-r2dbc | 0.1.0-alpha.1 | `c68d7db6b9fe78a01705635ac900fc71abe67f23` | 原生行流、强类型解码、有界预取、取消和事务契约通过；依赖 RBatis fork `bc904e9a` |
| rbatis-typehandlers-jsr310 | 0.1.0-alpha.1 | `ac285d0b858738776362ff60368ba39638d08926` | 全 feature 时间语义与互操作测试通过；ddd4r 已固定依赖 |
| rbatis-plus | 0.1.0-alpha.1 | `b4ceb07ec8d64acaf151ef36621b324083e74a29` | 事务化批量更新/混合 Upsert 与整体回滚通过；fail-closed 构建器固定关键阶段；SM4/HMAC-SM3 标准向量、篡改拒绝与轮换通过 |

| 临时基础依赖 fork | 固定 revision | 上游状态 | 移除条件 |
|---|---|---|---|
| rbatis-plus/rbdc | `10696c3bd5da7390275e4724e9d22c0da83c45d3` | [rbatis/rbdc#13](https://github.com/rbatis/rbdc/pull/13) | 官方 RBDC 发布包含 `lru >= 0.16.3` 的版本 |
