# ddd4r 架构

本文面向框架使用者、适配器维护者和发布工程师，记录 ddd4r 从系统边界到核心代码的 C4 架构。
架构变更必须在同一 PR 更新本文件和对应 ADR。

## L1：System Context

```mermaid
flowchart LR
    DEV["Rust 业务开发者"]
    JAVA["ddd4j 基线源码与 Fixtures"]
    DDD4R["ddd4r<br/>DDD/CQRS 开发脚手架"]
    RB["rbatis-plus 生态"]
    DB["PostgreSQL / MySQL / SQLite / SQL Server"]
    BROKER["Kafka / RabbitMQ / NATS / MQTT / 其他 Broker"]
    OBS["OpenTelemetry / Metrics 后端"]

    DEV -->|"构建领域应用"| DDD4R
    JAVA -->|"行为基线"| DDD4R
    DDD4R -->|"可选数据增强"| RB
    DDD4R --> DB
    DDD4R --> BROKER
    DDD4R --> OBS
```

## L2：Container

```mermaid
flowchart TB
    APP["业务应用"]

    subgraph WORKSPACE["ddd-4-rust/ddd4r"]
        FACADE["ddd4r facade"]
        CORE["ddd4r-core<br/>DDD / CQRS / Context / Repository"]
        MACRO["ddd4r-annotation<br/>proc macros"]
        CACHE["ddd4r-cache<br/>应用级 Cache / CAS"]
        OUTBOX["ddd4r-outbox<br/>交付状态机"]
        DATA["ddd4r-data-*<br/>RBatis / SQLx / SeaORM"]
        WEB["ddd4r-web-* / runtime-*"]
        MQ["ddd4r-mq-*"]
    end

    APP --> FACADE
    FACADE --> CORE
    FACADE --> MACRO
    DATA --> CORE
    CACHE --> CORE
    OUTBOX --> CORE
    WEB --> CORE
    MQ --> CORE
    DATA --> EXTDB["外部数据库"]
    MQ --> EXTMQ["外部 Broker"]
```

## L3：Core Components

```mermaid
flowchart TB
    subgraph DOMAIN["Domain"]
        MODEL["DomainModel / Entity / ValueObject"]
        AGG["AggregateRoot / Event Buffer"]
        EVENT["DomainEvent / EventEnvelope"]
    end

    subgraph APPLICATION["Application"]
        COMMAND["CommandBus / CommandExecutor"]
        QUERY["Query AST / Rich Query Methods"]
        PROJECTION["ProjectionRunner / ReadModel"]
    end

    subgraph PORTS["Ports"]
        REPO["Repository"]
        UOW["UnitOfWork"]
        OUTBOX["OutboxStore"]
        PUBLISHER["DomainEventPublisher"]
    end

    subgraph RUNTIME["Runtime"]
        CONTEXT["ContextScope"]
        TASK["Tokio task-local Registry"]
        GLOBAL["Global Registry"]
    end

    AGG --> MODEL
    AGG --> EVENT
    QUERY --> REPO
    COMMAND --> REPO
    REPO --> UOW
    UOW --> OUTBOX
    EVENT --> PUBLISHER
    CONTEXT --> TASK
    TASK -.->|"missing"| GLOBAL
    REPO --> CONTEXT
    PUBLISHER --> CONTEXT
```

## L4：核心类型关系

```mermaid
classDiagram
    class DomainModel {
        <<trait>>
        +Id
        +id() Id
    }
    class Entity {
        <<trait>>
    }
    class AggregateRoot {
        <<trait>>
        +version() u64
        +record_event(event)
        +pull_events() EventEnvelope[]
    }
    class AggregateRootExt {
        <<trait>>
        +save() Future
        +update() Future
        +delete() Future
        +query() Query
    }
    class Repository~A~ {
        <<trait>>
        +find_by_id(id)
        +save(aggregate)
        +find_list(query)
        +page(query)
    }
    class Registry {
        +register(key, service)
        +get(key) service
        +snapshot() Registry
    }
    class EventEnvelope {
        +event_id UUIDv7
        +aggregate_id String
        +aggregate_version u64
        +payload Value
    }

    DomainModel <|-- Entity
    Entity <|-- AggregateRoot
    AggregateRoot <|-- AggregateRootExt
    AggregateRootExt ..> Repository
    AggregateRoot --> EventEnvelope
    Repository ..> Registry
```

## 关键不变量

1. 领域核心不依赖 Web、数据库、消息组件或 DI 容器。
2. task-local 服务覆盖全局默认服务，作用域结束后不得泄漏。
3. 同一 Registry 中相同 key/type 重复注册必须失败。
4. 聚合与 Outbox 在正式数据适配器中必须原子提交。
5. Auth 失败关闭；缓存失败开放且不得改变数据库查询结果。
6. 所有 82 项迁移状态由根目录 `port-manifest.toml` 管理。
