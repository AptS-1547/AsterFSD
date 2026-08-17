# RFC-0002：Technology Stack and Infrastructure Profiles

| 字段 | 内容 |
| --- | --- |
| 状态 | Proposed |
| 日期 | 2026-08-17 |
| 负责产品组 | AsterFSD Platform |
| 影响范围 | Rust workspace、HTTP、gRPC、事件传输、数据库、可观测性、配置、部署和供应链 |
| 上位 RFC | [RFC-0001：AsterFSD Platform Architecture](0001-asterfsd-platform-architecture.md) |
| 相关 RFC | [RFC-0003：Identity and Trust Architecture](0003-identity-and-trust-architecture.md)、[RFC-0004：Network Runtime, Sharding and High Availability](0004-network-runtime-sharding-and-high-availability.md)、[RFC-0005：Event Model and Delivery Semantics](0005-event-model-and-delivery-semantics.md)、[RFC-0006：History, Replay and Telemetry Architecture](0006-history-replay-and-telemetry-architecture.md) |
| 当前实现基线 | Rust 2024、Tokio、Bytes、tokio-util、SeaORM/SQLx、tracing、AsterForge Logging |

## 1. 摘要

本 RFC 固定 AsterFSD Platform 的官方技术栈和基础设施 Profile。目标不是追逐每个生态热点，而是为以下两种同等正式的使用方式提供同一套架构：

- 几个人通过一个 binary、一个配置文件和 SQLite 快速启动飞行网络；
- 运营方通过 Axum、Tonic gRPC、NATS、PostgreSQL、History projection 和 Kubernetes 组装大型平台。

本 RFC 的核心决策如下：

1. Rust、Tokio、`tokio-util` 和 `Bytes` 继续作为 Network Runtime 基础，不替换运行时。
2. 对外 HTTP、WebSocket、管理和 Web Control Plane 使用 Axum/Tower；与 Tonic 共享 Tokio、Hyper、HTTP 和 Tower 生态，但保持独立 adapter 和默认 listener。
3. 内部同步服务通信使用 Tonic gRPC、Prost 和版本化 Protobuf；Tonic 与 Axum 共享 application service contract，不共享 transport DTO。
4. Standalone 使用进程内有界 transport；Distributed/Kubernetes 的官方事件 transport 使用 Core NATS + JetStream。
5. Core NATS 承载可合并的 realtime delta；JetStream 承载需要重放的 durable domain event。
6. 所有 durable consumer 仍按 at-least-once、幂等和 fencing 设计，不把 broker 的 exactly-once 功能当作跨服务事务。
7. SQLite 是 Standalone 数据库；PostgreSQL 是 Distributed/Kubernetes 的生产默认；MySQL 是可选兼容级；ClickHouse 和 S3/Parquet 属于大型 History Profile。
8. SeaORM 用于 repository 和控制面 CRUD；SQLx 可用于批量 History、数据库专属能力和高吞吐路径；领域 model 不依赖 ORM。
9. `tracing` 是代码埋点 API，`aster_forge_logging` 负责日志装配，Prometheus 负责指标，OpenTelemetry/OTLP 只作为 composition-root exporter。
10. Standalone 不强制运行 NATS、PostgreSQL、ClickHouse、OpenTelemetry Collector 或 Kubernetes。

## 2. 设计目标

### 2.1 必须满足

- Network packet 热路径不因 HTTP、数据库、broker 或 projection 故障而同步阻塞；
- local implementation 与 remote implementation 共享同一业务 contract；
- 技术栈与 RFC-0001 的服务和数据 ownership 一致；
- 每个外部系统都有明确的失败、背压、重试、幂等、恢复和观测语义；
- 默认依赖和 feature 不把所有部署能力编进每个 binary；
- 官方技术栈有活跃维护、稳定许可证、Rust/MSRV 兼容和真实运维路径；
- AsterDrive/AsterForge 的 Actix 经验作为安全、错误、限流和测试语义参考，不把已有框架实现直接搬入 AsterFSD；
- Kubernetes Profile 可以验证真实 artifact、Pod、Service、EndpointSlice、route、health 和恢复；
- 运营方可以替换 EventTransport、IdentityService、HistoryStore 和 projection，但不能改变 core 语义。

### 2.2 不追求

- 用一个框架承载 TCP、HTTP、gRPC、消息和数据库的全部抽象；
- 为每个 crate 创建独立网络服务；
- 让 Standalone 拥有与大型平台完全相同的进程和基础设施数量；
- 在 Network Runtime 中暴露 ORM entity、Protobuf DTO 或 broker message；
- 通过共享数据库、Redis 锁或分布式事务掩盖 ownership；
- 仅按 GitHub star、benchmark 榜单或单次发布速度选型；
- 把本 RFC 固定的语义误解为永不升级的依赖版本。

## 3. 技术栈总览

