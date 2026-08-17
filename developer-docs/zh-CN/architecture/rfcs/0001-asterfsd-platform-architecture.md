# RFC-0001：AsterFSD Platform Architecture

| 字段 | 内容 |
| --- | --- |
| 状态 | Proposed |
| 日期 | 2026-08-17 |
| 负责产品组 | AsterFSD Platform |
| 影响范围 | 产品边界、workspace、服务契约、运行时、数据、部署、运维和测试 |
| 取代 | MVP 单体 processor/handler/broadcast 设计 |
| 相关契约 | `architecture/project-contract.md`、`architecture/index.md`、`architecture/protocol-backends.md`、[RFC-0002](0002-technology-stack-and-infrastructure-profiles.md)、[RFC-0003](0003-identity-and-trust-architecture.md)、[RFC-0004](0004-network-runtime-sharding-and-high-availability.md)、[RFC-0005](0005-event-model-and-delivery-semantics.md)、[RFC-0006](0006-history-replay-and-telemetry-architecture.md)、[RFC-0007](0007-activity-and-dispatch-integration.md) |

## 1. 摘要

AsterFSD 不再被定义为“一个兼容 C FSD 的服务器”。它是面向飞行模拟网络的可组装平台，负责把多种协议客户端接入同一个权威飞行网络，并向身份、管制、活动、天气、航路、历史数据、实时地图和 Web 控制面提供稳定的产品契约。

FSD、VATSIM 和 Aster 协议只是 wire adapter。协议差异不应复制 session registry、callsign、position、flight plan、认证或路由实现。实时网络状态由 Network Runtime 负责；账号由 Identity 负责；长期连飞数据由 History 负责；地图、统计和回放由 Projection 服务负责。

本 RFC 的核心决策如下：

1. Standalone、Distributed 和 Kubernetes 是同一架构的部署 Profile，不是产品开发阶段，也不允许维护两套实现。
2. 每个能力同时定义 `LocalService` 和 `GrpcClient`（必要时还有第三方 adapter），通过同一份 contract 接入。
3. Gateway 拥有实时网络状态，但不拥有账号权威和长期历史数据库。
4. 高频位置数据走有界 realtime stream；低频业务变化走可重放、可幂等的 durable domain event。
5. 服务之间禁止共享数据库表、跨 bounded context join 和分布式事务；每个服务拥有自己的数据边界。
6. 远程服务的 deadline、retry、幂等、熔断、版本和权限必须显式建模，不能把网络调用伪装成本地函数。
7. AsterForge 只提供工程基础设施；AsterFSD 保留飞行网络的产品语义、数据模型、策略和协议兼容性。

## 2. 背景与问题

当前代码已经从 MVP 单体迁移为统一的 protocol -> command -> core -> effect -> dialect encoder 链路，但产品未来会超出单一 FSD listener：

- 用户需要通过 Web 注册账号、管理 network credential 和查看 rating；
- Gateway 需要支持内置认证和外部认证服务；
- 连飞数据需要长期保存、查询、回放和统计；
- Web 需要实时地图，而不能直接读取 Gateway 的内部锁和内存；
- Dispatch、管制移交、活动、Weather、AIRAC 和航路同步会形成独立领域；
- 小型连飞要求单 binary、SQLite、无需 broker 即可运行；
- 大型部署要求 gRPC、独立数据库、Kubernetes、水平扩展、审计和故障恢复；
- 运营方需要能够选择官方模块，或实现自己的 Rust adapter 后重新编译 distribution。

如果现在只围绕“FSD server”继续加 provider，最终会产生以下结构性问题：

- Web 数据库和 FSD 数据库同时维护账号，出现双重权威；
- Gateway 同步写历史库，History 故障拖慢 position 热路径；
- 三个协议分别维护自己的 client registry，跨协议客户端无法正确互相可见；
- 服务拆分后仍共享 ORM entity 和数据库表，无法独立发布；
- 把 gRPC 当作普通函数调用，缺少超时、重试和不确定提交处理；
- Kubernetes 部署只验证 YAML 渲染，不验证实际 image、Service、EndpointSlice、权限和 rollout；
- 任务或异步机制的内部细节泄露到用户 API，业务契约被实现方式绑死。

本 RFC 先固定长期所有权和失败语义，再决定具体服务数量和部署拓扑。

## 3. 产品和组织边界

### 3.1 AsterFSD Platform 产品组

AsterFSD 作为独立产品组，拥有以下职责：

