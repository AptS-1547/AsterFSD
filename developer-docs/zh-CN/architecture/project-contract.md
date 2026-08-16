# AsterFSD 项目契约

本文定义 AsterFSD 从 MVP 进入正式实现后长期成立的产品边界、依赖方向和完成标准。当前代码、测试与本契约冲突时，协议兼容任务应把实现收敛到本契约，而不是继续扩大旧单体路径。

## 产品身份

AsterFSD 是多协议飞行模拟网络服务端。它通过 classic FSD、VATSIM 兼容协议和 Aster 原生协议接入客户端，把各协议 frame 转换成统一的网络命令，再由同一套权威状态、路由、认证和生命周期内核处理。

```text
TCP listener / protocol backend
  -> bounded frame codec
  -> dialect decoder
  -> protocol-independent command
  -> AsterFSD core
  -> delivery effect
  -> recipient dialect encoder
  -> per-connection bounded mailbox
```

协议后端只负责 wire contract；核心层只负责产品语义。新增协议不得复制一套 client registry、flight plan、position 或 routing 状态。

## 最终形态优先

- MVP 的 `src/packet.rs -> processor -> handlers -> broadcast` 路径不作为长期兼容层保留。
- 内部重构一次更新全部调用方、测试、示例、配置和文档，不保留只做转发的旧 facade。
- 编译检查点属于内部风险控制，最终 checkout 只保留统一架构。
- classic、VATSIM、Aster 三种 listener 是同一运行时的并列 adapter，不是三个独立服务器实现。

## Crate 所有权

| Crate | 所有权 | 禁止承担 |
| --- | --- | --- |
| `aster_fsd_model` | Callsign、连接/客户端类型、ATC/Pilot rating、位置、flight plan、统一 command/event/error | TCP、数据库连接、具体 wire prefix |
| `aster_fsd_codec` | 有界 frame decoder/encoder、classic raw packet tokenization | 登录、路由、认证、数据库 |
| `aster_fsd_protocol` | dialect trait、连接握手上下文、decode/encode contract | 权威 client state、业务路由 |
| `aster_fsd_protocol_classic` | classic FSD draft 9 wire mapping | VATSIM greeting、产品认证策略 |
| `aster_fsd_protocol_vatsim` | VATSIM `$DI/$ID` 与 VATSIM 扩展 mapping | classic listener 的隐式行为 |
| `aster_fsd_protocol_aster` | 版本化 Aster JSON wire contract | 核心业务状态 |
| `aster_fsd_auth` | 密码哈希、认证 port、认证结果和结构化错误 | SeaORM 查询、TCP response |
| `aster_fsd_persistence` | SeaORM entity/repository、数据库认证 adapter | wire packet、路由策略 |
| `aster_fsd_core` | session registry、callsign ownership、状态机、路由、flight plan、position、weather provider port、lifecycle | TCP framing、dialect-specific strings、SQL |
| `aster_fsd_server` | listener、connection supervisor、mailbox、shutdown、core 与 backend 装配边界 | 协议业务规则、数据库查询 |
| `aster_fsd_migration` | append-only schema history | runtime 业务逻辑 |
| 根 `asterfsd` | 配置、日志、数据库、backend registry 和进程生命周期装配 | 领域规则和 parser |

依赖必须沿表格自上而下的 contract 方向，不允许具体协议 backend 被 core 反向引用。

## 统一数据处理层

`aster_fsd_core` 是所有协议的权威处理入口：

- 一个连接对应一个不可复用的 `ConnectionId`；socket address 仅是 transport metadata。
- callsign 通过强类型规范化，注册和释放在同一写边界完成。
- `Connected -> Identified -> Active -> Closed` 由 core 强制执行。
- 登录、重复 callsign、source ownership、logoff 和异常断开使用同一 registry invariant。
- position 与 flight plan 是权威 client state，不由 adapter 自行缓存。
- direct、`*`、`*A`、`*P` 和 range multicast 由 typed delivery 表达。
- protocol backend 只能产生统一 `Command`，不能直接向其他 socket 广播。
- core 只产生统一 `Event/Effect`，每个接收者再由自己的 dialect encoder 编码。
- weather source 通过异步 provider port 注入；core 负责 source ownership、请求形态和错误收敛，backend 只负责编码 profile。

## 协议兼容边界

### Classic FSD

- TCP accept 后保持静默，由客户端先发 `#AA/#AP`。
- revision 固定为 draft 9。
- public `#AA/#AP` presence 必须重建并清空 password 字段。
- `$HO/$HA/#SB/#PC/$C?/$CI/$CR` 只接受 direct callsign destination；`#TM/$PI/$PO/$CQ` 保留 C 实现允许的 multicast destination。
- `$HO/$HA/#SB/#PC/$C?/$CI` 与原始 C 一样是无状态 direct relay；目标不存在时静默，不添加 ATC/Pilot 角色或 handoff ownership 限制。
- `$CQ` 的 `@...` 使用 source range，不复用 `#TM` 的双边 message range；`$CR` 至少包含 kind 与一个 payload field。server flight-plan query 定向返回 requester，其他原始 C server query 静默。
- `$!!` 先检查 target、再检查 rating；成功路径先通知 requester，再向 typed target 发送 disconnect 并原子释放 session/callsign。
- `$ER000..013`、source ownership、direct/type/range routing 和 CRLF 输出按 `tmp/fsd-master` 固定测试。
- `#WX` parsed weather 固定为四层 temperature、四层 wind、两层 cloud 与一层 thunderstorm；命中后按 C 顺序定向输出 `#TD/#WD/#CD`，raw `$AX` 使用 `$AR`，缺失统一 `$ER009`。
- C 源码是行为证据，不复制它的内存管理、明文证书存储或不安全字符串操作。

