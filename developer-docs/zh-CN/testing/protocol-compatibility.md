# 协议兼容测试契约

## 测试层次

1. **Codec**：frame 边界、分隔符、长度和原始 packet。
2. **Backend contract**：每种 dialect 的 command/event exact wire。
3. **Core**：不启动 TCP，验证 state、routing、source、duplicate 和 cleanup。
4. **Runtime integration**：启动真实 listener，用两个以上 socket 验证端到端 wire。
5. **真实客户端**：swift、ATC client 和 pilot client 的登录、位置、私聊、flight plan。

## 必测安全矩阵

- peer wire、tracing capture、error text 中都没有 login password；
- invalid login error 只到请求连接；
- direct text 只到目标；
- unauthenticated socket 不接收 presence/position/message/wind delta；
- duplicate callsign 不覆盖旧 owner；
- spoofed source 不改变其他 client state；
- EOF/logoff/protocol close 都只产生一次 removal；
- slow mailbox 关闭单连接，不阻塞其他连接；
- oversized frame 在继续分配前被拒绝。

## Classic 证据

classic exact-wire fixtures 以 `tmp/fsd-master` 的行为为基线，并记录 C 源码路径。真实客户端结果是额外兼容证据，不用单个客户端的宽容行为替代 server contract。

登录以 `cluser.cpp::execaa`、`execap` 和 `checklogin` 为基线：字段下限先于 callsign 校验；非法 callsign 返回 `$ER002` 并关闭；数字或非数字 revision mismatch 都返回 `$ER010` 并关闭；credential、requested level 和 suspension 分别固定 `$ER006/$ER011/$ER013` 的 environment。字段不足 `$ER004` 与 active session 的 `$ER003` 保持连接；并发 duplicate 只允许一个 connection 原子取得 case-insensitive callsign ownership。所有 peer presence 都重建空 password 字段。

pilot position 的 transponder 采用原始 C 实现的整数 contract：`client.cpp::updatepilot` 使用 `atoi` 接收，`clinterface.cpp::sendpilotpos` 使用 `%d` 广播。因此 fixture 必须覆盖 `0`、`0000` 与 `0700`，并证明 peer wire 分别规范化为 `0`、`0` 与 `700`。

parsed weather 以 `clinterface.cpp::sendweather` 为基线：`#WX` 命中后必须严格按 `#TD -> #WD -> #CD` 输出四层 temperature、四层 wind、两层 cloud、一个 thunderstorm、barometer 和两位 visibility。测试同时覆盖 raw `$AX/$AR`、未知 station `$ER009`、provider failure、source spoof、非法 visibility 和真实 TCP 三帧顺序。

handoff/client-data 以 `cluser.cpp::execmulticast` 为基线：`$HO/$HA/$CI` 要求至少一个 payload field，`#SB/#PC/$C?` 允许零 payload field，全部 direct-only。fixture 覆盖八种 `$HO/$HA/#SB/#PC/$C?/$CI/$PI/$PO` exact wire、unknown target 静默、source spoof `$ER005`、syntax `$ER004`、Aster v1 mapping 与双客户端 TCP sender exclusion。

query/kill 以 `cluser.cpp::execcq`、`execmulticast` 与 `execkill` 为基线：fixture 覆盖 CAPS/ACC/ATIS/INF 双向 exact wire、`$CR` payload 下限、source-range 与 message-range 差异、requester-directed flight-plan、unsupported server query 静默，以及 `$!!` unknown/denied/accepted 的通知、wire、removal 和 close 顺序。`$ER000..013` 的 code、environment 与 C description 逐项固定。

position/range 以 `client.cpp::getrange` 和 `clinterface.cpp::calcrange` 为基线：pilot range 使用整数截断；pilot source 到 pilot target 使用双方 range 之和；ATC source 到 pilot target 使用较大 range；任何 source 到 ATC target 使用 target visual range；任一侧没有 position 时不投递。非法或 spoofed position 不覆盖上一份权威 state；flight plan 只投递 400 NM 内 ATC。

连接生命周期使用真实 TCP 与可控 mailbox/writer fixture 固定：RST、显式 `#DA/#DP`、idle timeout、oversized frame、writer failure 和 shutdown 都进入同一 cleanup；removal exact wire 只出现一次，callsign 可立即重新注册；mailbox full 只取消慢连接，健康连接继续收到后续 frame。

## VATSIM 证据

- `$DI` 只由显式 VATSIM listener 在 accept 后发出；classic listener 继续 client-speaks-first。
- `$ID` fixture 覆盖大小写不敏感的 `SERVER` destination、4 位十六进制 client ID、固定 `3:2`、非空 network ID、正负 signed 9-digit unique number，以及缺字段、额外字段和非法 callsign。
- core 必须在认证前固定 revision `100`，并在取得 callsign ownership 前同时比较 identification/login 的 callsign 与 network ID；mismatch、缺 identification 和 revision `9` 都关闭请求连接。
- Pilot exact wire 固定 `$CQSERVER:<callsign>:CAPS`、IP response 和 `$ER008`；ATC exact wire 固定 CAPS query、ATC status、CAPS profile 与 IP response。
- 两个以上 VATSIM TCP client 的 presence 必须使用 revision `100`，password 继续为空，Pilot `#AP` 保留尾部 real-name 字段；classic recipient 的同一统一 presence 仍由 classic backend 编码 revision `9`。
- ACC/ATIS/INF 与 client CAPS response 当前验证 typed relay；server-side provider 未接入的数据不写假 fixture。

## Rating 证据

- ATC rating 按原始 C `global.h` 的 `LEV_SUSPENDED = 0` 到 `LEV_ADMINISTRATOR = 12` 做完整 round trip。
- Pilot rating 按 VATSIM 累计编码 `0/1/3/7/15/31/63` 做完整 round trip；`2/4/6/8/16/32/64` 等空洞值必须在登录边界拒绝。
- SeaORM 测试必须读取实际数据库列，证明写入的是 `controller_1`、`private_pilot_license` 等稳定语义字符串，而不是整数或 Rust debug 名称。
- ATC/Pilot 类型不得互换；数据库中的未知字符串必须产生结构化读取错误。

## Logging 边界

- `logging.level = "debug"` 的真实子进程必须出现 connection、frame envelope、decoded command、core execution 和 dispatch debug marker。
- `logging.level = "info"` 保留 login/lifecycle 信息，同时压制上述 debug marker。
- `RUST_LOG=info` 覆盖 config debug 时必须输出覆盖告警；非法 `RUST_LOG` 必须告警并回退到 config debug。
- 相同 login fixture 在所有 filter 组合下都断言 password 和 raw `#AP` frame 不出现在 stdout/stderr。
- `database.sqlx_logging = false` 时即使全局 debug 也不出现 `sqlx::query`；显式设为 `true` 后才出现 statement debug。
- 空白 `logging.level` 在 bind 前返回配置错误，避免有效但全静默的空 filter。