- Flight Network Runtime：连接、会话、callsign、position、flight plan、routing、管制协调；
- Protocol Suite：classic FSD、VATSIM、Aster 原生协议及后续协议；
- Identity Integration：网络身份、credential、rating、suspension 和授权策略；
- Control Plane：网络配置、Dispatch、活动、Weather、AIRAC、运维命令；
- Data Plane：历史、回放、实时地图、统计和查询投影；
- Distribution：Standalone、Docker、Helm/Kubernetes 和自定义 distribution；
- Compatibility：C 实现、真实 Swift/ATC/Pilot 客户端和各协议 conformance matrix；
- Security/Operations：审计、服务身份、SLO、容量、灾难恢复和升级策略。

### 3.2 AsterForge 边界

AsterForge 是共享工程基础设施，不是飞行网络业务框架。可以复用：

- tracing/logging、request id、security headers、CSRF、CORS、限流和 metrics 基础组件；
- gRPC/HTTP server helper、配置、健康检查和 runtime component 装配模式；
- 通用 storage、external-auth、测试和发布工具；
- CI、SBOM、provenance、镜像、Helm 和通用 Kubernetes 约束。

AsterFSD 必须保留：

- rating、callsign、session、flight plan、handoff、network membership 等产品语义；
- 认证结果到 Network admission 的映射；
- FSD/VATSIM/Aster wire 行为和兼容性；
- AsterFSD 的 entity、repository、migration 和事件语义；
- 飞行网络的错误代码、审计分类和权限策略。

Forge adapter 可以很薄，但不能把产品语义转移给 Forge，也不能用 Forge 的通用类型取代 AsterFSD 的领域类型。

## 4. 领域分层

平台采用四个平面。它们是长期所有权边界，不代表必须部署成四组 Pod。

```text
AsterFSD Platform
├── Network Data Plane
│   ├── Protocol Gateway
│   ├── Session Runtime
│   ├── Callsign Registry
│   ├── Position / Flight Plan
│   ├── Routing
│   └── ATC Coordination
├── Control Plane
│   ├── Identity
│   ├── Network Administration
│   ├── Dispatch
│   ├── Activities
│   ├── NavData / AIRAC
│   ├── Weather
│   └── Policy
├── Projection Plane
│   ├── History
│   ├── Replay
│   ├── Live Map
│   ├── Statistics
│   └── Public Read APIs
└── Integration Plane
    ├── Local service adapters
    ├── gRPC clients/servers
    ├── Event/realtime transport
    ├── External providers
    └── Deployment adapters
```

### 4.1 Network Data Plane

Network Data Plane 是低延迟、连接密集、状态权威的热路径。它必须能够在没有数据库或消息 broker 的情况下处理已建立连接的实时流量。

Gateway 的职责：

- 接受显式 listener 上的协议连接；
- 在 transport 分配边界实施 frame 上限；
- 调用 dialect decoder，得到统一 `Command`；
- 调用 `Network::execute()` 校验 session phase、source ownership、callsign uniqueness 和字段约束；
- 原子更新实时权威状态；
- 产生 typed `Effect`；
- 为每个 recipient 使用正确的 dialect encoder；
- 通过 bounded mailbox 和单 writer 保证每个连接的顺序；
- 在 drain、EOF、writer error、idle timeout 和 shutdown 时释放 session/callsign 双索引。

Gateway 不负责：

- 直接查询 Web users 表；
- 直接写 History 表；
- 在 protocol handler 内实现产品认证策略；
- 为每个协议复制一套 registry；
- 在 position 热路径中同步调用远程服务；
- 用 socket address、全局广播或 magic string 表达 recipient。

### 4.2 Control Plane

Control Plane 处理低频、可审计、需要管理员或用户意图的操作，例如：

- 注册账号、修改 network credential、rating 和 suspension；
- 创建网络、配置 listener、限制 client capability；
- Dispatch 任务和管制席位；
- 活动创建、报名、开始、结束和参与者权限；
- Weather source、station lookup、AIRAC 发布和航路同步；
- kick、ban、handoff policy、network maintenance 等控制命令。

Control Plane 的 command 必须带有 actor、tenant/network、request id、幂等键（适用时）和审计分类。不能把“收到 HTTP 请求”直接当作业务状态已经成功。

### 4.3 Projection Plane

Projection Plane 只提供面向查询的模型，不反向成为 Network Runtime 的权威状态。

- History Store：session、flight plan、position track、handoff、activity 和事件历史；
- Live Map Projection：当前 aircraft/controller 的可读快照；
- Replay Projection：按时间窗口回放的轨迹；
- Statistics Projection：在线人数、峰值、活动统计和运营报表；
- Public Read API：向 Web、客户端工具和第三方查询提供稳定读模型。