### VATSIM

- 使用显式 VATSIM listener，server-first `$DI` 只出现在该 listener。
- `$ID` 必须完整校验 destination、4 位十六进制 client ID、固定 `3:2`、非空 network ID 和 signed 9-digit unique number；client ID 授权仍由认证 port 决定。
- 登录 revision 固定为 `100`；identification 与 login 的 callsign/network ID 必须同时一致，失败不得取得 callsign ownership。
- Pilot/ATC post-login CAPS、IP、flight-plan/controller profile 由 core 产生 typed event；VATSIM backend 负责编码 exact wire，并以 revision `100` 编码 peer presence，其中 Pilot `#AP` 保留 VATSIM 的尾部 real-name 字段。
- ACC、ATIS、INF 和 client CAPS payload 继续作为 typed client-to-client relay；没有领域 provider 的 server-side 数据不由 backend 伪造。
- VATSIM wire 变化不得改变 classic listener 的 first-frame 和 draft 9 合同。

### Aster 原生协议

- 使用显式版本字段，目前为 `v = 1`。
- frame 是单行 JSON，仍受 transport frame 上限和 mailbox/backpressure 约束。
- command/event 与 core model 一一映射；协议升级通过版本化 backend，不通过 core 中的 magic branch。

## 安全与资源不变量

- password 只存在于当前 decode/authenticate 调用链的最短生命周期，不进入 event、public presence、日志和持久 session。
- decoder 在继续分配前执行 frame 上限；encoder 超限返回结构化错误，禁止静默截断。
- 每连接 outbound mailbox 有界；慢客户端策略可观测并关闭该连接，禁止让全局状态静默缺包。
- hot-path delivery event 保持 inline；同一 event 按 dialect 编码一次并共享 immutable frame，不按 recipient box/clone event 或重复构造 wire buffer。
- disconnect 是 direct control effect，永远不进入全局业务广播。
- Argon2 在 bounded blocking execution 中运行；不同连接的认证不阻塞全局 packet dispatch。
- 所有 spawn task 都由 supervisor 持有并观察；shutdown 关闭 listener、连接和 mailbox。
- debug observability 只记录 dialect、frame bytes、command/event kind、source/destination、field/recipient count 和状态转换；raw frame、password、message/flight-plan payload、Aster JSON body 与数据库 URL 不进入日志。
- `logging.level` 控制应用 tracing filter；`RUST_LOG` 是显式运行时覆盖并必须产生启动告警。SQL statement logging 继续由 `database.sqlx_logging` 单独授权。

## 数据库与 migration

- `aster_fsd_persistence` 实现认证 repository；`aster_fsd_auth` 只定义 port 和密码规则。
- `AtcRating` 与 `PilotRating` 是两个不同的领域类型；数据库使用稳定语义字符串，协议整数只在 adapter/core 登录边界显式转换，禁止互换或共用裸整数范围比较。
- v0.2 schema 发布后 migration append-only，entity 与 migration 同步审查；当前 pre-release baseline 允许破坏性压缩，旧 MVP 数据库必须重建，不维护整数 rating 升级桥。
- SQLite 是最低运行验证；涉及方言、索引和事务时扩大 PostgreSQL/MySQL。
- 测试使用临时数据库，不污染仓库 `asterfsd.db`。

## Cargo、Lint 与 Rustdoc 治理

- 根 `Cargo.toml` 是 package metadata、外部依赖版本、workspace path dependency、lint 和 profile 的唯一事实源；member crate 只声明实际使用的 direct dependency 并继承 workspace 配置。
- workspace 使用 resolver 3、edition 2024、MSRV `1.95.0`；最低版本必须用真实 `cargo +1.95.0 check --workspace --all-targets --all-features` 验证。
- Clippy 默认启用完整 `pedantic`，并把 `allow_attributes`、无理由 allow、可能截断/符号丢失 cast、未记录 unsafe 等升级为 deny；CI 使用 `-D warnings` 收口。
- 非测试 target deny `unwrap`、`expect`、`panic`、`unreachable`、`unimplemented` 和 `todo`；测试可使用这些断言工具表达 fixture 失败。
- 每个 crate 必须有 `//!` 所有权说明；公共 API 的 rustdoc 解释业务不变量，返回 `Result` 的 API 提供 `# Errors`，纯查询 getter 按 Clippy 建议使用 `#[must_use]`。
- 常规依赖维护执行 `cargo upgrade && cargo update`；允许跨不兼容要求时执行 `cargo upgrade --incompatible allow && cargo update`，随后重跑 feature/MSRV/check/test/clippy/doc/machete 全矩阵。
- 禁止用 `allow` 掩盖 large enum/large future；热路径先拆 effect、借用共享 event、按 dialect 缓存 frame，再用测量证据决定是否引入间接分配。

## 完成标准

协议和运行时改动至少覆盖：

- exact wire parse/encode 与 round trip；
- frame 上限、CR/LF/CRLF、非法 UTF-8/JSON；
- 登录成功/失败、revision/rating、重复 callsign、source spoof；
- 两个以上真实 TCP client 的 direct/broadcast/audience routing；
- password 不出现在任何 peer wire；
- position/flight-plan authority 与查询；
- logoff、EOF、reset、慢消费者和 shutdown 清理；
- classic、VATSIM、Aster backend contract tests；
- workspace fmt/check/test/clippy 和最低 Rust 版本。
