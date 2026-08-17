# AsterFSD Platform Architecture Diagrams

这份文档是 RFC-0001～RFC-0009 的视觉索引。图中的服务、队列和数据库代表 ownership 与数据流，不代表每个 Profile 必须拆成独立 Pod。

相关 RFC：

- [RFC-0001：Platform Architecture](rfcs/0001-asterfsd-platform-architecture.md)
- [RFC-0002：Technology Stack and Infrastructure Profiles](rfcs/0002-technology-stack-and-infrastructure-profiles.md)
- [RFC-0003：Identity and Trust Architecture](rfcs/0003-identity-and-trust-architecture.md)
- [RFC-0004：Network Runtime, Sharding and High Availability](rfcs/0004-network-runtime-sharding-and-high-availability.md)
- [RFC-0005：Event Model and Delivery Semantics](rfcs/0005-event-model-and-delivery-semantics.md)
- [RFC-0006：History, Replay and Telemetry Architecture](rfcs/0006-history-replay-and-telemetry-architecture.md)
- [RFC-0007：Activity and Dispatch Integration](rfcs/0007-activity-and-dispatch-integration.md)
- [RFC-0008：Weather, AIRAC and Route Data Plane](rfcs/0008-weather-airac-and-route-data-plane.md)
- [RFC-0009：ATC Coordination and Handoff State Machine](rfcs/0009-atc-coordination-and-handoff-state-machine.md)

## 1. 一张图看完整个平台

```mermaid
flowchart TB
    U[Swift / Pilot / ATC / Aster Client]
    WEB[Web / Console / Admin]

    subgraph Edge[Edge and API]
        GW[Gateway + Protocol Backends]
        API[Axum Control API]
        GRPC[Tonic Internal API]
    end

    subgraph Runtime[Network Data Plane]
        NR[Network Runtime]
        DIR[Network Directory]
        POL[Policy Projections]
    end

    subgraph Domain[Domain Services]
        ID[Identity]
        ACT[Activity]
        DSP[Dispatch]
        WX[Weather]
        NAV[NavData / AIRAC]
        ROUTE[Route Service]
        COORD[ATC Coordination]
    end

    subgraph Transport[Event and Data Transport]
        CN[Core NATS]
        JS[JetStream]
        OUT[Service Outbox / Inbox]
    end

    subgraph History[History and Projection Plane]
        HI[History Ingest]
        HQ[History Query]
        RP[Replay]
        CH[ClickHouse]
        PG[PostgreSQL / SQLite]
        S3[S3 / Parquet]
        MAP[Live Map]
    end

    U --> GW
    WEB --> API
    API --> GRPC
    GW --> NR
    GRPC --> ID
    GRPC --> ACT
    GRPC --> DSP
    GRPC --> WX
    GRPC --> NAV
    GRPC --> ROUTE
    NR --> DIR
    ACT --> POL
    POL --> NR
    NR --> COORD
    NR --> CN
    NR --> OUT
    OUT --> JS
    CN --> MAP
    JS --> HI
    HI --> PG
    HI --> CH
    HI --> S3
    HQ --> PG
    HQ --> CH
    RP --> CH
    RP --> S3
    API --> HQ
```

## 2. 服务 ownership 图

```mermaid
flowchart LR
    ID[Identity]
    NET[Network Runtime]
    ACT[Activity]
    DSP[Dispatch]
    WX[Weather]
    NAV[NavData]
    ROUTE[Route]
    HIST[History]

    ID -->|principal / membership| NET
    ACT -->|policy / assignment| NET
    DSP -->|file command| NET
    NAV --> ROUTE
    WX --> DSP
    ACT --> DSP
    NET -->|DomainEvent| HIST
    NET -->|TelemetrySegment| HIST
    ACT -->|DomainEvent| HIST
    DSP -->|DomainEvent| HIST

    ID:::authority
    NET:::authority
    ACT:::authority
    DSP:::authority
    WX:::authority
    NAV:::authority
    ROUTE:::authority
    HIST:::projection

    classDef authority fill:#d7ebff,stroke:#2774ae,color:#123;
    classDef projection fill:#e8f5e9,stroke:#388e3c,color:#123;
```

| 服务 | 权威内容 | 下游消费者 |
| --- | --- | --- |
| Identity | Account、Membership、Rating、Permission | Gateway、Activity、Dispatch、History |
| Network Runtime | session、callsign、position、在线 flight plan、live handoff | Gateway、Map、History |
| Activity | 活动、报名、slot、assignment、policy | Runtime、Dispatch、History |
| Dispatch | FlightRequest、Release revision、filing projection | Network Control、History |
| Weather | observation、forecast、freshness、provenance | Runtime、Dispatch、History |
| NavData | AIRAC cycle、bundle、overlay、generation | Route、Dispatch、History |
| Route | candidate、validation、engine result | Dispatch、Web |
| History | timeline、track、replay、archive、read model | Web、Activity、Dispatch、运营方 |