地图服务挂掉不能影响 FSD 连飞；History 查询变慢不能阻塞 Gateway；Projection 落后必须可观测并能从事件或 snapshot 重建。

## 5. 服务和模块契约

### 5.1 官方逻辑模块

逻辑模块建议固定为：

| 模块 | 权威数据/职责 | 默认实现 |
| --- | --- | --- |
| `network-runtime` | 实时 session、callsign、position、flight plan、routing、handoff | Gateway 进程内 |
| `protocol-suite` | wire framing、dialect decode/encode、handshake | Gateway 进程内 |
| `identity` | account、credential、rating、suspension、membership | Embedded 或 gRPC |
| `network-control` | 网络配置、策略、管理员 command、audit | Embedded 或 gRPC |
| `dispatch` | Dispatch/协调任务及其生命周期 | Embedded 或 gRPC |
| `navdata` | AIRAC、航路、机场和版本发布 | Embedded 或 gRPC |
| `weather` | Weather source、station 数据和缓存 | Embedded/provider 或 gRPC |
| `history` | durable event、track、session 和 replay 数据 | Embedded 或独立 service |
| `map-projection` | 实时网络读模型和 streaming API | Embedded 或独立 service |
| `web-control-plane` | Web API、认证入口、admin UI、WebSocket/SSE | 独立 Web service |

这些模块的数量可以随部署 Profile 变化，但所有权和 contract 不变。

### 5.2 Local、gRPC 和自定义实现

每个可替换服务至少提供三层：

```text
stable contract
├── Embedded/Local implementation
├── Grpc client/server implementation
└── Custom Rust adapter
```

建议的命名是 `embedded`/`grpc`，不要使用含义不清的 `internal`/`out`。这里的 mode 描述服务位置，不描述权限或数据可信度。

示例：

```toml
[identity]
mode = "embedded" # embedded | grpc

[identity.grpc]
endpoint = "https://identity.internal:9443"
authority = "asterfsd-gateway"
timeout_ms = 1500
```

Embedded 实现也必须通过 `IdentityService` port 访问 repository，Gateway 不得绕过 port 直接访问 Identity 表。运营方可以实现同一 Rust contract 并重新编译，不需要运行时动态加载 ABI，也不要求 WASM。

### 5.3 合约分层

```text
crates/aster_fsd_model
  Gateway 内部网络领域模型

crates/aster_fsd_contract_*
  服务间稳定语义和版本化 DTO

crates/aster_fsd_proto
  protobuf/gRPC schema、生成代码、版本兼容检查

crates/aster_fsd_protocol_*
  wire adapter，不访问服务数据库
```

不要把所有服务内部 entity 放入一个万能 shared model crate。跨服务只共享稳定 ID、枚举语义和 versioned contract；服务内部可以独立演进。

## 6. Identity 架构

### 6.1 单一账号权威

账号权威永远属于 Identity：

```text
Account
├── account_id
├── web credentials
├── network app passwords
├── roles
├── ATC rating
├── Pilot rating
├── suspension
├── network memberships
└── authorized clients
```

Web 注册调用 Identity API；Classic/VATSIM 登录由 Gateway 调用 `AuthenticateNetwork`；Aster 协议可以使用短期 token、session ticket 或 OIDC 结果。网络客户端不应复用 Web 主密码。

### 6.2 同步登录语义

登录是 Gateway 唯一允许同步依赖 Identity 的热路径操作之一：

```text
protocol login
  -> decode credentials
  -> bounded Identity call
  -> AuthenticatedPrincipal
  -> Network admission
  -> password discarded
```

约束：

- password 只存在于 decode -> authenticate 最短链路；
- password 不进入 command/event、snapshot、presence、peer wire 或日志；
- Identity 错误对外收敛为不可枚举的认证失败；
- gRPC deadline 到期时不自动把未知结果当成功；
- 已建立 session 不因一次 Identity 查询抖动而被批量阻塞；
- rating/suspension 变更通过控制事件或有界复检更新活跃 principal；
- Identity 不可用时，新登录失败，已有连接按明确 policy 继续或被撤销。

### 6.3 数据库边界

Standalone 可以共用一个物理 SQLite 文件，但必须保持逻辑 schema ownership：

- Identity 只维护自己的 migration/entity/repository；
- Network Runtime 不查询 Identity 表；
- History 不 join Identity 的 credential 表；
- Web 不直接连接任意产品数据库；
- 分布式部署默认按服务使用独立 database/role/schema；
- 跨服务需要通过 contract 或 event 获取数据。

