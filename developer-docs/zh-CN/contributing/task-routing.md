# AsterFSD 开发任务路由

| 任务 | 先读 | 代码入口 | 最低验证 |
| --- | --- | --- | --- |
| 强类型领域模型 | 项目契约、架构概览 | `crates/aster_fsd_model` | model unit tests、workspace check |
| framing / raw FSD packet | 协议后端契约、C `user.cpp` | `crates/aster_fsd_codec` | CR/LF/CRLF、上限、非法 UTF-8、round trip |
| classic command | C `cluser.cpp` / `clinterface.cpp` / `protocol.h` | `crates/aster_fsd_protocol_classic` | exact wire、C fixture、direction |
| VATSIM command | VATSIM 证据与真实客户端 frame | `crates/aster_fsd_protocol_vatsim` | `$DI/$ID`、扩展 query、真实客户端 |
| Aster 原生协议 | protocol backend 文档 | `crates/aster_fsd_protocol_aster` | version/JSON/schema/unknown field |
| 登录与密码 | 认证 contract、schema | `crates/aster_fsd_auth`、`aster_fsd_persistence` | 成功/失败、blocking boundary、无泄密 |
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
