# 兼容性矩阵

| ddd4r | Rust | Tokio | ddd4j 基线 | RBatis | SQLx | SeaORM | 状态 |
|---|---|---|---|---|---|---|---|
| 0.1.0-alpha.1 | 1.97.1 | 1.x | `ddd4r-port-baseline-2026-07-21` | 4.9.6 / SQLite 纵切 | 0.9.0 / SQLite 纵切 | 2.0.0 / SQLite 纵切 | 开发中 |

外部扩展未发布前使用精确 Git revision。任何浮动 branch、未审计 fork 或跨组织源码复制都不进入发布分支。

| RBatis 扩展 | 版本 | 固定 revision | 状态 |
|---|---|---|---|
| rbatis-cache-core | 0.1.0-alpha.1 | `8028de4ceee63d98cf7f5ef4695ba59d0c4656e6` | SPI 与契约测试通过 |
| rbatis-caffeine | 0.1.0-alpha.1 | `34921f2c79c10bf1237fce767bda9af18368bb5f` | Moka 后端测试通过 |
| rbatis-redis | 0.1.0-alpha.1 | `761e4d1b77b651b15eb50556333b8fdf1c84fe2d` | Standalone Redis 8.4 集成测试通过；Cluster/Sentinel 拓扑待验证 |