## 7. 实时状态、事件和一致性

### 7.1 权威状态

Network Runtime 是当前连接状态的单一写入者：

```text
Network shard
├── sessions: ConnectionId -> Session
├── callsigns: NormalizedCallsign -> ConnectionId
├── sequence: monotonic per-shard sequence
└── state snapshot: optional recovery checkpoint
```

一个 session/callsign 注册、登录、logoff 或断开必须在同一写边界维护两个索引。多 Gateway 部署必须显式分片并保持连接 affinity，不能用共享 Redis 写锁伪造 ownership。

### 7.2 Durable domain event

以下变化属于 durable domain event：

```text
SessionStarted
SessionAuthenticated
SessionEnded
FlightPlanFiled
FlightPlanAmended
HandoffInitiated
HandoffAccepted
HandoffRejected
ActivityJoined
ActivityLeft
RatingChanged
AccountSuspended
AiracActivated
```

事件必须包含：

- `event_id`；
- `event_type` 和 schema version；
- `occurred_at` 与 source sequence；
- `network_id`、tenant/deployment context；
- actor/source/causation/correlation id（适用时）；
- redacted payload；
- producer version。

消费者必须按 at-least-once 交付设计，使用 `event_id` 或业务幂等键去重。绝不能把“消息只投递一次”当作系统正确性的前提。

### 7.3 Realtime telemetry

以下数据属于 realtime telemetry：

```text
PositionUpdated
TransponderChanged
FrequencyChanged
VisibilityChanged
```

Telemetry 可以合并、采样和降级：

- 同一 aircraft 的旧 position 可被最新 position 覆盖；
- stream 背压时不得阻塞 Gateway packet dispatch；
- 每个 shard/aircraft 带单调 sequence，消费者可检测 gap；
- History 负责 retention、downsampling 和 track compaction；
- Live Map 只保留当前 projection，不承担完整历史保存。

### 7.4 投递和失败

事件传输可以有本地 channel、gRPC stream 或独立 transport adapter，但 contract 不绑定具体 broker。服务必须区分：

- `accepted`：producer 已接收并分配 event id；
- `persisted`：事件已进入可靠存储；
- `projected`：读模型已应用；
- `published`：实时消费者已收到。

不允许用一个模糊的 `Ok(())` 同时表达这四种状态。

对于必须可靠的 control event，可使用 outbox/spool；对于高频 telemetry，允许合并和有限丢弃。outbox 必须有容量、滞留时间、重试和告警上限。

## 8. History、Replay 和 Live Map

### 8.1 数据分层

```text
NetworkSnapshotStore
  当前状态 checkpoint，用于故障恢复/重建

HistoryStore
  session、flight、handoff、position track、activity 历史

LiveMapProjection
  当前 aircraft/controller 的快速查询模型

AnalyticsStore
  统计、聚合、报表和长期压缩数据
```

Snapshot 不是 History。Snapshot 用于恢复当前运行状态，History 用于查询过去发生过什么；两者不能因为“都保存 JSON”而共用一个无版本 blob 表。

### 8.2 Web 查询边界

```text
Browser
  -> Web Control Plane REST/WebSocket/SSE
  -> gRPC Identity/History/Map/Control
```

浏览器不直接依赖内部 gRPC proto。Web BFF 负责公开 API 的鉴权、分页、错误映射、rate limit 和版本稳定性。

### 8.3 Retention 和隐私

History contract 必须定义：

- 实时轨迹、压缩轨迹和统计的 retention；
- account/callsign 显示策略；
- operator、pilot、ATC 和游客的读取权限；
- 导出、删除、匿名化和审计；
- 时区、时间精度和 replay 的一致性；
- schema version 和重放兼容策略。

默认不把 password、token、内部 service credential 或完整认证 payload 写入任何 history/event store。

## 9. 协议和 API 版本治理

### 9.1 Wire backend

现有协议边界继续成立：

```text
bounded frame codec
  -> dialect decoder
  -> protocol-independent command
  -> network runtime
  -> typed event/effect
  -> recipient dialect encoder
```

Classic、VATSIM、Aster listener 是并列 adapter，不是三个独立的网络服务器。新增协议只能实现 `ProtocolBackend`，不得复制 registry、flight plan、position 或 routing。

### 9.2 gRPC contract

每个 proto package 必须包含：

