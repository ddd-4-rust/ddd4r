# RBatis 扩展边界

RBatis 相关项目归属 `rbatis-plus` 组织，ddd4r 只依赖其公开 crate：

- `rbatis-cache-core`
- `rbatis-caffeine`
- `rbatis-redis`
- `rbatis-memcached`
- `rbatis-r2dbc`
- `rbatis-typehandlers-jsr310`
- `rbatis-plus`

RBatis 本体缺少事务提交回调或逐行流式读取 Hook 时，在 `rbatis-plus/rbatis` 维护最小补丁并同步提交上游。
扩展合并前固定 commit revision；上游发布后切回官方 crate。

缓存一致性、拦截器顺序、时间类型映射和 rbatis-plus 能力细节以 ADR 和各独立扩展仓库的契约测试为准。

## 当前实现基线

| 仓库 | 固定 revision | 已验证能力 | 状态 |
|---|---|---|---|
| [`rbatis-cache-core`](https://github.com/rbatis-plus/rbatis-cache-core) | `8028de4ceee63d98cf7f5ef4695ba59d0c4656e6` | SQL AST 表标签、BLAKE3 Key、MessagePack、事务绕过、generation、singleflight、fail-open、metrics | alpha 已推送 |
| [`rbatis-caffeine`](https://github.com/rbatis-plus/rbatis-caffeine) | `34921f2c79c10bf1237fce767bda9af18368bb5f` | Moka async、TTL、TTI、字节权重、TinyLFU、本地 generation、metrics | alpha 已推送 |
| [`rbatis-redis`](https://github.com/rbatis-plus/rbatis-redis) | `761e4d1b77b651b15eb50556333b8fdf1c84fe2d` | Standalone 真实 Redis 8.4 契约；Cluster/Sentinel 连接、PSETEX、INCR、Pub/Sub、超时、熔断、metrics | alpha 已推送；Cluster/Sentinel 拓扑测试待补 |

`rbatis-caffeine` 通过 Git `rev` 精确依赖 `rbatis-cache-core`，没有依赖浮动分支。
Memcached、R2DBC、时间类型和 rbatis-plus 本体仍是后续工作；Redis Cluster/Sentinel 也尚未取得
真实拓扑证据，不能从 Standalone 测试推断完成。
