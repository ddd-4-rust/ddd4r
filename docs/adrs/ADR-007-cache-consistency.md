# ADR-007：二级缓存一致性协议

**状态**：Accepted　**日期**：2026-07-21

## 背景

Redis、Memcached 和本地缓存的失效能力不同，事务读写还会产生脏缓存。

## 决策

事务读绕过缓存；提交后使用 namespace generation 失效；后端故障 fail-open；缓存加密态结果。

## 备选方案

通配删除不可移植；仅 TTL 一致性过弱；强一致分布式锁成本过高。

## 影响

所有缓存后端实现统一 key/envelope/generation 契约，允许 Redis Pub/Sub 作为优化而非正确性前提。