```text
Client protocols
  -> Tokio raw TCP
  -> tokio-util bounded codec
  -> Bytes/WireFrame
  -> AsterFSD Network Runtime

Public/control API
  -> Axum / Tower
  -> REST / WebSocket / SSE

Internal command/query
  -> Tonic gRPC
  -> Prost / Protobuf

Realtime event
  -> local bounded transport (Standalone)
  -> Core NATS (Distributed/Kubernetes)

Durable domain event
  -> local outbox/spool (Standalone)
  -> NATS JetStream (Distributed/Kubernetes)

Operational data
  -> SQLite (Standalone)
  -> PostgreSQL (Distributed/Kubernetes)

History and analytics
  -> PostgreSQL partitioned store (compact profile)
  -> ClickHouse (large profile)
  -> S3/Parquet (archive)

Observability
  -> tracing
  -> aster_forge_logging
  -> Prometheus
  -> optional OTLP exporter
```

## 4. Rust、Tokio 和热路径基础

### 4.1 Rust 和 MSRV

- workspace 使用 Rust 2024 edition；
- 根 `Cargo.toml` 是 MSRV 的唯一事实源；
- 所有 member 继承 edition、MSRV、package metadata 和 workspace lints；
- 引入依赖前检查其 MSRV、许可证、unsafe surface、feature graph 和维护状态；
- 不为单个服务随意提高 MSRV；需要提升时一次更新 workspace、CI、镜像、开发文档和 release note。

### 4.2 Tokio

Tokio 是唯一官方异步 runtime。不得在同一进程引入另一套通用 async runtime。

职责：

- TCP listener 和连接任务；
- timer、idle timeout、signal 和 graceful shutdown；
- bounded channel、task supervision 和 cancellation；
- Tonic、SQLx/SeaORM 和 async-nats 的 runtime 基础。

当前 workspace 使用 `features = ["full"]`。最终治理要求按真实调用收缩 feature：

```toml
tokio = {
    version = "...",
    features = [
        "macros",
        "rt-multi-thread",
        "net",
        "io-util",
        "sync",
        "signal",
        "time",
    ],
}
```

只有真实使用文件 API 的 crate 才启用 `fs`。测试辅助 feature 不应无条件进入 release binary。

### 4.3 Bytes 和 framing

- `tokio-util::codec` 继续拥有 transport framing；
- decoder 在继续扩容前执行 frame 上限；
- `Bytes`/`BytesMut` 用于 wire buffer；
- recipient fan-out 使用共享只读 `Arc<[WireFrame]>` 或等价结构；
- 同一 event 对同一 dialect 只编码一次；
- 不为每个 recipient clone 完整 frame；
- 不以 `Box<Event>` 或动态分配规避 large-enum 和 ownership 设计。

### 4.4 async trait

认证、Weather、History repository 等低频 port 可以在有明确 object-safety 需求时使用 `async-trait`。packet decode、routing、position update 和 recipient encoding 热路径不使用会为每次调用分配 boxed future 的动态 async trait。

优先选择顺序：

1. 具体类型或 enum dispatch；
2. 泛型静态 dispatch；
3. 冷路径上的 `Arc<dyn Port>`；
4. 只有真实边界价值时才使用 boxed future/dynamic dispatch。

## 5. HTTP 与 Web：Axum/Tower

### 5.1 决策

AsterFSD 的公开 HTTP、Web Control Plane、WebSocket/SSE、管理 API 和健康/指标 HTTP endpoint 使用 Axum/Tower。

原因：

- AsterFSD 的 Web/Control Plane 仍是新边界，没有需要迁移的既有 HTTP 产品实现；
- Axum、Tonic、Hyper 和 Tower 使用相同的 service/middleware 生态，减少 HTTP 类型转换和重复 middleware glue；
- timeout、load shed、concurrency limit、request id、trace 和 auth context 可以使用一致的 Tower 分层方式；
- Tonic 官方 crate family 已经依赖 Axum/Tower 作为 router/service 基础，版本和运行时组合更自然；
- AsterDrive/AsterForge 的 Actix 实现继续提供产品语义、失败边界和测试模式参考，但不是本项目的框架约束；
- HTTP framework 不是 Network Runtime 热路径，框架替换不会改善 FSD packet 处理。

### 5.2 Axum 的职责

- Public REST API；
- Web 注册、登录和账号设置入口；
- Admin/Control API；
- Live Map WebSocket/SSE；
- OpenAPI document endpoint（按部署策略决定是否公开）；
- HTTP health/readiness/metrics endpoint；
- browser cookie、CSRF、CORS 和 public rate limiting。

Axum handler 不直接访问其他服务数据库。handler 只完成：

```text
HTTP request
  -> authentication/authorization
  -> product request type
  -> local service or Tonic client
  -> product result
  -> HTTP response/error mapping
```

### 5.3 与 AsterForge 的关系

