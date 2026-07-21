# ADR-008：保留 Java 迁移兼容名称

**状态**：Accepted　**日期**：2026-07-21

## 背景

`r2dbc`、`jsr310`、`caffeine` 是 Java 概念，但能显著降低迁移认知成本。

## 决策

保留 `rbatis-r2dbc`、`rbatis-typehandlers-jsr310`、`rbatis-caffeine` 名称，并在文档中明确 Rust 实现语义。

## 备选方案

完全 Rust 化命名更纯粹但破坏对照；复制 Java 协议模型则没有技术价值。

## 影响

名称兼容、实现原生：R2DBC 映射 Future/Stream/RBDC，Caffeine 映射 Moka。
