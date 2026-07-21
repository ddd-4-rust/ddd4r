# 兼容性矩阵

| ddd4r | Rust | Tokio | ddd4j 基线 | RBatis | SQLx | SeaORM | 状态 |
|---|---|---|---|---|---|---|---|
| 0.1.0-alpha.1 | 1.97.1 | 1.x | `ddd4r-port-baseline-2026-07-21` | 4.9.6 + `bc904e9a` 流式 Hook / SQLite 纵切 | 0.9.0 / SQLite 纵切 | 2.0.0 / SQLite 纵切 | 开发中 |

外部扩展未发布前使用精确 Git revision。任何浮动 branch、未审计 fork 或跨组织源码复制都不进入发布分支。

| RBatis 扩展 | 版本 | 固定 revision | 状态 |
|---|---|---|---|
| rbatis-cache-core | 0.1.0-alpha.1 | `8028de4ceee63d98cf7f5ef4695ba59d0c4656e6` | SPI 与契约测试通过 |
| rbatis-caffeine | 0.1.0-alpha.1 | `34921f2c79c10bf1237fce767bda9af18368bb5f` | Moka 后端测试通过 |
| rbatis-redis | 0.1.0-alpha.1 | `761e4d1b77b651b15eb50556333b8fdf1c84fe2d` | Standalone Redis 8.4 集成测试通过；Cluster/Sentinel 拓扑待验证 |
| rbatis-memcached | 0.1.0-alpha.1 | `d2549b050c7271d0136f4d5d892faa114ffcfcd3` | 真实 Memcached 1.6 契约通过；一致性哈希、CAS、TTL、generation、健康检查 |
| rbatis-r2dbc | 0.1.0-alpha.1 | `c68d7db6b9fe78a01705635ac900fc71abe67f23` | 原生行流、强类型解码、有界预取、取消和事务契约通过；依赖 RBatis fork `bc904e9a` |
| rbatis-typehandlers-jsr310 | 0.1.0-alpha.1 | `ac285d0b858738776362ff60368ba39638d08926` | 全 feature 时间语义与互操作测试通过；ddd4r 已固定依赖 |
| rbatis-plus | 0.1.0-alpha.1 | `03cf088ec6bc324888e30c3238ef391f29cd240e` | 原生 Mapper 事务化批量更新/混合 Upsert 与整体回滚通过；已接入 SQL 重写/参数变换/验签/解密/观测阶段；SM4/SM3 与默认安全装配尚未完成 |