AsterForge 现有 Actix 组件不能直接作为 Axum middleware 使用。AsterFSD 可以复用其已验证的安全和行为契约，并在 Tower/Axum 边界实现对应 adapter；只有真正产品中立、经过多个消费者验证的 Tower helper 才回收到 Forge。AsterFSD 保留：

- Identity/Network/History 的错误语义；
- API response envelope；
- actor、network、membership 和 permission mapping；
- route assembly；
- product config；
- audit 分类；
- OpenAPI 中的产品 schema。

不得为了复用 Forge 类型而让 AsterFSD 的公开 API 泄露通用 infrastructure error。

### 5.4 OpenAPI 和前端契约

- Rust handler/domain contract 生成 OpenAPI；
- OpenAPI 生成 Web TypeScript client；
- CI 重新生成并用 `git diff --exit-code` 检查 drift；
- generated file 不手工修改；
- gRPC Protobuf 和 public OpenAPI 是两个独立 contract，不能互相冒充；
- browser 不直接依赖内部 Tonic proto。

具体 OpenAPI derive/generator crate 在 public API ADR 中固定；本 RFC 只固定生成链和 drift gate。

## 6. 内部 RPC：Tonic gRPC

### 6.1 决策

所有跨服务同步 command/query 使用 Tonic。Tonic 当前由官方 `grpc/grpc-rust` 仓库维护，是 AsterFSD 的唯一官方 Rust gRPC 实现。

采用的 crate family：

```text
tonic
tonic-prost
tonic-prost-build
prost
tonic-health
tonic-reflection
```

2026-08-17 的已验证发布基线是：

```text
tonic              0.14.6
tonic-prost        0.14.6
tonic-prost-build  0.14.6
prost              0.14.4
```

这些版本是首次接入的兼容基线，不是永久冻结值。根 `Cargo.toml` 和 `Cargo.lock` 决定当前 checkout 的实际版本。

### 6.2 Listener 分离

默认端口角色：

```text
6809/6810/6811  Tokio raw TCP protocol listeners
8080            Axum HTTP/WebSocket
9090            Tonic internal gRPC
```

Axum 和 Tonic 默认使用独立 listener，以便分别执行 exposure、health、drain 和 scaling。即使二者都实现 Tower `Service`，也不为了减少一个端口而混合 public HTTP 与 internal gRPC 的权限边界。

二者共享的是 application service contract：

```text
Axum adapter -----\
                   -> IdentityService / HistoryQuery / NetworkControl
Tonic adapter ----/
```

### 6.3 Protobuf 治理

- package 使用 `asterfsd.<domain>.v1`；
- source proto 位于 workspace 的 canonical `proto/`；
- generated Rust code 由固定命令生成；
- 使用 Buf CLI 做 lint、format 和 breaking-change check；
- Buf 是构建/CI 工具，不是 runtime crate；
- build 不从网络下载 proto 或 plugin；
- `protoc`/generator 版本固定并记录；
- enum number、field number 和 reserved field 受兼容性门禁保护；
- public message 不复用 persistence entity；
- Protobuf DTO 必须通过显式 conversion 进入 domain/contract type。

### 6.4 gRPC 运行要求

每个方法声明：

- deadline；
- retryable/non-retryable status；
- idempotency key；
- request/response size limit；
- authn/authz；
- cancellation；
- version/capability；
- audit 分类；
- unknown commit outcome。

默认启用标准 gRPC health service。Reflection 仅在开发环境或受保护内部网络启用，生产默认关闭。

### 6.5 TLS 和服务身份

- 使用 rustls；
- Kubernetes Profile 使用 mTLS 或等价 workload identity；
- client identity 映射为明确 service principal；
- network user token 不充当 service credential；
- service certificate、NATS credential、database password 分开轮换；
- gRPC metadata 不记录 bearer token 和完整 credential。

## 7. 事件传输：Local、Core NATS 与 JetStream

### 7.1 决策

官方 transport 分为三种语义，而不是一个模糊的 `publish()`：

```rust
trait RealtimeTransport {
    // 可合并、允许 gap、不能阻塞 Network hot path。
}

trait DurableEventTransport {
    // 可重放、at-least-once、带持久化确认和幂等身份。
}

trait TelemetryIngestTransport {
    // 已采样、分段、压缩的 bounded telemetry；短保留、at-least-once ingest。
}
```

部署映射：

| Profile | Realtime | Durable Event | Telemetry Ingest |
| --- | --- | --- | --- |
| Standalone | bounded in-process channel/coalescer | local outbox + embedded consumer | local bounded spool + embedded History |
| Distributed | Core NATS | NATS JetStream domain streams | short-retention JetStream telemetry stream |
| Kubernetes | Core NATS cluster | replicated JetStream domain streams | replicated bounded telemetry ingest stream |
| Custom | 自定义 adapter | 自定义 adapter | 自定义 adapter |

### 7.2 为什么选择 NATS