## 3. 三种部署 Profile

### 3.1 Standalone

```mermaid
flowchart TB
    P[Clients]
    subgraph APP[asterfsd process]
        L[Listeners / Protocol Backends]
        R[Network Runtime]
        I[Embedded Identity]
        AC[Embedded Activity]
        DP[Embedded Dispatch]
        H[Embedded History]
        T[Local Realtime / Telemetry]
        L --> R
        I --> R
        AC --> R
        DP --> R
        R --> T
        T --> H
    end

    P --> L
    H --> DB[(SQLite logical schemas)]
    T --> FS[(Local bundle / spool)]
```

Standalone 是完整架构的 `ShardId(0)`，不是功能阉割版。外部 NATS、PostgreSQL、ClickHouse 和 Kubernetes 属于可选基础设施。

### 3.2 Distributed Compact

```mermaid
flowchart LR
    C[Clients] --> G[Gateway Shards]
    G --> N[Core NATS]
    G --> JS[JetStream]
    G --> ID[Identity]
    G --> ACT[Activity + Dispatch]
    ACT --> PG[(PostgreSQL)]
    ID --> PGI[(Identity DB)]
    JS --> H[History]
    H --> PGH[(History DB)]
    H --> S3[S3 Archive]
```

### 3.3 Kubernetes Large

```mermaid
flowchart TB
    ING[Ingress / Load Balancer]
    ING --> GW[Gateway Shard Deployments]
    GW --> NATS[Core NATS Cluster]
    GW --> TS[Telemetry Stream]
    GW --> DIR[Directory / Lease Store]

    NATS --> MAP[Live Map Consumers]
    TS --> HI[History Ingest Workers]
    HI --> PG[(PostgreSQL HA)]
    HI --> CH[(ClickHouse Cluster)]
    HI --> S3[(Object Storage)]

    ID[Identity] --> PGI[(Identity PostgreSQL)]
    ACT[Activity] --> PGA[(Activity PostgreSQL)]
    DSP[Dispatch] --> PGD[(Dispatch PostgreSQL)]
    WX[Weather] --> OBJ[(Weather Cache / Object Store)]
    NAV[NavData Registry] --> OBJ
```

Kubernetes 只改变部署和故障域，Command/Event/Query/Telemetry contract 与 Standalone 保持一致。

## 4. 一次登录与在线状态

```mermaid
sequenceDiagram
    participant C as Client
    participant G as Gateway
    participant P as Protocol Backend
    participant I as Identity
    participant D as Directory
    participant R as Runtime
    participant H as History

    C->>G: TCP connect
    G->>P: decode login
    P->>I: Authenticate
    I-->>P: AuthenticatedPrincipal
    G->>D: Claim callsign
    D-->>G: Claim + ShardEpoch
    G->>R: Append SessionStarted journal
    R-->>G: Active session
    G-->>C: protocol welcome
    R->>H: SessionStarted event
```

密码、app password、session ticket 只存在于 decode → authenticate 最短链路。

## 5. 一次 position 更新

```mermaid
flowchart LR
    F[Client Position Frame]
    D[Decode + Validate]
    R[Runtime State]
    E[Local Effects]
    C[Realtime Coalescer]
    N[Core NATS]
    M[Live Map]
    T[Telemetry Recorder]
    TC[TrackChunk]
    JS[Telemetry JetStream]
    H[History]

    F --> D --> R
    R --> E
    R --> C --> N --> M
    R --> T --> TC --> JS --> H
```

position 热路径：

```text
decode -> validate -> local state -> local delivery -> bounded recorder offer
```

同步数据库、Identity、Directory、History 和 JetStream ack 均位于该路径之外。

## 6. Activity、Dispatch、Network filing

```mermaid
sequenceDiagram
    participant U as Web User
    participant A as Activity
    participant D as Dispatch
    participant W as Weather/NavData
    participant N as Network Control
    participant R as Runtime

    U->>A: Register / Confirm Slot
    A-->>U: Assignment + Policy Version
    U->>D: Create FlightRequest
    D->>W: Weather + AIRAC + Route
    W-->>D: Snapshot / Candidate
    D-->>U: DispatchRelease revision 3
    U->>N: FileDispatchRelease
    N->>R: FileFlightPlan expected version
    R-->>N: FlightPlanFiled version 12
    N-->>D: Filed event
```

```text
Activity assignment != Dispatch Release != Network FlightPlan
```

## 7. Weather、AIRAC 与 Route

```mermaid
flowchart TB
    W1[METAR / TAF Providers] --> W[Weather Service]
    N1[NavData Source] --> I[Import]
    I --> V[Validate]
    V --> S[Stage Generation]
    S --> A[Atomic Activation]
    A --> C[Local Immutable Cache]
    O[Network Overlay] --> R[Route Service]
    C --> R
    W --> D[Dispatch]
    R --> D
    D --> REL[Release pinned refs]
```