- 明确的 package/version；
- request/response 的错误模型；
- field compatibility 规则；
- deadline 和 retry 语义；
- idempotency 说明；
- authn/authz 要求；
- resource name 和分页规则；
- server health 和 readiness；
- contract test fixture；
- generated code 的来源和 drift gate。

破坏性字段删除、enum 重编号、错误语义改变和 resource identity 改变必须升级 major contract version。新字段优先采用向后兼容的 additive change。

### 9.3 Capability negotiation

协议能力和服务能力必须分开：

- protocol capability：wire backend 支持什么；
- network capability：core 当前允许什么；
- service capability：Identity/Dispatch/Weather provider 提供什么；
- deployment capability：当前 distribution 是否装配该模块。

不能因为某个 backend 不认识一个 capability，就在 core 中硬编码另一套业务路径。

## 10. 部署 Profile

### 10.1 Standalone

目标是几个人快速开服连飞：

```text
asterfsd
├── protocol gateway
├── network runtime
├── embedded identity
├── embedded history projection
└── SQLite
```

约束：

- 一个 binary 可以不依赖 broker；
- 默认只启用 classic listener；
- 本地服务仍通过 port/contract 装配；
- background projection 使用 bounded channel；
- storage 和 spool 有容量上限；
- config example 能直接生成可运行配置；
- SQLite 测试和 runtime smoke 不污染正式数据库。

### 10.2 Distributed

目标是同一台机器或小型集群上的可选拆分：

```text
Gateway -> Identity gRPC
        -> History ingest
        -> Map projection
        -> Control/Dispatch
```

默认使用 PostgreSQL 或专用历史存储。每个服务使用专属 role/credential；数据服务保持内部网络暴露，不直接公开到公网。

### 10.3 Kubernetes

目标是多副本、独立扩缩容和可验证发布：

- Gateway 按 network/shard 和连接 affinity 扩展；
- Identity、History、Map、Web 独立部署；
- service-to-service 使用 mTLS 或等价服务身份；
- liveness、readiness、startup 和 drain 分离；
- 发布使用 immutable image digest；
- migration 使用独立 owner/job，不由普通 API Pod 隐式执行；
- Helm 只负责结构化 desired state；
- image updater 只更新明确标记的无状态 workload；
- Secret 不进入 image、日志、annotation 或普通 CI 输出；
- Service、EndpointSlice、NetworkPolicy、ingress route 和当前 Pod `imageID` 都要验证；
- rollback 优先使用已知良好 commit 构建的新 immutable tag，而不是依赖可变 `latest`。

Kubernetes YAML 渲染成功不等于部署成功。验收必须沿 desired state -> controller -> Pod -> image -> Service/EndpointSlice -> route -> health 完整验证。

## 11. 服务调用和可靠性规则

### 11.1 远程调用

所有 gRPC/HTTP 调用必须明确：

- deadline；
- retryable 与 non-retryable error；
- retry budget 和 backoff；
- idempotency key；
- circuit breaker 或 bulkhead；
- cancellation 传播；
- response size/frame limit；
- auth context；
- correlation/trace context；
- unknown commit outcome 的处理。

重试不是可靠性本身。对于 at-least-once command，服务端必须用幂等键、版本号、CAS 或 fencing token 防止旧请求覆盖新状态。

### 11.2 任务和异步工作

Dispatch、History ingest、projection rebuild 等后台任务遵循：

- lease/heartbeat；
- processing token fencing；
- 可重入 handler；
- 明确 checkpoint；
- stale worker 防写；
- retry/backoff；
- terminal state 幂等；
- backlog、lease age 和 dead-letter 指标。

用户 API 返回领域结果，不泄露 task polling、内部 lease 或 worker state，除非该操作本身就是长期任务。

### 11.3 崩溃恢复

可靠性测试必须由进程外 controller 驱动：

1. 等待被测进程报告命名 checkpoint；
2. 使用确定性 seed 和同一 DB/storage 恢复上下文；
3. 发送 `SIGKILL` 或等效真实进程终止；
4. 重启同一版本或明确兼容版本；
5. 验证数据库、对象、事件、quota、lease、fencing、cleanup 和 audit 不变量；
6. 对缺失、重复或跳过 checkpoint 直接判失败。

普通 unit fault hook、timeout 猜测和“HTTP 返回成功”不构成 crash recovery 证据。

## 12. 安全边界

### 12.1 身份和服务权限

- 用户身份和服务身份分离；
- Web token 不作为内部服务 credential；
- Gateway 只拥有调用 Identity/Control 的最小权限；
- History 只写入需要的数据，不读取 credential；
- database role 按服务分配；
- gRPC 使用 mTLS、service identity 或等效机制；
- operator command 必须记录 actor、reason 和 target scope。