截至 2026-08-17：

- NATS 是 CNCF Incubating 项目；
- NATS Server 最新已验证版本为 `2.14.5`；
- Rust `async-nats` 最新已验证版本为 `0.50.0`；
- Server 和 Rust client 在 2026 年保持活跃发布；
- `async-nats` 支持 service、JetStream、NKeys、rustls、FIPS、KV 和 Object Store；
- 官方提供 Helm/Kubernetes 资源；
- 2025 年完成独立的 Trail of Bits 安全审计；
- request/reply、subject routing、pub/sub、edge/leaf topology 与飞行网络的区域化特征匹配。

NATS 被选为官方默认不是因为它能替代所有数据库，而是因为它同时满足：

- 低延迟 realtime fan-out；
- 低运维成本；
- durable consumer/replay；
- service request/reply；
- 多区域/edge 扩展；
- Rust 原生异步客户端；
- Standalone 可完全不依赖它。

### 7.3 Core NATS

Core NATS 是 at-most-once realtime transport。适用：

- position delta；
- current frequency/transponder/visibility；
- map projection 最新状态；
- 允许新值覆盖旧值的 presence hint；
- operator dashboard 的非权威实时视图。

约束：

- 每条 delta 带 `network_id`、shard、entity id 和 sequence；
- consumer 检测 sequence gap；
- slow consumer 不反向阻塞 Gateway；
- gap 后通过 snapshot/query 重建，而不是要求 publisher 重发全部历史；
- 不通过 Core NATS 发送必须审计或必须保存的唯一业务变化。

### 7.4 JetStream

JetStream 是 NATS 的持久化和 streaming 层。适用：

- session authenticated/ended；
- flight plan filed/amended；
- handoff initiated/accepted/rejected；
- activity lifecycle；
- rating、membership、suspension 变化；
- AIRAC activated；
- projection rebuild 所需的 durable source event。

经过 Gateway sampler/segmenter 生成的 `TrackChunk` 也使用 JetStream，但进入独立 `ASTER_TELEMETRY_INGEST` stream。该 stream 使用短保留和 chunk-level 幂等，长期权威存储属于 History/ClickHouse/S3；它不与 domain event stream 混合 retention 和 consumer policy。

生产配置至少明确：

- stream subject；
- storage type；
- replicas；
- retention policy；
- max age/bytes/messages；
- duplicate window；
- consumer type；
- ack policy；
- ack wait/backoff；
- max delivery；
- dead-letter/quarantine policy；
- replay 和 restore 方法。

### 7.5 交付语义

JetStream 支持 publish acknowledgement、consumer acknowledgement、publisher deduplication 和 double ack。AsterFSD 仍采用以下工程语义：

```text
producer: at-least-once
consumer: idempotent
side effect: CAS/fencing/unique constraint
projection: replayable
```

原因是 broker 的 exactly-once 不覆盖：

- consumer 对 PostgreSQL/ClickHouse 的独立事务；
- gRPC command 的不确定提交；
- side effect 已完成但 ack 尚未送达；
- producer outbox 与 JetStream 之间的跨系统边界；
- 多 projection 的不同失败时刻。

事件 envelope 必须包含稳定 `event_id`。消费者按 `event_id` 和业务版本去重，旧 processing token 不得覆盖新状态。

### 7.6 Subject 规范

建议 canonical subject：

```text
aster.network.<network_id>.realtime.position.<shard_id>
aster.network.<network_id>.realtime.presence.<shard_id>
aster.network.<network_id>.event.session
aster.network.<network_id>.event.flight_plan
aster.network.<network_id>.event.handoff
aster.network.<network_id>.event.activity
aster.network.<network_id>.telemetry.track.<bucket>
aster.identity.event.principal
aster.navdata.event.airac
```

约束：

- subject 不包含 password、token、email 或未规范化用户输入；
- network id、tenant id 和 shard id 使用稳定编码；
- subject 决定粗粒度路由，payload 决定业务内容；
- 版本在 envelope/schema 中表达，不通过无限复制 subject 层级逃避兼容治理；
- wildcard subscription 必须有服务权限和流量上限。

### 7.7 安全和运维

- 使用 TLS；
- 使用 NKeys/JWT account 或等价最小权限凭据；
- Gateway 只允许 publish 它拥有的 subject；
- History/Map consumer 只有需要的 subscribe 权限；
- NATS monitoring endpoint 不公开公网；
- NATS credential 不写入 TOML 示例、日志或镜像；
- Kubernetes 使用 Secret/provider 注入；
- 生产 JetStream 使用奇数节点和明确 replication；
- snapshot/restore 与灾难恢复演练独立验证。

### 7.8 禁止滥用

