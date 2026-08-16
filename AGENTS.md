# AsterFSD

AsterFSD（根 crate 与二进制名均为 `asterfsd`）是多协议飞行模拟网络服务端。classic FSD、VATSIM 和 Aster 原生协议只负责 wire adapter；所有客户端共享同一个权威连接状态、callsign registry、position、flight plan、认证和路由内核。

## 开始工作

涉及代码、依赖、协议、数据或架构时依次读取：

1. `developer-docs/zh-CN/architecture/project-contract.md`
2. `developer-docs/zh-CN/architecture/index.md`
3. 协议任务再读 `developer-docs/zh-CN/architecture/protocol-backends.md`
4. `developer-docs/zh-CN/contributing/task-routing.md`
5. 当前 branch、HEAD、worktree、相关 crate 和相邻测试

用户负责 Git 时只读 status/diff，不修改 index、不提交、不重排现有改动。

## 事实来源

- 当前用户任务决定范围和优先级。
- 当前 checkout、测试和真实 TCP frame 决定已实现事实。
- 项目契约定义长期所有权与依赖方向。
- classic wire 行为以 `tmp/fsd-master`、`AptS-1547/fsd-doc` 和真实客户端交叉验证。
- VATSIM/Aster 行为必须由对应 backend contract 和真实 frame 证明，不能套用 classic 结论。
- 历史 MVP 文档、`target/`、`server.log` 和 `asterfsd.db` 不构成当前验证证据。

## 最终形态

- 根 `src/` 只保留配置和 composition root。
- `aster_fsd_model` 拥有统一 command/event/domain model。
- `aster_fsd_codec` 拥有 bounded framing 和 raw FSD tokenization。
- `aster_fsd_protocol*` 拥有 backend trait 与具体 dialect mapping。
- `aster_fsd_core` 是 session/callsign/position/flight-plan/routing 唯一权威。
- `aster_fsd_server` 负责 listener、connection supervisor、bounded mailbox 和 shutdown。
- `aster_fsd_auth` 定义认证 port 与密码 primitive；`aster_fsd_persistence` 实现 SeaORM adapter。
- `aster_fsd_migration` 拥有 append-only schema history。

内部重构一次迁移全部调用方，不保留旧单体 processor/handler、旧 crate 名或薄 re-export facade。

## 协议硬约束

- classic listener accept 后静默，客户端先发 `#AA/#AP`；VATSIM `$DI` 只出现在显式 VATSIM listener。
- decoder 在继续分配前实施 frame 上限；encoder 超限返回错误，禁止静默截断。
- password 只存在于 decode -> authenticate 最短链路，不进入 event、presence、peer wire、日志和快照。
- public `#AA/#AP` 必须重建并清空 password 字段。
- 所有 active command 在 core 校验 session phase 和 source ownership。
- callsign case-insensitive 唯一；session 与 callsign 两个索引原子注册和释放。
- direct、`*`、`*A`、`*P`、range 是 typed delivery，不用 magic socket address 或全局 broadcast 猜目标。
- 未登录连接不接收 presence、position、message、flight plan 或 wind delta。
- disconnect 是 direct control effect，不进入业务广播。
- position 与 flight plan 先更新权威 state，再按 recipient policy 产生 effect。
- `7500` 是普通 transponder code；产品告警不得伪装成 classic wire 断开规则。

## Crate 依赖方向

```text
model <- core <- server <- asterfsd
model + codec <- protocol <- concrete backends <- asterfsd
model <- auth <- persistence <- asterfsd
migration <- persistence <- asterfsd
```

- core 不依赖具体 protocol backend。
- backend 不访问数据库和全局 session registry。
- server 不实现 command 业务规则。
- repository 不构造 wire error。
- 新协议通过实现 `ProtocolBackend` 接入，不复制 core。

## Cargo.toml 治理

根 `Cargo.toml` 是 workspace package metadata、依赖版本、lint 和 profile 的唯一事实源。

- workspace 使用 resolver 3、edition 2024、MSRV 1.95.0。
- member 继承 package metadata 和 `[lints] workspace = true`。
- 外部版本与本地 path dependency 统一放 `[workspace.dependencies]`。
- direct dependency 必须有真实调用或清楚的 feature-unification 边界。
- 常规升级执行 `cargo upgrade && cargo update`；跨不兼容升级执行 `cargo upgrade --incompatible allow && cargo update`，然后检查 feature tree、API、MSRV 和全量测试。
- 不手工伪造 `Cargo.lock`。

## 认证、数据库与日志

- Argon2 使用 bounded blocking execution；一个连接的登录不得阻塞全局 packet dispatch。
- 认证失败保持对外不可枚举，classic error code 使用 `$ER006`。
- migration、entity、repository 和 core auth mapping 一起审查。
- SQLite 是最低运行验证；方言/索引/事务变化扩大到 PostgreSQL/MySQL。
- 测试使用临时数据库，不污染 `asterfsd.db`。
- 默认 logging filter 保持 `info`。
- 联调用 `RUST_LOG=info,asterfsd=debug,aster_fsd_server=debug,aster_fsd_core=debug`。
- SQLx statement logging 只由 `database.sqlx_logging` 和 `sqlx_logging_level` 控制，默认关闭。
- 日志记录 connection id、peer、dialect、command、方向、字段数、wire bytes 和错误类别，不记录完整登录 payload。

## 测试与完成标准

优先 focused，再扩大：

```bash
cargo test -p aster_fsd_codec
cargo test -p aster_fsd_protocol_classic
cargo test -p aster_fsd_core
cargo test -p aster_fsd_server
cargo check --workspace --all-targets --all-features
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

协议变化至少验证：

- exact wire、CR/LF/CRLF、上限、非法输入和 round trip；
- 登录成功/失败、revision/rating、duplicate/source spoof；
- 两个以上 TCP client 的 direct/audience/range routing；
- peer wire 中没有 password；
- logoff、EOF、writer error、idle timeout、slow mailbox 和 shutdown；
- classic/VATSIM/Aster backend 的独立 handshake；
- 真实 swift/ATC/Pilot client 作为补充兼容证据。

结束前同步 README、config example、developer docs、示例和 changelog，运行 `git diff --check`，准确报告实际验证和未运行矩阵。
