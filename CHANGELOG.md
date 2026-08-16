# Changelog

本项目遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 与语义化版本。

## [Unreleased]

### Added

- **多协议 backend** — classic FSD draft 9、显式 VATSIM `$DI/$ID` 和版本化 Aster JSON v1 通过统一 `ProtocolBackend` 接入同一权威网络内核；VATSIM 固定 revision `100`，校验 identification 的 client ID/version/CID/unique number，并为 Pilot/ATC 输出 CAPS、IP、flight-plan/controller profile。
- **企业级 workspace 边界** — 新增 model、codec、protocol、具体 backend、auth、persistence、core、server 和 migration crates，并补齐项目契约、架构、任务路由和兼容测试文档。
- **真实多协议 TCP 覆盖** — 测试 classic 登录失败/重复登录、脱敏 presence、私聊、position/range、reset/logoff、慢 mailbox，以及 classic/Aster 跨协议互通和 VATSIM `$DI/$ID`、Pilot/ATC post-login exact wire、revision `100` presence 与 Pilot real-name 字段。
- **C FSD 天气 profile** — 新增异步 `WeatherProvider` port 与固定形状的统一 weather profile；classic `#WX` 命中后按原始 C 顺序定向输出 `#TD/#WD/#CD`，`$AX/$AR` 保留 raw METAR，缺失和 provider 失败统一映射 `$ER009`。
- **Typed classic command routing** — `$HO/$HA/#SB/#PC/$C?/$CI` 保持无状态 direct relay；`$CQ/$CR` 覆盖 CAPS/ACC/ATIS/INF、C source-range 与 direct flight-plan response；`$!!` 保持 target/rating 检查、通知、exact wire 和断开顺序。
- **公共 API 文档** — 所有 workspace crate 增加 crate-level rustdoc，核心公共 API 记录所有权、不变量、错误语义和 `must_use` 合同。

### Changed

- **Rating 强类型与 schema** — ATC/Pilot 权限改为独立领域 enum，数据库保存稳定语义字符串，wire 整数只在登录边界转换；pre-release `users` schema 直接压缩为最终形态，旧 MVP 数据库需重建。
- **Classic pilot position** — transponder 按原始 C FSD 的整数 wire contract 校验；`0`/`0000` 与 `0700` 均可输入，peer wire 分别规范化为 `0`/`0` 与 `700`，`7500` 仍是普通 position update。
- **运行名称统一** — 根 crate、library 和二进制统一为 `asterfsd`，默认服务名为 `AsterFSD`，默认 SQLite 文件为 `asterfsd.db`。
- **配置契约** — 单一 `[server]` bind 配置调整为 `[[listeners]]`，每个 listener 显式声明协议、地址、端口、frame 上限和 idle timeout；默认只启用 classic `6809`。
- **产品欢迎信息** — 登录后的首条产品信息由 `[server].product_message` 配置，默认使用当前 AsterFSD 版本标识，不再冒用原始 C FSD 的 Windows Beta 文案。
- **协议执行模型** — MVP 的全局 broadcast processor 调整为 bounded frame codec、统一 command/event、原子 session/callsign registry、typed delivery 和 per-client bounded mailbox。
- **热路径事件分发** — delivery event 保持 inline，不通过 box 隐藏体积；同一 event 按 dialect 编码一次并用 `Bytes`/`Arc<[WireFrame]>` 共享，避免按 recipient 克隆 event 和重复构造 frame。
- **严格工程门禁** — 对齐 AsterForge 的 pedantic Clippy 和生产 target 禁用 unwrap/expect/panic 等规则；拆分登录、active dispatch、classic encode 与 connection lifecycle 大函数，并清理所有未使用 direct dependency。
- **Classic direction contract** — 按原始 C `multiok` 行为区分 direct-only 与 multicast command，并用 exact command-direction tests 固定。
- **Classic 登录顺序** — 按原始 C 固定字段下限、callsign、revision、credential、suspension 和 requested-level 的错误码、environment 与 close 行为；非数字 revision 统一 `$ER010`。
- **依赖升级** — 执行 incompatible-aware `cargo upgrade` 与 `cargo update`，同步兼容版本并保持 Rust 1.95 MSRV。
- **Debug 可观测性** — 补齐 logging 初始化、数据库、TCP accept、handshake、frame envelope、protocol decode、core command/effect、routing、dialect cache、mailbox 和 disconnect 的结构化 tracing 打点。
- **Logging 边界测试** — 新增真实 `asterfsd` 子进程矩阵，固定 config debug/info、有效/非法 `RUST_LOG` 覆盖、SQLx opt-in、空 filter 拒绝以及 password/raw frame 永不落日志。

### Security

- **登录凭据隔离** — public `#AA/#AP` presence 由认证结果重建，password 字段始终为空；登录错误、welcome、private message 和 disconnect 都使用明确 recipient。
- **连接所有权** — active command 统一检查 session phase、source ownership、case-insensitive duplicate callsign 和 protocol revision/rating，异常断开与显式 logoff 走同一索引清理路径。
- **资源边界** — transport 在继续分配前拒绝超限 frame，Argon2 进入 blocking boundary，慢客户端 mailbox 和 writer I/O failure 只关闭对应连接。

### Removed

- 删除旧单体 packet/processor/handler/broadcast 路径及其失真的 MVP 完成文档和 Python 测试脚本。