- 不使用 NATS KV 作为 Identity 或 Network Runtime 权威数据库；
- 不使用 NATS Object Store 替代正式 archive contract；
- 不把每个 position 都写入永久 JetStream retention；
- 不让 Gateway 同步等待所有 History consumer；
- 不把 broker availability 等同于 platform readiness；
- 不让 Standalone 为了接口统一而启动内置 NATS server；
- 不依赖 broker “exactly once” 删除应用幂等和 fencing。

## 8. 消息系统备选结论

### 8.1 Redpanda/Kafka

适用：

- 极大规模持久日志；
- 长期 partition ordering；
- 大量独立消费者；
- Kafka Connect/Flink/生态集成；
- 已有平台团队负责 Kafka-compatible infrastructure。

不作为默认：

- Standalone 和中小型飞行网络的运维成本过高；
- request/reply 和 edge/leaf 使用方式不如 NATS 自然；
- 不应因为其他 Aster 项目部署过 Redpanda，就把它强加给 AsterFSD。

保留 `DurableEventTransport` adapter，未来大型运营方可以选择 Redpanda。

### 8.2 RabbitMQ

RabbitMQ 对复杂 work queue、routing 和企业运维很成熟，但 AsterFSD 同时需要高频 realtime fan-out、低延迟 subject 和 replay。它不作为官方默认事件 backbone，可以作为 custom task transport。

### 8.3 Redis Streams/PubSub

- Redis PubSub 无持久 replay，不适合作为 durable event transport；
- Redis Streams 能表达 consumer group，但平台 ownership、长期 retention 和 event topology 不如专用系统清晰；
- Redis 可以服务 rate limit、短期 cache 或 Web session，但不是 Network Runtime 或 History 权威。

### 8.4 PostgreSQL LISTEN/NOTIFY

`LISTEN/NOTIFY` 可以做局部 invalidation hint，不作为跨服务 durable transport。PostgreSQL outbox 仍用于数据库事务与事件发布之间的可靠边界。

## 9. 数据库和持久化

### 9.1 支持级别

| 数据库 | 定位 | 保证 |
| --- | --- | --- |
| SQLite | Standalone 默认 | 官方 schema、migration、运行和恢复测试 |
| PostgreSQL | Distributed/Kubernetes 默认 | 官方 HA、migration、性能和运维验证 |
| MySQL | 可选兼容级 | feature-gated 编译和契约测试；生产保证按实际需求扩大 |
| ClickHouse | 大型 History/Analytics Profile | telemetry ingest、retention、query 和恢复测试 |
| S3/Parquet | 长期归档 | export/import、schema version、完整性和 retention |

### 9.2 SQLite

- 单机、开发、测试和轻量网络；
- 使用 WAL、busy timeout 和有界 pool；
- migration 使用独占 owner；
- 测试使用临时数据库；
- config example 默认 SQLite；
- 不宣称 SQLite 支持多 Pod 共享写入。

### 9.3 PostgreSQL

PostgreSQL 是 Identity、Control Plane、业务元数据、outbox 和紧凑 History Profile 的生产默认。

要求：

- 每个 service 有独立 role/database/schema ownership；
- 禁止跨 service 表 join；
- migration 使用独立 Job/owner；
- 多副本启动不并发执行未加锁 migration；
- 索引和 partition 变更有真实 PostgreSQL 测试；
- backup/restore、PITR 和 schema compatibility 独立演练；
- connection pool 和 timeout 按服务配置，不从 Gateway 全局共享。

### 9.4 SeaORM 与 SQLx

SeaORM 适用：

- Identity、membership、rating、activity 和 control CRUD；
- entity/repository；
- 通用 transaction；
- 跨 SQLite/PostgreSQL/MySQL 的明确公共子集。

SQLx 适用：

- History 批量写入；
- PostgreSQL partition/CTE/数据库专属语义；
- 高吞吐 query；
- SeaORM 无法清晰表达的性能关键语句。

必须修复的依赖方向：

```text
错误：aster_fsd_model -> sea-orm

正确：aster_fsd_model <- aster_fsd_persistence -> sea-orm/sqlx
```

领域 enum 与 ORM ActiveEnum 分开，由 persistence 显式转换。数据库字符串、column type 和 migration 不进入 model crate。

### 9.5 数据库 feature

不得默认同时把 SQLite、PostgreSQL 和 MySQL driver 编入所有 artifact。建议：

```toml
[features]
default = ["sqlite"]
sqlite = ["aster_fsd_persistence/sqlite"]
postgres = ["aster_fsd_persistence/postgres"]
mysql = ["aster_fsd_persistence/mysql"]
full-database = ["sqlite", "postgres", "mysql"]
```

CI 单独验证每个 feature 组合和 `full-database`，避免只测试 feature-unified 结果。

## 10. History 和 Analytics Profile

### 10.1 Compact History

适用于 Standalone 和中小型部署：

```text
SQLite/PostgreSQL
├── sessions
├── durable events
├── flight plans
├── handoffs
└── sampled position tracks
```

Position 按策略采样、分区和清理，不能无限增长。