```text
DispatchRelease
├── WeatherSnapshotRef
├── AiracCycleRef
├── OverlayGeneration
├── RouteEngineVersion
└── RouteChecksum
```

## 8. Event transport

```mermaid
flowchart LR
    CMD[Command] --> OWN[Domain Owner]
    OWN --> EFF[Local Effect]
    OWN --> RT[RealtimeDelta]
    OWN --> EVT[DomainEvent]
    OWN --> TEL[TelemetrySegment]
    RT --> CN[Core NATS]
    EVT --> OUT[Durable Outbox]
    OUT --> JS[Domain JetStream]
    TEL --> TJS[Telemetry JetStream]
    CN --> LIVE[Live Consumers]
    JS --> PROJ[History / Activity / Audit]
    TJS --> TRACK[History Track Ingest]
```

| 类型 | 交付 | 失败处理 |
| --- | --- | --- |
| Command | gRPC/HTTP/local port | typed error、deadline、idempotency |
| Query | gRPC/HTTP/stream | cursor、limit、cancel |
| RealtimeDelta | Core NATS | gap、coalesce、snapshot |
| DomainEvent | outbox + JetStream | redelivery、inbox、version |
| TelemetrySegment | bounded recorder + telemetry stream | spool、sampling degradation、gap |
| Snapshot | object/DB/checkpoint | checksum、generation、rebuild |

## 9. ATC handoff

```mermaid
stateDiagram-v2
    [*] --> Offered
    Offered --> Accepted
    Offered --> Rejected
    Offered --> Cancelled
    Offered --> Expired
    Accepted --> Completed
    Accepted --> Cancelled
    Accepted --> Expired
```

```text
Classic $HO/$HA
  -> Classic backend mapping
  -> typed HandoffCommand
  -> Runtime coordination state
  -> direct/audience effect
  -> Classic exact wire
```

旧 ConnectionRef、旧 ShardEpoch、过期 HandoffVersion 都经过 fencing/version 校验。

## 10. History、Replay 与 Archive

```mermaid
flowchart LR
    EV[Domain Events] --> HI[History Ingest]
    TC[TrackChunk] --> HI
    HI --> PG[PostgreSQL / SQLite<br/>lifecycle + metadata]
    HI --> CH[ClickHouse<br/>tracks + analytics]
    HI --> S3[S3 + Parquet<br/>archive]
    PG --> Q[History Query]
    CH --> Q
    S3 --> RP[Replay / Export]
```

```text
ReplayManifest
├── event checkpoint
├── track watermark
├── snapshot refs
├── schema versions
├── storage generation
├── gap intervals
└── checksum
```

Replay 返回明确的 gap、watermark 和 consistency mode，不将展示插值伪装成 recorded sample。

## 11. 依赖方向

```text
aster_fsd_model
    ^
    +-- aster_fsd_codec
    +-- aster_fsd_protocol*
    +-- aster_fsd_auth
    +-- aster_fsd_activity
    +-- aster_fsd_dispatch
    +-- aster_fsd_weather
    +-- aster_fsd_navdata
    +-- aster_fsd_route
    +-- aster_fsd_history
            ^
            +-- persistence adapters
            +-- protocol/service adapters
            +-- composition root
```

硬边界：

- core 不依赖具体 protocol、Activity、Dispatch、Weather provider 或数据库；
- backend 不访问数据库和全局 registry；
- History 不写 Runtime current state；
- Activity/Dispatch 不直接写 Network Runtime；
- adapter 负责 transport、认证、错误映射和观测；
- 所有跨服务消息带 Network scope、version 和 authorization context。

## 12. 运行时故障边界

```mermaid
flowchart TB
    subgraph Hot[Packet Hot Path]
        P[Decode]
        R[Runtime State]
        W[Mailbox Writer]
        P --> R --> W
    end

    subgraph Async[Async Control / Data Plane]
        I[Identity]
        D[Directory]
        A[Activity Policy]
        DP[Dispatch]
        H[History]
        T[Telemetry]
    end

    R -. bounded control .-> I
    R -. claim/lease .-> D
    R -. local projection .-> A
    R -. command .-> DP
    R -. event/spool .-> H
    R -. non-blocking offer .-> T
```

依赖故障首先表现为对应领域的 lag、degraded、backpressure、stale 或 policy state；故障传播需要经过明确 admission/lease/queue 边界。

## 13. 相关规范

图册只提供结构索引，字段、错误、阈值和测试要求以 RFC 正文为准。新增服务需要回答：

1. 它拥有什么权威状态？
2. 它消费哪些 command/event/query？
3. 它的热路径在哪里？
4. 它的 durable boundary、outbox 和 replay source 是什么？
5. 它如何隔离 Network、Membership 和 service credential？
6. Standalone/Distributed/Kubernetes 如何保持同一 contract？
