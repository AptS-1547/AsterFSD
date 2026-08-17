# AsterFSD 架构概览

视觉总览：[AsterFSD Platform Architecture Diagrams](platform-architecture-diagrams.md)。

## RFC

- [RFC-0001：AsterFSD Platform Architecture](rfcs/0001-asterfsd-platform-architecture.md)：平台产品边界、Network Runtime、Identity、History/Projection、gRPC contract、Standalone/Kubernetes Profile、可靠性和企业级运维约束。
- [RFC-0002：Technology Stack and Infrastructure Profiles](rfcs/0002-technology-stack-and-infrastructure-profiles.md)：Axum/Tower、Tonic、Core NATS/JetStream、数据库、可观测性、Cargo feature 和部署基础设施选型。
- [RFC-0003：Identity and Trust Architecture](rfcs/0003-identity-and-trust-architecture.md)：Account、Organization、Network Profile、Rating、credential、权限、suspension、embedded/gRPC Identity 和 Gateway admission。
- [RFC-0004：Network Runtime, Sharding and High Availability](rfcs/0004-network-runtime-sharding-and-high-availability.md)：Gateway/Runtime shard、Network Directory、callsign ownership、epoch fencing、跨 shard routing、drain、snapshot 和高可用语义。
- [RFC-0005：Event Model and Delivery Semantics](rfcs/0005-event-model-and-delivery-semantics.md)：Command/Query/Effect、realtime delta、durable domain event、outbox、幂等、顺序、schema 演进、Core NATS/JetStream 和 replay 语义。
- [RFC-0006：History, Replay and Telemetry Architecture](rfcs/0006-history-replay-and-telemetry-architecture.md)：History ownership、TrackChunk、采样、telemetry ingest、PostgreSQL/ClickHouse/S3、retention、gap 和 replay consistency。
- [RFC-0007：Activity and Dispatch Integration](rfcs/0007-activity-and-dispatch-integration.md)：Activity lifecycle、registration/slot/assignment、Runtime policy projection、Dispatch Release revision、Weather/AIRAC 引用和 Network filing。
- [RFC-0008：Weather, AIRAC and Route Data Plane](rfcs/0008-weather-airac-and-route-data-plane.md)：Weather provenance/freshness、immutable AIRAC generation、Network overlay、Route Service、provider sync、atomic activation 和 Dispatch reference。
- [RFC-0009：ATC Coordination and Handoff State Machine](rfcs/0009-atc-coordination-and-handoff-state-machine.md)：Controller jurisdiction、tracking ownership、typed handoff、cross-shard coordination、disconnect/timeout 和 Classic/VATSIM/Aster mapping。

## 60 秒版本

- 根 `asterfsd` 只装配配置、日志、数据库和 runtime。
- listener 显式选择 `classic`、`vatsim` 或 `aster_v1`，协议不会靠第一包猜测。
- `aster_fsd_codec` 在 transport 分配边界实施有界 framing。
- 各协议 backend 把 wire frame 解码成 `aster_fsd_model::Command`。
- `aster_fsd_core::Network` 是唯一 client/session/callsign/position/flight-plan 权威来源。
- weather lookup 通过 core 的异步 `WeatherProvider` port 进入统一 event；classic backend 将 parsed profile 编成 C 顺序的 `#TD/#WD/#CD`。
- core 产生 delivery effect；server 为每个 recipient 调用其 dialect encoder。
- 每个连接有独立 bounded mailbox，direct response、broadcast exclude 和 disconnect 不再共享 magic socket address。
- 认证通过 `aster_fsd_auth::Authenticator` port 注入，SeaORM 实现在 `aster_fsd_persistence`。

## Workspace

```text
asterfsd
├── aster_fsd_server
│   ├── aster_fsd_core
│   │   ├── aster_fsd_model
│   │   └── aster_fsd_auth
│   └── aster_fsd_protocol
│       ├── aster_fsd_codec
│       └── aster_fsd_model
├── aster_fsd_protocol_classic
├── aster_fsd_protocol_vatsim
├── aster_fsd_protocol_aster
├── aster_fsd_persistence
└── aster_fsd_migration
```

