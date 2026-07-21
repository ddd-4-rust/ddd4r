# ddd4j → ddd4r 移植状态

权威机器清单为 [`port-manifest.toml`](./port-manifest.toml)，由
[`tools/generate_port_manifest.py`](./tools/generate_port_manifest.py) 从 ddd4j Reactor 生成。

当前阶段：`0.1.0-alpha.1`。

| 能力 | 状态 | 当前证据 |
|---|---|---|
| 82 项 Reactor 清单 | 已建立 | manifest 固定校验数量为 82 |
| Rust workspace/toolchain | 已建立 | Rust 1.97.1、Edition 2024 |
| DomainModel/Entity/ValueObject | 进行中 | 核心 traits 和 derive macros |
| AggregateRoot Active Record 门面 | 进行中 | save/update/delete/query 契约测试 |
| Context/Repository Registry | 进行中 | task-local 优先和全局兜底测试 |
| Query AST/CQRS | 进行中 | Query rich methods、CommandBus |
| Event Envelope/Publisher | 进行中 | UUIDv7 信封和 task-local publisher |
| UnitOfWork/Outbox | 进行中 | SPI 和内存 Outbox 状态机 |
| 应用级 Cache | 进行中 | 内存 Cache、TTL、CAS、Stats |
| RBatis/SQLx/SeaORM | 计划中 | 尚未计入完成 |
| RBatis 扩展族 | 计划中 | 归属 rbatis-plus 组织 |
| Web/Auth/MQ/Runtime | 计划中 | 逐模块移植 |

任何没有实现和测试证据的包都不能标记为 `complete`，也不能计入 `1.0.0` 发布门禁。
