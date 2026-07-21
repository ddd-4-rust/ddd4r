# ADR-009：多 Web 框架映射

**状态**：Accepted　**日期**：2026-07-21

## 背景

ddd4j 提供多运行时适配，单一 Rust Web 框架无法验证框架无关边界。

## 决策

保留兼容模块名并映射到 Axum、Actix Web、Salvo、Poem、Hyper、Rocket 等实现，共享 conformance suite。

## 备选方案

只支持 Axum 简单但不满足模块对应；模拟 Java 容器会产生错误抽象。

## 影响

每个包只处理传输和生命周期，领域/应用语义必须一致。