### 10.2 Large History

适用于大型平台：

```text
JetStream
  -> History Ingest
  -> ClickHouse
  -> S3/Parquet archive
```

ClickHouse 负责：

- 大规模 position/time-series ingest；
- 时间范围查询；
- track、在线人数和活动统计；
- materialized projection；
- TTL 和冷热分层。

PostgreSQL 继续负责业务元数据和 transaction，不因为引入 ClickHouse 就把 Identity/Control 搬入分析数据库。

### 10.3 Archive

- S3-compatible object storage；
- Parquet 或等价列式开放格式；
- manifest 记录 schema version、network、时间范围、row count、checksum 和 producer version；
- archive write 使用临时对象 + commit manifest；
- export/import 幂等；
- 删除和 retention 有审计；
- archive 不是在线 Network Snapshot 的替代品。

## 11. 可观测性

### 11.1 tracing

所有 crate 使用 `tracing` 作为统一埋点 API。禁止新业务代码直接使用 `println!` 或建立第二套 logging facade。

必需字段包括：

- `service`、`version`、`deployment`；
- `network_id`、`shard_id`；
- `connection_id`、`peer`、`dialect`；
- `command`、`direction`、`phase`；
- `event_id`、sequence、correlation/causation id；
- queue depth、wire bytes、elapsed；
- error category，不记录 credential/raw login payload。

### 11.2 AsterForge Logging

`aster_forge_logging` 负责：

- text/JSON formatting；
- stdout/file/rotation；
- filter 和 `RUST_LOG` integration；
- tracing subscriber 装配；
- panic/log bridge（如果需要）。

AsterFSD 保留字段、事件名称、redaction 和默认 filter 的产品契约。默认日志等级继续是 `info`。

### 11.3 Prometheus

Prometheus 是官方 metrics exposition。至少包含：

- connections/sessions；
- packet/frame/decode error；
- mailbox depth/slow consumer；
- core command latency；
- gRPC latency/status/deadline；
- NATS publish ack、consumer lag、redelivery；
- outbox/spool depth/age；
- History ingest/projection lag；
- database pool/query/migration；
- drain、restart 和 snapshot age。

label 必须控制基数。callsign、connection id、event id 和 peer address 不作为常规 metrics label。

### 11.4 OpenTelemetry

OpenTelemetry/OTLP 是可选 exporter，不进入 model、protocol 或 core API：

```text
tracing spans
  -> composition root subscriber/layer
  -> OTLP Collector
```

原因：

- tracing 是 Rust 内部稳定埋点边界；
- OpenTelemetry crate 版本联动不应传播到核心 crate；
- Standalone 不需要 Collector；
- exporter 故障不能阻塞 packet 和 gRPC request。

## 12. 配置和 Secret

### 12.1 配置格式

- TOML 继续作为 Standalone canonical example；
- 配置反序列化到强类型 struct；
- 环境变量用于部署覆盖；
- Secret 支持独立文件/Secret provider 注入；
- 启动时打印配置来源和非敏感覆盖信息；
- 未识别关键字段、非法 mode 和冲突配置启动失败；
- 默认 logging level 保持 `info`。

### 12.2 Profile 配置

示意：

```toml
[deployment]
profile = "standalone" # standalone | distributed | kubernetes

[identity]
mode = "embedded" # embedded | grpc

[events]
mode = "local" # local | nats

[history]
mode = "embedded" # embedded | grpc
store = "sqlite" # sqlite | postgres | clickhouse
```

配置 mode 只决定 adapter 和部署位置，不改变领域 contract。

### 12.3 Secret 规则

- password/token/key 不进入普通 TOML example；
- 环境变量名称可以配置 Secret 引用，但启动日志不打印值；
- database URL 日志必须 redaction；
- NATS credential、mTLS private key、JWT signing key 分开管理；
- CI 不持有生产 cluster-admin、数据库密码或解密 archive；
- Secret rotation 不要求重编译 binary。

## 13. Cargo 和供应链治理

### 13.1 Workspace 单一事实源

根 `Cargo.toml` 统一管理：

- package metadata；
- edition/MSRV；
- direct dependency versions；
- path dependency；
- lint；
- feature/Profile；
- release/profiling profile。

member 使用 `[lints] workspace = true`。direct dependency 必须有真实调用或明确 feature-unification 理由。

### 13.2 Git dependency

当前 `aster_forge_logging` 使用未声明 `rev` 的 Git dependency。正式发布必须：

- 优先使用已发布 semver crate；或
- 固定 immutable `rev`；
- 本地 sibling 开发通过 `[patch]`；
- release metadata 记录实际 Forge revision；
- 不依赖远端默认分支漂移。

### 13.3 升级

常规和不兼容升级执行项目契约规定的 Cargo 流程。升级后检查：

