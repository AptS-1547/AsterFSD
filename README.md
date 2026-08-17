# AsterFSD

AsterFSD 是 Rust 实现的多协议飞行模拟网络服务端。classic FSD、VATSIM FSD 扩展和 Aster 原生协议通过独立 backend 接入同一套权威连接状态、callsign registry、position、flight plan、认证和路由内核。

> 当前版本是从 MVP 单体实现迁移后的 `0.2.0` 架构基线。协议兼容结论以 backend contract test、`tmp/fsd-master` 参考实现和真实客户端联调共同证明。

## 核心能力

- 有界 CR/LF/CRLF transport codec，在继续分配前执行 frame 上限；
- classic draft 9 client-speaks-first listener；
- 显式 VATSIM `$DI/$ID` listener；
- 版本化 Aster JSON v1 listener；
- protocol-independent command/event model；
- 原子 session/callsign registry；
- direct、`*`、`*A`、`*P` 和 range routing；
- per-client bounded mailbox 与 supervised connection lifecycle；
- SeaORM SQLite/PostgreSQL/MySQL 认证 adapter 与 migration；
- ATC/Pilot rating 强类型领域模型与稳定字符串存储；
- 异步 weather provider 与 C exact-wire `#TD/#WD/#CD` parsed profile；
- Argon2 bounded blocking verification；
- `aster_forge_logging` tracing、JSON/file/rotation 支持；
- 登录 public presence 重建，password 不进入 peer wire。

## Workspace

```text
asterfsd                         # composition root 与二进制
crates/
├── aster_fsd_model              # 统一 command/event/domain model
├── aster_fsd_codec              # bounded framing 与 raw FSD packet
├── aster_fsd_protocol           # backend trait
├── aster_fsd_protocol_classic   # classic draft 9 adapter
├── aster_fsd_protocol_vatsim    # VATSIM adapter
├── aster_fsd_protocol_aster     # Aster JSON v1 adapter
├── aster_fsd_auth               # 认证 port 与密码 primitive
├── aster_fsd_persistence        # SeaORM repository/auth adapter
├── aster_fsd_core               # 权威状态、路由和生命周期
├── aster_fsd_server             # listener/mailbox/supervisor
└── aster_fsd_migration          # schema history
```

完整边界见：

- [`developer-docs/zh-CN/architecture/project-contract.md`](developer-docs/zh-CN/architecture/project-contract.md)
- [`developer-docs/zh-CN/architecture/index.md`](developer-docs/zh-CN/architecture/index.md)
- [`developer-docs/zh-CN/architecture/platform-architecture-diagrams.md`](developer-docs/zh-CN/architecture/platform-architecture-diagrams.md)
- [`developer-docs/zh-CN/architecture/rfcs/0001-asterfsd-platform-architecture.md`](developer-docs/zh-CN/architecture/rfcs/0001-asterfsd-platform-architecture.md)
- [`developer-docs/zh-CN/architecture/rfcs/0002-technology-stack-and-infrastructure-profiles.md`](developer-docs/zh-CN/architecture/rfcs/0002-technology-stack-and-infrastructure-profiles.md)
- [`developer-docs/zh-CN/architecture/rfcs/0003-identity-and-trust-architecture.md`](developer-docs/zh-CN/architecture/rfcs/0003-identity-and-trust-architecture.md)
- [`developer-docs/zh-CN/architecture/rfcs/0004-network-runtime-sharding-and-high-availability.md`](developer-docs/zh-CN/architecture/rfcs/0004-network-runtime-sharding-and-high-availability.md)
- [`developer-docs/zh-CN/architecture/rfcs/0005-event-model-and-delivery-semantics.md`](developer-docs/zh-CN/architecture/rfcs/0005-event-model-and-delivery-semantics.md)
- [`developer-docs/zh-CN/architecture/rfcs/0006-history-replay-and-telemetry-architecture.md`](developer-docs/zh-CN/architecture/rfcs/0006-history-replay-and-telemetry-architecture.md)
- [`developer-docs/zh-CN/architecture/rfcs/0007-activity-and-dispatch-integration.md`](developer-docs/zh-CN/architecture/rfcs/0007-activity-and-dispatch-integration.md)
- [`developer-docs/zh-CN/architecture/rfcs/0008-weather-airac-and-route-data-plane.md`](developer-docs/zh-CN/architecture/rfcs/0008-weather-airac-and-route-data-plane.md)
- [`developer-docs/zh-CN/architecture/rfcs/0009-atc-coordination-and-handoff-state-machine.md`](developer-docs/zh-CN/architecture/rfcs/0009-atc-coordination-and-handoff-state-machine.md)
- [`developer-docs/zh-CN/architecture/protocol-backends.md`](developer-docs/zh-CN/architecture/protocol-backends.md)
- [`developer-docs/zh-CN/testing/protocol-compatibility.md`](developer-docs/zh-CN/testing/protocol-compatibility.md)

## 构建与验证

要求 Rust `1.95.0` 或更高版本。

```bash
cargo build --release
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo doc --workspace --all-features --no-deps
cargo machete
cargo +1.95.0 check --workspace --all-targets --all-features
```