### 12.2 Secret 和日志

- password、token、private key、database URL 中的 credential 永不进入日志；
- wire debug 只记录 command、source、destination、field count、wire bytes 和错误类别；
- SQLx statement logging 默认关闭，只有显式配置启用；
- Secret 只通过运行时 Secret provider 注入；
- CI 不拿 cluster-admin、生产 kubeconfig 或数据库明文密码；
- 生产诊断必须使用 redaction 和结构化字段。

### 12.3 网络隔离

ClusterIP 不是隔离。Kubernetes 部署必须：

- 验证 NetworkPolicy selector 对准实际 Running Pod；
- 验证允许的同 namespace/service-to-service probe；
- 验证拒绝的跨 namespace 或未授权 probe；
- 保持 PostgreSQL、Redis、event transport、History store 和 Console 为内部服务；
- 将公开 route 限制在 Web/API 或明确的 FSD listener。

## 13. 可观测性和运维

### 13.1 Tracing

使用 `tracing` 和 `aster_forge_logging` 作为日志基础，但 AsterFSD 自己定义字段语义：

- `network_id`、`tenant_id`、`connection_id`、`peer`；
- `dialect`、`command`、`direction`、`phase`；
- `source`、typed destination、field count、wire bytes；
- `event_id`、sequence、correlation id、causation id；
- `service`、`shard`、deployment version；
- `decode_elapsed`、queue latency、handler elapsed、write elapsed。

不能记录完整 login payload、password、token 或未脱敏 provider response。

### 13.2 指标

最少提供：

- active connections、authenticated sessions、callsign conflicts；
- decode errors、protocol errors、wire bytes、frame limit rejects；
- mailbox depth、queue wait、slow consumer、writer failure；
- command latency、effect delivery count、event publish lag；
- Identity latency/error/timeout；
- History backlog、projection lag、dropped/coalesced telemetry；
- shard ownership、reconnect、handoff success/failure；
- database pool、migration status、snapshot age；
- graceful drain duration 和 shutdown outcome。

### 13.3 健康检查

- liveness：进程仍能调度；
- readiness：可以接收新连接或明确处于 drain；
- startup：依赖初始化和 schema 状态完成；
- dependency health：单独展示 Identity、History、event transport 等依赖；
- health response 不泄漏凭据、内部拓扑或 SQL 错误。

健康检查通过不代表协议兼容、历史可查询或 rollout 成功，运维报告必须分开记录。

## 14. 测试和一致性验收

### 14.1 Contract test 层级

每个服务和 adapter 至少提供：

- request/response exact contract；
- unknown field、版本和错误映射；
- deadline、cancel、retry 和 idempotency；
- authn/authz positive/negative matrix；
- generated code drift 检查；
- local implementation 与 gRPC implementation 的相同语义测试。

### 14.2 Network Runtime

至少覆盖：

- 登录成功/失败、revision、rating、duplicate callsign、source spoof；
- password 不进入 event、presence、peer wire 和日志；
- direct、`*`、`*A`、`*P`、range routing；
- position/flight plan 先改权威状态再生成 effect；
- handoff 正常、拒绝、重复、超时、断线和抢占；
- EOF、writer failure、idle timeout、slow mailbox、shutdown；
- 两个以上 TCP client 的跨 dialect delivery；
- 7500 等普通 transponder code 不触发错误的断开语义。

### 14.3 History/Projection

至少覆盖：

- durable event 重复消费；
- event gap、乱序和 replay；
- snapshot 与 event 重建；
- telemetry 合并、降采样和背压；
- History/Map 故障时 Gateway 继续处理连接；
- projection rebuild 后结果一致；
- retention、匿名化、删除和权限；
- unknown commit outcome、outbox 重试和 stale token 防写。

### 14.4 外部和真实客户端

协议变化必须继续使用：

- `tmp/fsd-master` 和 `AptS-1547/fsd-doc` 作为 Classic 行为证据；
- golden wire fixture corpus；
- property/fuzz tests；
- Swift、Pilot、ATC、VATSIM client 的真实 frame；
- SQLite 最低运行验证，必要时扩大 PostgreSQL/MySQL；
- TCP 黑盒、负载、soak、allocation 和 slow consumer 测试。

“本地 unit test 通过”不能替代真实客户端、部署和数据恢复证据。

## 15. 热路径和内存约束

所有实现必须优先保护 Network Data Plane：