- `cargo tree -e features`；
- MSRV；
- all targets/all features；
- 单数据库 feature matrix；
- protobuf generated diff；
- NATS/Tonic interoperability；
- `cargo machete`；
- binary size 和热路径 allocation；
- security advisory、许可证和 provenance。

不手工编辑 `Cargo.lock`。

## 14. 部署 Profile

### 14.1 Standalone

必须能够只依赖一个 binary 和本地数据目录运行：

```text
asterfsd
├── Tokio protocol listeners
├── optional Axum Web
├── embedded services
├── bounded local event transport
├── local outbox/projection
└── SQLite
```

不要求：

- NATS；
- PostgreSQL；
- ClickHouse；
- Redis；
- OpenTelemetry Collector；
- Kubernetes。

### 14.2 Distributed

```text
Gateway
├── Tonic -> Identity
├── Tonic -> Control/Dispatch
├── Core NATS -> Live Map
└── JetStream -> History/Projection

Axum Web
├── Tonic -> Identity
├── Tonic -> History
└── WebSocket/SSE -> Browser
```

PostgreSQL 是 operational 默认。Compact History 可以继续 PostgreSQL，大型 History 使用 ClickHouse。

### 14.3 Kubernetes

- protocol Gateway 按 network/shard 调度；
- HTTP Axum 和 Tonic gRPC 可独立 scaling；
- NATS/JetStream 使用官方 Helm/Kubernetes 资源和持久 volume；
- data service 使用 ClusterIP，但仍需 NetworkPolicy；
- HTTP、gRPC、NATS 和数据库分别有最小权限；
- migration 使用独立 Job；
- liveness、readiness、startup、drain 分离；
- image 使用 immutable digest；
- rollout 验证 Pod `imageID`、Service、EndpointSlice、route、health、migration 和 event lag；
- broker/History 故障不把已建立的 Network Runtime 连接全部判为 unhealthy。

## 15. 故障语义

| 故障 | Gateway 行为 | Control/Web 行为 | 数据行为 |
| --- | --- | --- | --- |
| Identity 不可用 | 已登录 session 按 policy 继续；新登录失败 | Identity mutation 失败 | 不缓存明文 credential |
| Core NATS 不可用 | packet/routing 继续 | realtime map 可能落后 | delta 丢弃/合并并计数 |
| JetStream 不可用 | packet 热路径继续 | 需要 durable audit 的 control operation 按 policy 拒绝 | 进入有界 outbox/spool |
| History 不可用 | 不影响在线连接 | history query 失败 | consumer lag/backlog 增长 |
| Map 不可用 | 不影响在线连接 | map API 降级 | 从 snapshot/event 重建 |
| PostgreSQL 不可用 | 已建立 session 不因查询阻塞 | identity/control mutation 失败 | outbox/任务暂停 |
| ClickHouse 不可用 | 无影响 | analytics query 降级 | ingest backlog/retry |
| OTLP/Prometheus 不可用 | 无影响 | 无影响 | exporter 丢弃/重试有界 |

任何 outbox/spool 都必须配置最大 bytes、最大 age 和告警。容量耗尽时按事件 durability class 决定拒绝新操作或丢弃 telemetry，不允许无限占用磁盘。

## 16. 测试和验收

### 16.1 Axum/Tower

- auth/session/CSRF/CORS/rate limit；
- public/internal route exposure；
- OpenAPI/generated client drift；
- WebSocket/SSE reconnect/backpressure；
- product error mapping；
- request/body/stream 上限；
- graceful shutdown。

### 16.2 Tonic

- local 与 gRPC implementation 语义一致；
- unary/streaming exact contract；
- deadline、cancel、retry、idempotency；
- mTLS/service identity；
- health/reflection policy；
- request/response size；
- incompatible proto breaking gate；
- mixed-version interoperability。

### 16.3 NATS/JetStream

- Core NATS disconnect 和 gap recovery；
- JetStream publish ack；
- durable pull consumer；
- duplicate event；
- consumer crash before/after side effect；
- ack lost/redelivery；
- max delivery/quarantine；
- retention/replica/storage；
- outbox recovery；
- NATS restart 和 JetStream snapshot/restore；
- unauthorized subject publish/subscribe；
- slow consumer 和 bounded Gateway behavior。

### 16.4 数据库

- SQLite temporary runtime；
- PostgreSQL migration/concurrency/rollback；
- 每个 database feature 单独构建和测试；
- repository domain conversion；
- batch History；
- partition/retention；
- ClickHouse ingest/query/retry；
- archive checksum/export/import；
- credential redaction。

### 16.5 部署

- Standalone 无外部服务 smoke；
- Distributed multi-process contract test；
- Kubernetes NetworkPolicy allowed/denied probe；
- current image digest；
- Service/EndpointSlice；
- gRPC health；
- NATS cluster/stream readiness；
- migration Job；
- rolling drain/rollback；
- Identity/NATS/History/ClickHouse 故障注入；
- real client connection 在非核心服务故障期间保持。