依赖版本、workspace path dependency、Clippy 和 profile 统一由根 `Cargo.toml` 治理；member crate 继承 metadata/lint，只声明真实使用的 direct dependency。常规升级使用 `cargo upgrade && cargo update`，需要跨不兼容升级时使用 `cargo upgrade --incompatible allow && cargo update`，随后重跑上面的完整矩阵。

所有 crate 具有 crate-level rustdoc；公共 `Result` API 记录 `# Errors`。非测试 target deny `unwrap/expect/panic/todo/unimplemented/unreachable`，避免拿 lint allow 或热路径 `Box` 掩盖所有权和体积问题。

## 运行

```bash
cp config.example.toml config.toml
cargo run --release --bin asterfsd
```

默认只监听 classic FSD：

```text
0.0.0.0:6809
```

classic listener 建立 TCP 连接后保持静默，由客户端首先发送：

```text
#AA...:9\r\n
```

或：

```text
#AP...:9:...\r\n
```

VATSIM 和 Aster listener 在 `config.toml` 中显式启用，互相不污染握手行为。

## 配置

```toml
[server]
name = "AsterFSD"
version = "0.2.0"
product_message = "AsterFSD 0.2.0"
max_clients = 1000
mailbox_capacity = 256
wind_delta_interval_seconds = 70
motd = []

[[listeners]]
name = "classic"
protocol = "classic"
address = "0.0.0.0"
port = 6809
max_frame_bytes = 511
idle_timeout_seconds = 500

[logging]
level = "info"

[database]
url = "sqlite://asterfsd.db"
max_connections = 100
min_connections = 5
sqlx_logging = false
sqlx_logging_level = "debug"
```

应用协议联调：

```bash
# 直接使用 config.toml 中的 [logging] level = "debug"
cargo run --bin asterfsd

# 临时环境覆盖；RUST_LOG 优先于 config.toml，并在启动时输出覆盖告警
RUST_LOG=info,asterfsd=debug,aster_fsd_server=debug,aster_fsd_core=debug cargo run --bin asterfsd
```

SQL statement logging 继续由 `database.sqlx_logging` 独立控制；默认 `false`。

`level = "debug"` 会增加以下结构化打点：

- logging filter、输出格式和 `RUST_LOG` 覆盖状态；
- database backend、pool limits 和 migration lifecycle；
- TCP accept、connection ID、dialect、frame/timeout limits 和 handshake frame 数；
- inbound wire bytes、classic/VATSIM packet envelope、command/source/destination；
- core command execution、login/identification、effect/delivery/recipient 数；
- dialect encode cache、mailbox、close 和 disconnect。

packet envelope 日志只记录 prefix/command/source/destination/field count/wire bytes；raw frame、password、text payload、flight-plan payload 和 Aster JSON body 不进入日志。`database.sqlx_logging = true` 是 SQL statement debug 的唯一开关。

## 协议后端

### Classic

- 端口默认 `6809`；
- draft revision `9`；
- client-speaks-first；
- `$ER000..013`；
- public `#AA/#AP` password 字段为空；
- `$CQ` 支持 direct/audience/range，`$CR` 保持 direct-only，CAPS/ACC/ATIS/INF payload 原样 typed relay；
- `$HO/$HA/#SB/#PC/$C?/$CI` 只做 typed、无状态的 direct relay，不虚构 handoff ownership 状态机；
- `$CQ...:SERVER:FP:<callsign>` 返回 requester-directed `$FP`，其他 C server query 保持静默；
- `$!!` 按 C 顺序检查 target/rating、通知 requester、发送 typed disconnect 并释放 session；
- `#WX` parsed weather 通过统一 provider 定向编码为 `#TD/#WD/#CD`，`$AX/$AR` 处理 raw METAR；
- position/range、private message、flight plan、ping/pong 和 lifecycle 进入统一 core。

天气数据源通过 `aster_fsd_core::WeatherProvider` 注入；未配置数据源或找不到 station 时按 C contract 返回 `$ER009`，协议 backend 不自行访问外部天气服务。

### VATSIM

- 建议独立端口 `6810`；
- server-first `$DI`；
- `$ID` software identification 校验 `SERVER` destination、4 位十六进制 client ID、固定 `3:2`、非空 network ID 和 signed 9-digit unique number；
- 登录 revision 固定为 `100`，且 `$ID` callsign/network ID 必须与随后 `#AA/#AP` 一致；
- Pilot 登录后收到 CAPS、IP 与 `$ER008` flight-plan 状态，ATC 登录后收到 CAPS、ATC status 与 IP profile；
- VATSIM peer presence 使用 revision `100`，Pilot `#AP` 保留尾部 real-name 字段，不会回退成 classic draft `9` 结构；
- 其余 FSD command 通过 VATSIM backend 映射到同一 core。

### Aster v1

- 建议独立端口 `6811`；
- 单行、带 `v: 1` 的 tagged JSON；
- command/event 直接映射统一 model；
- password 只允许出现在 client login command。

## 示例

classic 示例客户端：

```bash
cargo run --example simple_client
```

交互客户端：

```bash
cargo run --example test_client
```

## License

MIT