- decoder 在继续分配前执行 frame 上限；
- mailbox 有界，慢消费者不会无限积压；
- `Event`/`Effect` 尽量 inline 存储；
- recipient fan-out 复用 `Bytes`/`Arc<[WireFrame]>` 等只读编码缓存；
- 同一 dialect 的 event 只编码一次；
- position 不同步写数据库、不等待 History ack、不调用 Identity；
- 登录的 Argon2 blocking work 与 packet dispatch 隔离并有并发上限；
- 不以 `Box<dyn Trait>`、全局锁或泛型擦除逃避所有权设计；
- 只有测量证明的冷路径优化才允许引入额外分配或缓存层。

所有优化必须同时报告吞吐、延迟、分配次数、RSS、队列深度和正确性影响。不能只报告 benchmark 的单个平均值。

## 16. Workspace 和目录建议

当前 workspace 继续作为 monorepo，按所有权逐步整理为：

```text
asterfsd/
├── src/                         # composition root、配置、日志和 binary
├── crates/
│   ├── aster_fsd_model/         # Gateway 内部领域模型
│   ├── aster_fsd_codec/         # bounded frame/raw tokenization
│   ├── aster_fsd_protocol/      # backend trait
│   ├── aster_fsd_protocol_*/    # concrete dialect adapters
│   ├── aster_fsd_contract_*/    # service contracts
│   ├── aster_fsd_proto/         # protobuf/gRPC schema and generated code
│   ├── aster_fsd_auth/          # auth port/password primitives
│   ├── aster_fsd_persistence/   # repository adapters
│   ├── aster_fsd_migration/     # append-only migration history
│   ├── aster_fsd_core/          # Network Runtime
│   └── aster_fsd_server/        # listener/supervisor/mailbox
├── services/
│   ├── identity/
│   ├── history/
│   ├── map/
│   ├── control/
│   └── web/
├── proto/                       # source proto or generated-contract manifest
├── deploy/
│   ├── standalone/
│   ├── docker/
│   └── kubernetes/
├── conformance/
│   ├── classic/
│   ├── vatsim/
│   ├── aster/
│   └── service-contracts/
└── developer-docs/zh-CN/
    └── architecture/rfcs/
```

服务目录不代表必须立刻拆成独立仓库或 Pod。它表示未来可部署边界；Standalone 仍可由 root composition 向进程内注册 local implementation。

根 `Cargo.toml` 继续是 workspace package metadata、依赖版本、lint、profile 和 feature governance 的唯一事实源。所有 direct dependency 必须有真实调用或清楚的 feature-unification 边界；升级后必须检查 feature tree、API、MSRV、全量测试和 `cargo machete`。

## 17. 发布、迁移和版本兼容

### 17.1 Schema ownership

每个服务拥有自己的 migration history。禁止跨服务 migration 修改另一个服务的表。破坏性数据迁移必须：

- 明确 owner 和兼容窗口；
- 先 expand，再切换读写，再 contract；
- 提供 forward rollback 方案；
- 在同一版本矩阵中验证旧/新 reader 和 writer；
- 在临时数据库和真实方言上验证；
- 对不可逆删除先做 emptiness/data-level backup 检查。

### 17.2 Artifact 和 rollout

- 发布 artifact 使用 immutable version/digest；
- SBOM、provenance 和签名属于发布证据；
- CI 只验证和发布，不直接修改共享生产 namespace；
- migration、Helm reconciliation、Secret、routing 和 image update ownership 分开；
- rollout 证明必须包含当前 revision、Pod `imageID`、Service/EndpointSlice、route、health 和 migration outcome；
- 不能把 workflow “passed”、Helm render 或 Flux applied revision 单独当作版本已上线。

### 17.3 破坏性变更

本项目允许内部破坏性重构，但必须在一个完整变更中更新：

- trait、实现和全部调用方；
- proto、generated code、测试和 fixtures；
- migration、entity、repository 和配置；
- README、config example、developer docs 和 changelog；
- conformance matrix 和升级/回滚说明。

不保留只做改名、转发或 re-export 的旧内部 facade。

## 18. 决策和禁止项

### 已决定

1. AsterFSD 是独立产品组，Forge 是工程基础设施。
2. Gateway 是实时网络状态的权威写入者。
3. Identity 是账号、credential、rating 和 suspension 的唯一权威。
4. History 不同步阻塞 Gateway position 热路径。
5. Standalone 与 Kubernetes 使用同一套 contracts 和 local/remote adapters。
6. gRPC 用于同步 command/query 和受控 streaming；高频 telemetry 使用独立 realtime/event adapter。
7. 默认不采用 WASM 作为 Identity、数据库或 Dispatch 的主插件机制；运营方通过 Rust crate 重新编译。
8. 不共享数据库表，不使用分布式事务，不以共享全局状态代替明确 ownership。
9. 不以 `Box`、无限队列、全局锁或可变 singleton 解决架构问题。

