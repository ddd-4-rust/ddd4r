# ADR-003：Task-local 与全局 Registry

**状态**：Accepted　**日期**：2026-07-21

## 背景

ddd4j 使用 ThreadContext→BaseContext；OS 线程局部变量不适用于异步任务迁移。

## 决策

使用 Tokio task-local Registry 优先、全局 Registry 兜底；相同 key/type 重复注册失败。

## 备选方案

只使用全局单例会破坏租户覆盖；使用线程局部变量会跨 await 错配；强制所有调用传递 Context 会失去兼容门面。

## 影响

保留 `save/query/publish` 语义，同时要求入口适配器显式创建 `ContextScope`。
