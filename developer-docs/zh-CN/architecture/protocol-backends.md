# 协议后端契约

## Backend API

每个协议实现同一 object-safe contract：

```rust
pub trait ProtocolBackend: Send + Sync {
    fn dialect(&self) -> ProtocolDialect;
    fn initial_frames(&self, context: &HandshakeContext) -> Result<Vec<WireFrame>, ProtocolError>;
    fn decode(&self, context: &DecodeContext, frame: &[u8]) -> Result<Command, ProtocolError>;
    fn encode(&self, context: &EncodeContext, event: &Event) -> Result<Vec<WireFrame>, ProtocolError>;
}
```

- `initial_frames` 只表达该 listener 的正式握手。classic 返回空；VATSIM 返回 `$DI`；Aster 返回 version hello。
- `decode` 只做 wire 到统一 command 的映射和协议字段形状验证。
- `encode` 只做统一 event 到 recipient wire 的映射。
- 登录是否成功、callsign 是否重复、消息发给谁都属于 core。

## Classic backend

Classic backend 以 `tmp/fsd-master/fsd` 为 draft 9 行为基线：

- `#AA/#AP` 登录；
- `#DA/#DP` logoff；
- `#TM` text；
- `@/%` position；
- pilot transponder 以整数 wire 值处理；`0` 与 `0000` 均是合法的 squawk 0000，最多四位八进制数字；
- `$PI/$PO` ping/pong；
- `$FP` flight plan；
- `$CQ/$CR` query/response；
- `$HO/$HA/#SB/#PC/$C?/$CI/$CR` direct-only direction；
- `$AX/$AR` METAR；
- `#WX` parsed weather request 与定向 `#TD/#WD/#CD` profile；
- `$ER` error；
- `#TM/$PI/$PO/$CQ` 的 `*`、`*A`、`*P`、`@...` multicast destination。

未映射命令返回 typed unsupported error；server 根据当前 session phase 决定发送 `$ER004` 或关闭连接。server-to-client only command 不进入 client command enum。

`$HO/$HA/#SB/#PC/$C?/$CI` 在统一 model 中使用 typed kind，但 core 行为仍是 C 的无状态 direct relay：只校验 active session、source ownership 和 direct callsign，目标不存在时没有 delivery，不建立额外 handoff 状态机。

`$CQ` direct/audience/range 请求与 `$CR` direct response 保留 typed query kind；CAPS/ACC/ATIS/INF 由目标客户端响应。`@...` query/ping 使用 C 的 source range，只有 `#TM` 使用 message range。`$CQ...:SERVER:FP:<callsign>` 由权威 flight-plan state 定向回答；其他 C server query 无输出。

`$AX` 只在首字段为 `METAR` 且存在 station 时查 raw weather；原始 C 会静默忽略的非 METAR/缺 station ACARS 形态映射成 source-validated no-op。syntax error 对外统一 C `$ER004::Syntax error`，详细 parser 原因只进入结构化日志。

parsed weather profile 是统一 model：四个 temperature layer、四个 wind layer、两个 cloud layer、一个 thunderstorm layer、barometer 和 visibility。classic encoder 必须按 C `clinterface.cpp::sendweather` 的 `TD -> WD -> CD` 顺序产生三帧；backend 不查询天气源。

## VATSIM backend

VATSIM backend 复用 FSD raw packet tokenizer，但独立拥有：

- server-first `$DI`；
- `$ID` client identification 的完整字段验证与 client authorization port；
- 固定 revision `100`，以及 identification/login callsign 与 network ID ownership；
- Pilot 的 CAPS/IP/no-flight-plan profile；
- ATC 的 CAPS/ATC status/IP profile；
- revision `100` 的 public `#AA/#AP` presence，Pilot `#AP` 包含 VATSIM 尾部 real-name 字段；
- CAPS / ACC / ATIS / INF client-to-client typed relay；
- 更大的显式 frame 上限。

server-originated query/response 的 source 是协议端点字符串，入站 command source 仍是强类型 callsign 并经过 core ownership 校验。backend 不伪造尚无领域 provider 的 ACC/ATIS/INF 数据，也不改变 classic backend 或把 `$DI` 注入 classic listener。

公开 `fsd-doc` 明确排除了 VATSIM `$ZC/$ZR` 的私有 hash 算法；当前 backend 不发送无法验证的 challenge。后续接入必须由独立、可替换的认证 port 和真实 frame fixture 证明，不把 hash 逻辑塞进 core 或硬编码占位响应。

## Aster v1 backend

Aster v1 使用 tagged JSON：

```json
{"v":1,"type":"login","callsign":"ECP4143","client_type":"pilot","network_id":"ECP1547","password":"...","requested_rating":1}
```

server event 示例：

```json
{"v":1,"type":"client_added","client":{"callsign":"ECP4143","client_type":"pilot","rating":1}}
```

密码只存在于 login command，永远没有对应的 output/event 字段。

## 新增 backend 的验收

新增协议必须同时提交：

1. dialect enum 与配置解析；
2. backend crate；
3. decode/encode contract tests；
4. handshake exact-wire test；
5. 与 core 的多协议 TCP integration test；
6. README/config.example 文档；
7. frame 上限和错误映射；
8. password redaction test。
