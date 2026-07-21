# ddd4j 与 Rust 依赖对照

机器可读决策记录在 [`dependency-equivalence.toml`](./dependency-equivalence.toml)。首批锁定对照如下：

| Java 生态 | Rust 对应 | 决策 |
|---|---|---|
| CompletableFuture / Reactor | Tokio、futures | Tokio 是唯一受支持运行时 |
| Jackson | serde、serde_json、rmp-serde | JSON 用于 Fixtures，MessagePack 用于缓存负载 |
| Lombok | ddd4r-annotation、lombok-macros | 领域宏自维护；lombok-macros 仅辅助 DTO 迁移 |
| MyBatis | RBatis、RBDC、RBS | `ddd4r-data-rbatis` 正式后端 |
| MyBatis-Plus | rbatis-plus | 独立归属 `rbatis-plus` 组织 |
| JPA | SeaORM | `ddd4r-data-seaorm` 正式后端 |
| JDBC / 显式 SQL | SQLx | 新增 `ddd4r-data-sqlx` 正式后端 |
| Caffeine | Moka | 保留 `rbatis-caffeine` 兼容名称 |
| Redis | redis-rs | generation token + 可选 Pub/Sub |
| Memcached | memcache | generation key + CAS，不扫描 key |
| R2DBC | RBatis + RBDC `exec_rows` + futures `Stream` | 保留 `rbatis-r2dbc` 迁移名，不重复实现数据库协议 |
| ArchUnit | cargo_metadata、guppy、syn | Cargo 图与源码 AST 联合检查 |
| SLF4J / Micrometer / OTel | tracing、metrics、opentelemetry | tracing 为统一观测门面 |
| Spring MVC / WebFlux / Javalin | Actix Web、Axum | 共用 Web conformance suite |
| Quarkus / Micronaut / Vert.x | Salvo、Poem、Tokio Event Bus | 兼容包是迁移入口，不模拟 JVM 容器 |
| Helidon / Dropwizard | Hyper、Rocket | 轻量运行时适配 |

未确认的依赖不得仅凭名称映射；必须记录 API、行为差异、许可证和维护状态。
