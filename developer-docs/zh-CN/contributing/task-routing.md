# AsterFSD 开发任务路由

| 任务 | 先读 | 代码入口 | 最低验证 |
| --- | --- | --- | --- |
| 强类型领域模型 | 项目契约、架构概览 | `crates/aster_fsd_model` | model unit tests、workspace check |
| framing / raw FSD packet | 协议后端契约、C `user.cpp` | `crates/aster_fsd_codec` | CR/LF/CRLF、上限、非法 UTF-8、round trip |
| classic command | C `cluser.cpp` / `clinterface.cpp` / `protocol.h` | `crates/aster_fsd_protocol_classic` | exact wire、C fixture、direction |
| VATSIM command | VATSIM 证据与真实客户端 frame | `crates/aster_fsd_protocol_vatsim` | `$DI/$ID`、扩展 query、真实客户端 |
| Aster 原生协议 | protocol backend 文档 | `crates/aster_fsd_protocol_aster` | version/JSON/schema/unknown field |
| 登录、账号、Rating、权限与 credential | RFC-0003、认证 contract、schema | `crates/aster_fsd_auth`、`aster_fsd_persistence`、Identity service/adapter | 成功/失败、network scope、blocking boundary、revoke、无泄密 |
| Identity/gRPC/事件/History/平台部署 | RFC-0001、RFC-0002、RFC-0003、RFC-0004、RFC-0005 | service contract、adapter、composition root 或 `deploy/` | local/remote parity、幂等/背压/故障隔离、Profile、shard、drain 验证 |
| Network Runtime、shard、callsign ownership、跨 shard routing、HA | RFC-0001、RFC-0004 | `crates/aster_fsd_core`、`aster_fsd_server`、Directory adapter、`deploy/` | claim/epoch/fencing、direct/audience/range、断线重连、snapshot、drain |
| Command、事件、NATS、outbox、projection、replay | RFC-0002、RFC-0004、RFC-0005、RFC-0006 | `aster_fsd_model`、event transport adapter、service outbox/inbox、History/Projection | schema compatibility、duplicate/order/gap、backpressure、replay、allocation |
| History、TrackChunk、telemetry ingest、轨迹回放、archive | RFC-0001、RFC-0002、RFC-0005、RFC-0006 | `aster_fsd_history`、History ingest/query、SQLite/PostgreSQL/ClickHouse/S3 adapter | sampling、checksum、watermark、gap、retention、cursor、replay、Network isolation |
| Activity、报名、slot、assignment、Dispatch、Network filing | RFC-0003、RFC-0005、RFC-0006、RFC-0007 | `aster_fsd_activity`、`aster_fsd_dispatch`、policy projection、Network Control adapter | state machine、并发 hold、policy lease、revision、idempotency、real client |
| Weather、AIRAC、NavData、Route、provider sync | RFC-0002、RFC-0007、RFC-0008 | `aster_fsd_weather`、`aster_fsd_navdata`、`aster_fsd_route`、provider/storage adapter | freshness、checksum、activation、overlay、pinned generation、exact wire |
| ATC jurisdiction、tracking、handoff、cross-shard coordination | RFC-0004、RFC-0005、RFC-0007、RFC-0009 | `aster_fsd_core` coordination、protocol backend、Directory/NATS adapter | state machine、version/fencing、disconnect、exact wire、real ATC/Pilot |
| client state / routing | 架构概览 | `crates/aster_fsd_core` | source/duplicate/direct/audience/range/lifecycle |
| TCP / mailbox / shutdown | 架构概览 | `crates/aster_fsd_server` | 多连接、backpressure、EOF、shutdown |
| 数据库 / migration | schema 与 entity | `crates/aster_fsd_persistence`、`aster_fsd_migration` | 临时 SQLite；按语义扩大 PG/MySQL |
| 配置 / 启动 / 日志 | config example | 根 `src/` | parse/default/invalid、runtime smoke |

协议任务默认沿以下链路阅读：

```text
backend decode
  -> model command
  -> core use case
  -> effect recipients
  -> backend encode
  -> TCP exact bytes
```

只改单侧 parser 或只加 processor string branch 都不构成完整协议实现。
