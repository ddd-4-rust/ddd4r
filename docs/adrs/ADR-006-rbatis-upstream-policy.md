# ADR-006：RBatis Fork 与上游协作

**状态**：Accepted　**日期**：2026-07-21

## 背景

缓存提交回调和流式读取可能需要 RBatis/RBDC 底层 Hook。

## 决策

在 `rbatis-plus/rbatis` 维护最小、可上游化补丁；合并前固定 commit，合并后回到官方版本。

## 备选方案

长期私有分叉和复制 RBatis 源码均被拒绝；完全等待上游会阻塞扩展验证。

## 影响

所有补丁必须有独立契约测试和上游 PR 链接。

## 实施证据

首个补丁固定为 `bc904e9aa78ab44d5d4ff03b13767262c0caffe8`，将 RBDC `exec_rows`
提升为 RBatis 有界 `QueryStream`，覆盖逐行读取、取消后连接释放和不兼容拦截器拒绝测试；
对应 [rbatis/rbatis#623](https://github.com/rbatis/rbatis/pull/623)。