具体 backend 只在 composition root 注册，`aster_fsd_server` 与 `aster_fsd_core` 都不依赖具体 dialect crate。

## 一个 packet 如何流转

1. listener accept TCP stream，分配单调递增 `ConnectionId`。
2. `Framed<TcpStream, FsdFrameCodec>` 在 frame 超限前停止扩容。
3. listener 对应的 `ProtocolBackend` 解码 frame。
4. connection task 把统一 `Command` 交给 `Network::execute()`。
5. core 校验连接状态、source ownership、callsign uniqueness 和 command 字段。
6. core 原子更新权威状态并产生 `Effect`。
7. server 根据 typed recipients 解析当前连接集合。
8. 每个 recipient 使用自己的 backend 把统一 `Event` 编码为 wire frame。
9. frame 进入该连接的 bounded mailbox，由单 writer 保序写出。

## Listener 登录边界

- classic 在 accept 后静默，直接接收 revision `9` 的 `#AA/#AP`。
- VATSIM 先发送 `$DI`，只接受完整 `$ID` 后的 revision `100` 登录；identification 与 login 的 callsign/network ID 共同形成 ownership 边界。
- VATSIM 登录成功后的 CAPS、IP、flight-plan/controller profile 是 core typed event，不是 server task 拼接的临时 wire。
- Aster v1 通过 JSON `v = 1` 进入相同登录状态机。

这些差异只属于 adapter。认证结果、callsign 唯一性、active phase 和失败关闭仍由同一个 `Network` 强制执行。

## 为什么 backend 编码发生在 recipient 侧

同一网络里可以同时存在 classic pilot、VATSIM ATC 和 Aster observer。core 广播的是“ECP4143 的 pilot position 已更新”，而不是 `@S:...` 字符串：

```text
Event::Position(PilotPosition)
  -> classic encoder -> @S:ECP4143:...
  -> vatsim encoder  -> @S:ECP4143:...
  -> aster encoder   -> {"v":1,"type":"position",...}
```

这样新增协议只添加 adapter，不复制网络状态和 use case。

### 热路径分配边界

- core 使用 `Effects { deliveries, close }`，把 delivery 与 close 分开建模，`Event` inline 存放；不通过 `Box<Event>` 规避 large-enum 诊断。
- 一条 delivery 的 event 在 recipient 循环中只借用，不按客户端 clone。
- server 按 dialect 缓存编码结果；同一 classic/VATSIM/Aster event 每种 dialect 只编码一次。
- `WireFrame` 使用 `Bytes`，mailbox 使用共享 `Arc<[WireFrame]>`；同 dialect fan-out 只增加引用计数，不复制完整 frame buffer。
- direct control event 自带 typed target；即使 core 已释放 session，disconnect frame 也不依赖失效 snapshot，并可安全进入 dialect cache。
- Argon2 的 blocking allocation 与 protocol event hot path 分离，并受连接任务并发边界约束。

## 权威状态

```text
Network
├── sessions: ConnectionId -> Session
├── callsigns: NormalizedCallsign -> ConnectionId
└── sequence: monotonic event sequence

Session
├── phase / dialect / peer / generation
├── authenticated identity
├── client presence
├── current position
├── current flight plan
└── outbound mailbox
```

注册、登录、logoff 和异常断开一次修改两个索引。任何路径都不能只删 `sessions` 或只删 `callsigns`。

## 配置

listener 是列表：

```toml
[[listeners]]
name = "classic"
protocol = "classic"
address = "0.0.0.0"
port = 6809
max_frame_bytes = 511

[[listeners]]
name = "vatsim"
protocol = "vatsim"
address = "0.0.0.0"
port = 6810
max_frame_bytes = 4096

[[listeners]]
name = "aster"
protocol = "aster_v1"
address = "0.0.0.0"
port = 6811
max_frame_bytes = 16384
```

只启用配置中存在的 listener；默认配置仍只启用 classic `6809`，避免无意暴露额外端口。
