# RBatis 扩展边界

RBatis 相关项目归属 `rbatis-plus` 组织，ddd4r 只依赖其公开 crate：

- `rbatis-cache-core`
- `rbatis-caffeine`
- `rbatis-redis`
- `rbatis-memcached`
- `rbatis-r2dbc`
- `rbatis-typehandlers-jsr310`
- `rbatis-plus`

RBatis 本体缺少事务提交回调或逐行流式读取 Hook 时，只在 `rbatis-plus/rbatis` 维护最小补丁并固定
commit revision。未经项目所有者逐次明确授权，不得向 RBatis、RBDC 或其他外部开源项目提交 PR、
Issue、评论或任何写操作；是否切回官方 crate 只依据公开发布版本的兼容性验证结果。

缓存一致性、拦截器顺序、时间类型映射和 rbatis-plus 能力细节以 ADR 和各独立扩展仓库的契约测试为准。

## 当前实现基线

| 仓库 | 固定 revision | 已验证能力 | 状态 |
|---|---|---|---|
| [`rbatis`](https://github.com/rbatis-plus/rbatis) | `bc904e9aa78ab44d5d4ff03b13767262c0caffe8` | RBDC 原生逐行流、非零有界预取、取消传播、连接释放、拦截器 fail-closed | 仅维护组织内 fork；禁止未授权的外部写操作 |
| [`rbatis-cache-core`](https://github.com/rbatis-plus/rbatis-cache-core) | `8028de4ceee63d98cf7f5ef4695ba59d0c4656e6` | SQL AST 表标签、BLAKE3 Key、MessagePack、事务绕过、generation、singleflight、fail-open、metrics | alpha 已推送 |
| [`rbatis-caffeine`](https://github.com/rbatis-plus/rbatis-caffeine) | `34921f2c79c10bf1237fce767bda9af18368bb5f` | Moka async、TTL、TTI、字节权重、TinyLFU、本地 generation、metrics | alpha 已推送 |
| [`rbatis-redis`](https://github.com/rbatis-plus/rbatis-redis) | `761e4d1b77b651b15eb50556333b8fdf1c84fe2d` | Standalone 真实 Redis 8.4 契约；Cluster/Sentinel 连接、PSETEX、INCR、Pub/Sub、超时、熔断、metrics | alpha 已推送；Cluster/Sentinel 拓扑测试待补 |
| [`rbatis-memcached`](https://github.com/rbatis-plus/rbatis-memcached) | `d2549b050c7271d0136f4d5d892faa114ffcfcd3` | 一致性哈希、generation、CAS、TTL、大对象限制、超时、健康与指标；真实 Memcached 1.6 容器契约 | alpha 已推送并由 ddd4r 可选 feature 固定依赖 |
| [`rbatis-r2dbc`](https://github.com/rbatis-plus/rbatis-r2dbc) | `c68d7db6b9fe78a01705635ac900fc71abe67f23` | 强类型 `Stream`、有界预取、取消、Future execute、commit/rollback/Drop 回滚事务作用域 | alpha 已推送并由 ddd4r 可选 feature 固定依赖 |
| [`rbatis-typehandlers-jsr310`](https://github.com/rbatis-plus/rbatis-typehandlers-jsr310) | `ac285d0b858738776362ff60368ba39638d08926` | time 主模型、Year/YearMonth/MonthDay/Period/ZonedDateTime、UTC 微秒精度、DST fail-closed、chrono/jiff/fastdate | alpha 已推送并由 ddd4r 固定依赖 |
| [`rbatis-plus`](https://github.com/rbatis-plus/rbatis-plus) | `b4ceb07ec8d64acaf151ef36621b324083e74a29` | 原生 Mapper CRUD、事务化批量插入/更新/混合 Upsert、分页、Wrapper、乐观锁、逻辑删除；fail-closed `SecurePipelineBuilder`；AES-256-GCM、认证型 SM4/HMAC-SM3、blind index、行签名、轮换、部分行策略、审计填充 | SQLite Upsert/整体回滚和安全端到端管线通过；四数据库矩阵待补 |

所有跨仓库依赖都使用 Git `rev`，没有依赖浮动分支。`ddd4r-data-rbatis` 通过
`memcached-cache`、`r2dbc` feature 暴露两个兼容入口；全 feature 门禁会编译并测试固定 revision。

`rbatis-plus` 当前完成原生 Mapper 与安全管线的 alpha 纵切，尚未达到 MyBatis-Plus/Enhance 完整语义；Redis
Cluster/Sentinel 也尚未取得真实拓扑证据，不能从 Standalone 测试推断完成。官方版本若公开提供
等价 Hook，需要先在同一兼容矩阵中验证，再决定是否切回；该过程不包含外部 PR 或 Issue 操作。

ddd4j 冻结基线的 `DefaultEncryptedFieldHandler` 将 mode、padding、key 和 IV 作为 `null`
传给策略，而其缓存键和 Base64 路径不能稳定执行。因此 `gm1` 信封保留 SM4/HMAC-SM3
算法意图，但不宣称与该失效默认路径逐字节兼容；迁移时必须显式配置密钥并重新加密数据。
