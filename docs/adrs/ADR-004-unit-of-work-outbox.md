# ADR-004：Unit of Work 与 Transactional Outbox

**状态**：Accepted　**日期**：2026-07-21

## 背景

仅缓冲领域事件不能证明聚合提交与消息发布的一致性。

## 决策

Repository 通过 Unit of Work 在同一数据库事务保存聚合和 Outbox；提交后按 at-least-once 投递。

## 备选方案

保存后直接发布会丢消息；分布式事务扩大基础设施耦合；Transactional Outbox 被采纳。

## 影响

消费者必须幂等；无法共享事务的适配器必须显式报告能力限制。
