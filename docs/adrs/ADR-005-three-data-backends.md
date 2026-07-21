# ADR-005：三套正式数据后端

**状态**：Accepted　**日期**：2026-07-21

## 背景

Java 用户需要 MyBatis/JPA 迁移路径，Rust 用户也需要原生显式 SQL。

## 决策

RBatis、SeaORM、SQLx 均实现完整 Repository、Query、事务和 Outbox 契约。

## 备选方案

仅 RBatis 无法覆盖 ORM/显式 SQL 偏好；只定义空 trait 不算支持；三后端完整契约被采纳。

## 影响

测试矩阵显著增加，但领域层不绑定任一 ORM。