### 明确禁止

- Web 直接读写 Gateway/Identity/History 的数据库；
- Gateway 直接 join Identity 或 History 表；
- protocol backend 自己维护业务 registry；
- 每个 position command 同步调用远程服务；
- 使用 `latest` 或可变 tag 作为发布证明；
- 仅以 YAML 渲染、HTTP 200 或单条 unit test 宣布服务完成；
- 把 at-least-once 当作 exactly-once；
- 把任务 lease、polling 和 worker 状态泄露成用户领域 API；
- 为了“微服务”把每个 crate 都拆成网络服务；
- 把未经当前 checkout 验证的历史记忆当作当前实现事实。

## 19. 实施约束

本 RFC 不把产品拆成“先做一个简化阶段、以后再重写”的路线。后续实现可以按风险分批编译和验证，但最终 checkout 必须收敛到本 RFC 的 ownership 和 contract：

- 新增模块必须先声明它的权威数据和依赖方向；
- 远程实现必须先有与 local 实现相同的 contract test；
- 新服务不得先创建共享数据库表再补服务边界；
- 新协议不得复制 core；
- 新历史/地图能力不得进入 Gateway 的同步热路径；
- 新部署资源必须同时提供离线验证和 live rollout 证据；
- 每个故障语义必须有可复现测试入口和可观测指标；
- 任何临时兼容层都必须写明对象、期限、测试和删除条件。

## 20. 验收标准

本 RFC 对后续架构实现的验收不是“服务能启动”，而是同时满足：

1. 同一网络内 Classic、VATSIM、Aster 客户端共享同一权威 session/callsign/position/flight-plan state。
2. Standalone 不依赖外部 broker；切换到 gRPC Identity/History 不改变 Network Runtime 语义。
3. Identity、History、Map 故障边界分别可测试，且不会把非核心故障扩散到 Gateway 热路径。
4. durable event 可重放、可幂等；telemetry 可合并、可降采样并能检测 gap。
5. 数据库所有权、migration owner、credential scope 和 service auth 可审计。
6. Gateway 的 frame、mailbox、recipient fan-out 和 remote call 均有 bounded allocation/backpressure 证据。
7. 真实 Swift/ATC/Pilot/VATSIM 客户端和 C FSD fixture 通过兼容性矩阵。
8. K8s 验收沿 desired state、controller、Pod imageID、Service/EndpointSlice、route、health 和 rollback 证据完成。
9. 崩溃恢复由外部 controller 触发命名 checkpoint，验证幂等、fencing、cleanup、replay 和审计不变量。
10. RFC 约束、Cargo governance、config example、README、测试和 changelog 保持同步。

## 21. 后续 ADR 主题

本 RFC 固定平台总边界，以下主题应各自形成 ADR 或子 RFC，不在实现中用隐式约定解决：

- `NetworkId`、tenant 和 membership 的完整数据模型；
- [RFC-0004](0004-network-runtime-sharding-and-high-availability.md) 固定的 Gateway shard、跨 shard routing、epoch fencing、drain 和重连语义；
- Identity gRPC v1 proto、credential lifecycle 和 token/session ticket；
- [RFC-0005](0005-event-model-and-delivery-semantics.md) 固定的 durable event envelope、outbox/inbox、realtime transport、幂等、顺序和 replay 语义；
- [RFC-0006](0006-history-replay-and-telemetry-architecture.md) 固定的 History retention、TrackChunk、track compression、archive 和 replay consistency；
- handoff/ATC coordination 状态机和 provider ownership；
- [RFC-0007](0007-activity-and-dispatch-integration.md) 固定的 Activity、Dispatch、policy projection、release revision 和 Network filing contract；
- [RFC-0008](0008-weather-airac-and-route-data-plane.md) 固定的 Weather、AIRAC/NavData、Route、provider sync 和 atomic activation contract；
- Kubernetes topology、PDB、HPA、drain、migration job 和 rollback rehearsal；
- public Web API、WebSocket/SSE 和权限矩阵；
- conformance runner、真实客户端测试和 allocation benchmark 规范。

这些 ADR 只能细化本 RFC，不能重新引入共享数据库、协议复制 core、同步阻塞 telemetry 或双重账号权威。