## 17. 已拒绝的选择

### Actix Web 作为公开 Web 默认

拒绝原因：Actix 本身成熟且团队熟悉，但 AsterFSD 的 Web/Control Plane 尚未形成需要保护的既有实现。新项目选择 Axum/Tower 可以与 Tonic 共用 middleware/service 心智、Hyper/HTTP 类型和可观测性集成。AsterDrive/AsterForge 的 Actix 代码只作为行为参考，不形成新项目的技术锁定。

### NATS 作为 Standalone 强制依赖

拒绝原因：破坏快速开服目标，并把单进程内部事件变成不必要的外部运维依赖。

### Redpanda/Kafka 作为所有部署默认

拒绝原因：对普通飞行网络过重。它保留为大型 durable event adapter。

### Redis 作为 session/callsign 权威

拒绝原因：Network Runtime 必须拥有明确 single-writer/shard ownership，Redis 不能替代状态机和索引原子性。

### 全部数据使用 SeaORM

拒绝原因：History 批量写入、partition、ClickHouse 和数据库专属优化需要更明确的 SQL/adapter。

### OpenTelemetry 类型进入 core

拒绝原因：exporter/version churn 不应污染 protocol/model/core；核心只产生 tracing span/event 和稳定 metrics 语义。

### 共享数据库作为服务集成

拒绝原因：破坏 schema ownership、独立发布、权限和故障隔离。

## 18. 实施约束

本 RFC 固定最终技术方向，不建立长期“双框架”或“双 transport”临时架构：

- Axum/Tower 是公开 HTTP 的唯一官方框架；
- Tonic 是内部 gRPC 的唯一官方实现；
- local/NATS 是同一 EventTransport contract 的不同 Profile，不是两套领域实现；
- NATS adapter 不进入 core；
- Protobuf DTO 不进入 model；
- ORM derive 从 model 移出；
- 数据库 driver feature 收紧；
- AsterForge dependency 固定版本/revision；
- 引入外部系统时同时添加失败测试、指标、文档和 config example；
- 内部 API 调整一次更新调用方，不保留薄兼容 facade。

编译检查点和 focused tests 是实施风险控制，不代表允许留下半迁移结构。

## 19. 完成标准

本 RFC 对技术栈落地的完成标准是：

1. Axum HTTP 与 Tonic gRPC 默认使用独立 listener，并共享明确 application service contract 和受控 Tower 基础层。
2. Tonic proto 有 Buf lint/breaking gate、固定生成入口和 local/gRPC parity test。
3. Standalone 在没有 NATS/PostgreSQL/ClickHouse 的环境完整运行。
4. Distributed Profile 使用 Core NATS 传 realtime delta、JetStream 传 durable event。
5. durable consumer 在重复、redelivery、ack lost 和进程崩溃下保持幂等和 fencing。
6. NATS/History/Map 故障不阻塞 Network packet 热路径。
7. SQLite/PostgreSQL/MySQL feature 独立，model 不依赖 SeaORM。
8. PostgreSQL operational、ClickHouse telemetry、S3 archive 的 ownership 不混合。
9. tracing、Prometheus 和可选 OTLP exporter 有稳定字段、基数和 redaction 约束。
10. Kubernetes 验证真实 image、Service/EndpointSlice、NetworkPolicy、gRPC/NATS health、migration 和 rollback。
11. 根 Cargo 治理依赖版本、feature、MSRV、lint 和 release profile。
12. README、config example、developer docs、conformance、changelog 与实现同步。

## 20. 后续 ADR

以下主题在实际接入前形成独立 ADR，但不得改变本 RFC 的总选择：

- [RFC-0003](0003-identity-and-trust-architecture.md) 下的 Tonic Identity v1 proto 和 service error model；
- [RFC-0004](0004-network-runtime-sharding-and-high-availability.md) 下的 Network Directory、Core NATS subject、snapshot 和 Kubernetes shard topology；
- Axum public API、OpenAPI generator 和 Web client generation；
- [RFC-0005](0005-event-model-and-delivery-semantics.md) 固定的 Event envelope、subject naming、schema compatibility、JetStream consumer 语义和 local outbox/inbox 边界；
- JetStream stream/consumer/retention/replication 参数基线；
- local outbox/spool 的具体 schema 和恢复算法；
- [RFC-0006](0006-history-replay-and-telemetry-architecture.md) 下的 PostgreSQL/ClickHouse History schema、TrackChunk、retention 和 S3/Parquet archive；
- OTLP/Prometheus exporter 和 label budget；
- Kubernetes NATS topology、storage class、PDB、backup/restore；
- Redpanda custom adapter contract；
- feature/package artifact matrix。

这些 ADR 可以细化版本、参数和实现，但不能把 Actix、Kafka、Redis、ClickHouse、OpenTelemetry Collector 或外部 broker 变成 Standalone 的强制依赖。
