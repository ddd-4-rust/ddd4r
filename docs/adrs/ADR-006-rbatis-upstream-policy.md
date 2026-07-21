# ADR-006：RBatis Fork 与外部写操作边界

**状态**：Accepted　**日期**：2026-07-21

## 背景

缓存提交回调和流式读取可能需要 RBatis/RBDC 底层 Hook。

## 决策

在 `rbatis-plus/rbatis` 维护最小补丁并固定 commit。未经项目所有者针对某一次操作的明确授权，
不得向 RBatis、RBDC 或其他外部开源项目提交 PR、Issue、评论或执行任何写操作。只允许读取公开
源码、文档、Release 和现有讨论。官方版本公开提供等价能力后，经兼容矩阵验证再切回官方版本。

## 备选方案

长期私有分叉和复制 RBatis 源码均被拒绝；完全等待上游会阻塞扩展验证。

## 影响

所有补丁必须有独立契约测试、组织内 commit revision 和兼容矩阵证据，不要求也不允许自动创建
外部上游链接或讨论。

## 实施证据

首个补丁固定为组织内 commit `bc904e9aa78ab44d5d4ff03b13767262c0caffe8`，将 RBDC
`exec_rows` 提升为 RBatis 有界 `QueryStream`，覆盖逐行读取、取消后连接释放和不兼容拦截器
拒绝测试。此前误提交的外部 PR 已关闭，不作为实施证据。
