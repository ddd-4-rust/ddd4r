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
