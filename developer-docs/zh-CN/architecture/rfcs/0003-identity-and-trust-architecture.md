# RFC-0003：Identity and Trust Architecture

| 字段 | 内容 |
| --- | --- |
| 状态 | Proposed |
| 日期 | 2026-08-17 |
| 负责产品组 | AsterFSD Platform |
| 影响范围 | Identity、账号、Network Profile、Rating、credential、授权、审计、Gateway admission 和 Web 注册 |
| 上位 RFC | [RFC-0001](0001-asterfsd-platform-architecture.md)、[RFC-0002](0002-technology-stack-and-infrastructure-profiles.md) |
| 相关 RFC | [RFC-0004：Network Runtime, Sharding and High Availability](0004-network-runtime-sharding-and-high-availability.md)、[RFC-0005：Event Model and Delivery Semantics](0005-event-model-and-delivery-semantics.md)、[RFC-0007：Activity and Dispatch Integration](0007-activity-and-dispatch-integration.md) |
| 用户侧术语 | Network Profile / 网络身份 |
| 内部领域术语 | `NetworkMembership` |

## 1. 摘要

AsterFSD Identity 是账号、网络身份、credential、Rating、角色、权限、suspension 和认证结果的唯一权威。Web、Gateway、Dispatch 和 Admin 不直接读取 Identity 数据库，而是通过进程内 service contract 或 Tonic gRPC 调用同一语义。

平台允许一个 Identity Authority 管理一个或多个逻辑 Network。公司只创建一个 Network 是正常且完整的部署；托管平台可以管理多个 Network。一个 Gateway runtime/shard 在启动后只属于一个 `NetworkId`，不会在同一个实时状态域中混合不同 Network。

本 RFC 固定以下模型：

```text
Identity Authority
├── Account
├── Organization
│   └── Network
└── NetworkMembership (user-facing: Network Profile)
    ├── Account
    ├── Network
    ├── ATC/Pilot Rating
    ├── Roles/Permissions
    ├── Status/Suspension
    ├── Network Credentials
    └── Authorized Clients
```

关键决策：

1. `Account` 表示 Identity Authority 内稳定的人类/服务账号；它不直接携带某个 Network 的 Rating 和封禁状态。
2. `NetworkMembership` 表示 Account 在一个 Network 中的身份；用户界面称为“网络身份”或 “Network Profile”。
3. Rating、role、permission、suspension、network credential 和 authorized client 都按 NetworkMembership 管理。
4. 一个 Network 只能配置一个 Identity Authority；不能由一个系统验密码、另一个数据库读 Rating、第三个服务判断 suspension。
5. Web credential、network app password、Aster session ticket 和 service credential 是不同凭据，不能互相复用。
6. Classic/VATSIM 客户端使用可撤销的 network app password，不发送网站主密码。
7. Aster 原生客户端优先使用短期、指定 network/audience 的 session ticket。
8. Gateway 只保存有界 `AuthenticatedPrincipal` 快照，不保存 Identity entity、password hash 或 Web session。
9. 新登录在 Identity 不可用时 fail closed；已有 session 不立即批量掉线，但受有界 admission lease 和显式 revoke/suspension 事件约束。
10. 所有 credential、role、Rating、suspension 和 membership 变更都产生可审计、可重放、幂等的 durable event。

## 2. 背景与问题

当前 MVP 使用内部数据库验证 network id/password，并把 ATC/Pilot Rating 作为账号数据返回。平台化后将出现：

- Web 注册、登录、MFA/OIDC 和账号设置；
- Classic/VATSIM 客户端的 password 登录；
- Aster 客户端的 token/ticket 登录；
- 一个运营方管理一个或多个 Network；
- 同一 Account 在不同 Network 中拥有不同 Rating、role 和 suspension；
- embedded Identity 与 remote gRPC Identity；
- 外部 Identity Authority、已有 Web 用户库或第三方 IdP；
- 管理员即时封禁、credential revoke 和活跃连接下线；
- History、Map、Dispatch 需要身份引用，但不应获得 credential 数据。

如果不先固定权威边界，会出现：

- Web 数据库和 FSD 数据库同时维护 account；
- Rating 在全局账号与 Network 配置之间冲突；
- Gateway 缓存 password 或直接查询 Identity 表；
- 外部认证只验密码，本地数据库继续决定 suspension，形成拆分权威；
- Classic 客户端发送 Web 主密码；
- suspension event 丢失后，被封禁连接永久保留；
- 相同 `account_id` 在不同 issuer 下碰撞；
- role 名称直接成为 Gateway 权限判断；
- 多 Network 补丁遗漏 `network_id`，造成跨网络数据和权限泄漏。

## 3. 术语

### 3.1 Identity Authority

拥有账号和网络身份权威的逻辑服务。它可以是：

- Standalone 进程中的 `EmbeddedIdentityService`；
- 独立的 `aster-identity` Tonic service；
- 运营方实现的 custom Rust/gRPC adapter。

Identity Authority 的稳定身份用 `AuthorityId` 或等价 issuer 标识。跨 Authority 的 subject 不能只用裸字符串比较。

### 3.2 Organization

拥有或运营 Network 的组织边界。用户侧可以称“组织”“公司”或“社区”。

```text
Organization
├── one Network       常见公司/社区部署
└── many Networks     托管平台、区域或活动网络
```

Standalone 自动创建一个本地 Organization，用户不需要理解多租户。

### 3.3 Network

独立的逻辑飞行网络，不是 wire protocol：

- 拥有自己的 callsign namespace；
- 拥有自己的 Network Profile、Rating 和 policy；
- 拥有自己的 session、position、flight plan、handoff 和 History scope；
- 可以同时通过 Classic、VATSIM 和 Aster listener 接入；
- 可以由一个或多个 Gateway shard 承载。

一个 Gateway runtime/shard 只属于一个 Network。Network 选择来自受信配置和部署，不接受未认证客户端通过 packet 字段任意切换。

### 3.4 Account

Identity Authority 内稳定的账号主体。Account 可以绑定：

- Web password；
- OIDC/external identity；
- MFA/recovery；
- profile/display information；
- 一个或多个 NetworkMembership。

Account 不直接拥有某个 Network 的 ATC/Pilot Rating、封禁或 network app password。

### 3.5 NetworkMembership

Account 与 Network 的关系和 Network 内身份。用户侧术语统一为：

```text
中文：网络身份
英文：Network Profile
```

内部领域/数据库可以使用 `NetworkMembership`，因为它准确表达 account-network 关系。

### 3.6 AuthenticatedPrincipal

一次成功认证后发给 Network Runtime 的最小授权快照，不是 Account entity：

```text
AuthenticatedPrincipal
├── authority_id
├── account_id
├── membership_id
├── network_id
├── principal_version
├── display identity required by protocol
├── client type admission
├── ATC/Pilot Rating
├── capabilities/permissions
├── issued_at
├── refresh_after
└── valid_until
```

### 3.7 Credential

用于证明主体身份的秘密或可验证 assertion。不同 credential 的 audience、生命周期和泄漏影响不同，禁止把它们抽象成一个无类型 `token: String`。

## 4. 标识和作用域

### 4.1 标识类型

至少定义以下强类型标识：

```text
AuthorityId
OrganizationId
NetworkId
AccountId
MembershipId
CredentialId
PrincipalVersion
ConnectionId
```

要求：

- 跨进程标识全局唯一或以 Authority/Network namespace 组合唯一；
- 不把数据库自增主键直接暴露为跨服务身份；
- `ConnectionId` 只属于 Gateway runtime，不替代 Account/Membership；
- event、audit、History 和 gRPC contract 使用强类型语义；
- log/metrics 不把高基数标识全部作为 metrics label。

### 4.2 唯一性

```text
Account external subject:
  unique(authority_id, issuer, subject)

NetworkMembership:
  unique(network_id, account_id)

Network callsign ownership:
  unique(network_id, normalized_callsign) while active

Credential:
  unique(authority_id, credential_id)
```

同一个 callsign 可以在两个不同 Network 同时存在；同一 Network 内 case-insensitive 唯一。

### 4.3 一个公司一个 Network

这是首要正常形态，不是降级模式：

```text
Organization: Example Aviation
Network: Example Network
Gateway shards: 1..N
Identity: embedded or gRPC
```

多 Network 能力只意味着模型不把所有数据做成全局单例，不要求每个运营方创建多个 Network。

## 5. 领域模型

### 5.1 Account

概念字段：

```text
Account
├── id
├── authority_id
├── status
├── display_name/profile
├── locale/timezone
├── created_at/updated_at
└── version
```

Account status 只表达全局账号状态，例如：

- `Active`；
- `Suspended`；
- `Disabled`；
- `Deleted/Anonymized`。

Network 局部 suspension 不修改 Account 全局状态。

### 5.2 Organization 和 Network

```text
Organization
├── id
├── name
├── status
└── owner/admin policy

Network
├── id
├── organization_id
├── name/slug
├── status
├── identity_authority_id
├── policy_version
└── created_at/updated_at
```

一个 Network 只有一个 `identity_authority_id`。迁移 Authority 是显式数据迁移，不是运行时 fallback 链。

### 5.3 NetworkMembership

```text
NetworkMembership
├── id
├── network_id
├── account_id
├── status
├── atc_rating
├── pilot_rating
├── authorization_version
├── joined_at
├── suspended_at/until
├── suspension_reason_internal
└── updated_at
```

状态机：

```text
Pending -> Active -> Suspended -> Active
                   -> Revoked
Pending ------------------------> Revoked
```

- `Pending`：存在邀请/注册，但不能进入 Network；
- `Active`：可以按 capability/policy 登录；
- `Suspended`：暂时禁止进入，已有 principal 按 revoke policy 失效；
- `Revoked`：关系终止，需要重新创建或显式恢复流程，不能靠 credential 自动复活。

状态变化必须使用 CAS/version，防止旧管理员页面覆盖较新的 suspension。

### 5.4 Rating

ATC/Pilot Rating 使用 `aster_fsd_model` 的强类型 enum 语义，但 Identity persistence 自己负责数据库映射。

约束：

- Rating 属于 NetworkMembership；
- ATC Rating 与 Pilot Rating 分开；
- protocol wire integer 由 protocol backend 映射；
- database string/integer 由 persistence 映射；
- 不把未知值静默转换为最高或最低 Rating；
- Rating 不自动授予 administrator 权限；
- requested client type 的 admission 由 Rating + capability + Network policy 共同判断。

### 5.5 Role 和 Permission

Role 是 Network 内可管理的权限集合，Permission/Capability 是稳定授权语义：

```text
Role: Controller
  -> network.connect.atc
  -> network.handoff.manage

Role: NetworkAdmin
  -> network.member.manage
  -> network.session.disconnect
  -> network.policy.manage
```

规则：

- Gateway 不根据 role 名称写业务分支；
- Identity 返回最小 capability 集或预计算 admission result；
- role 变化增加 `authorization_version`；
- permission 名称有 namespace 和版本治理；
- organization role 与 network role 分开；
- service principal 与 human role 分开。

## 6. Credential 模型

### 6.1 Credential 类型

```text
WebCredential
  网站密码、Passkey、OIDC session、MFA

NetworkAppPassword
  Classic/VATSIM 等传统客户端登录

NetworkSessionTicket
  Aster 原生客户端短期登录

ServiceCredential
  Gateway、Web、History 等服务间 mTLS/workload identity
```

四类 credential 不共用 secret、audience、TTL 和 revoke 流程。

### 6.2 Web credential

Web 登录由 Identity/Web Control Plane 管理。可以支持：

- Argon2 password；
- Passkey/WebAuthn；
- OIDC/OAuth external identity；
- MFA/recovery code。

Gateway 不接收 Web cookie、Web refresh token 或网站主密码。

### 6.3 Network app password

传统客户端使用按 NetworkMembership 创建的 app password：

```text
NetworkAppPassword
├── credential_id
├── membership_id
├── label
├── secret_hash
├── allowed client/protocol scope
├── created_at
├── expires_at
├── last_used_at
├── revoked_at
└── version
```

要求：

- secret 只在创建时展示一次；
- 数据库只保存 Argon2 hash 和参数；
- app password 可单独撤销，不改变 Web password；
- 默认绑定 Network；
- 可选限制 protocol/client id；
- `last_used_at` 异步、有界更新，不能阻塞登录成功；
- credential 列表只返回 id、label、时间和状态，不回显 secret/hash；
- Classic/VATSIM wire 可能明文承载 password，因此 app password 必须限制泄漏影响；
- 支持 TLS listener 或受信 proxy 时优先使用，但不改变 Classic wire 兼容合同。

### 6.4 Aster NetworkSessionTicket

现代 Aster 客户端优先通过 Web/OIDC session 申请短期 ticket：

```text
IssueNetworkTicket(account_session, network_id, client_context)
  -> ticket

Aster client
  -> Gateway
  -> ValidateNetworkTicket
  -> AuthenticatedPrincipal
```

Ticket 必须包含或绑定：

- issuer/authority；
- subject/account；
- membership；
- network audience；
- client/audience；
- issued/expiry；
- nonce/jti；
- authorization version；
- signing key id 或 opaque lookup id。

Ticket 短期有效，不能作为 Web refresh token。是否 one-time use 由 ticket ADR 决定，但 replay 风险必须测试。

### 6.5 Service credential

Gateway 调用 Identity、Web 调用 Control、History 订阅 event 使用服务身份：

- mTLS/workload identity；
- NATS NKeys/JWT account；
- service-specific database role；
- 最小权限；
- 独立 rotation；
- 不使用用户 credential；
- 不把 service credential 写入 config example、日志、metric 或 event。

## 7. 认证和准入链路

### 7.1 Classic/VATSIM password 登录

```text
client frame
  -> bounded protocol decode
  -> AuthenticateNetworkPassword
  -> Identity verifies app password
  -> Identity evaluates membership/rating/capability
  -> AuthenticatedPrincipal
  -> Network Runtime admission
  -> password dropped
```

password 只存在于 decode -> Identity authenticate 的最短链路：

- 不进入 command event history；
- 不进入 peer presence；
- 不进入 snapshot；
- 不进入 error detail；
- 不记录 raw payload；
- public `#AA/#AP` 重建并清空 password。

### 7.2 Aster ticket 登录

```text
ticket frame
  -> ValidateNetworkTicket
  -> verify issuer/audience/expiry/replay/version
  -> AuthenticatedPrincipal
  -> admission
```

Gateway 不自行读取 Identity signing database。验证可以是：

- gRPC opaque ticket introspection；
- 经过明确 key rotation/revoke 设计的本地签名验证 adapter。

两种实现返回同一 principal contract。

### 7.3 Admission

Identity 负责“这个主体是谁，以及拥有什么授权”；Network Runtime 负责“这个连接此刻是否可以进入实时网络”。

Network Runtime 仍校验：

- configured `NetworkId` 与 principal 一致；
- requested client type；
- protocol revision/capability；
- callsign uniqueness；
- session phase；
- source ownership；
- max clients/network policy；
- requested Rating 与 authority Rating 的一致性。

Identity 不能直接注册 callsign 或写 Gateway session registry。

### 7.4 AuthenticatedPrincipal

Principal 是不可变、有版本的最小快照：

```text
AuthenticatedPrincipal
├── identity: AuthorityId + AccountId + MembershipId
├── network_id
├── membership_status = Active
├── atc_rating
├── pilot_rating
├── permissions/capabilities
├── authorized client constraints
├── authorization_version
├── issued_at
├── refresh_after
└── valid_until
```

不包含：

- password/hash；
- Web cookie/refresh token；
- MFA secret/recovery code；
- database row/entity；
- 不需要向协议客户端公开的 email/profile 数据；
- suspension 内部理由。

## 8. Identity Service contract

### 8.1 Local 与 gRPC

```text
IdentityService contract
├── EmbeddedIdentityService
│   └── IdentityRepository -> SQLite/PostgreSQL
└── GrpcIdentityClient
    └── aster-identity -> IdentityRepository
```

Gateway 只依赖 contract，不根据 mode 写两套认证逻辑。

### 8.2 Tonic package

```text
package asterfsd.identity.v1;
```

Gateway 所需 RPC：

- `AuthenticateNetworkPassword`；
- `ValidateNetworkTicket`；
- `RefreshPrincipal`；
- `GetPrincipalStatus`（运维/恢复用途）。

Web/Control 所需 RPC：

- `RegisterAccount`；
- `AuthenticateWebSession` 或对接 Web auth service；
- `ListNetworkProfiles`；
- `Create/RevokeNetworkCredential`；
- `IssueNetworkTicket`；
- `Create/Update/Suspend/Resume/RevokeMembership`；
- `SetRatings`；
- `Assign/RevokeRole`；
- `ListAuditEvents`（受权限约束）。

具体 API surface 在 proto ADR 中固定；本 RFC 固定职责和安全语义。

### 8.3 Request context

所有 mutation/query 按需要包含：

- caller service principal；
- actor account/admin；
- authority/network scope；
- request id；
- correlation/causation id；
- idempotency key；
- expected version/CAS；
- client metadata（脱敏且有界）；
- deadline。

### 8.4 错误语义

内部错误至少区分：

- invalid credential；
- inactive account；
- missing/inactive membership；
- suspended/revoked；
- insufficient capability/rating；
- expired/revoked credential；
- wrong network/audience/client；
- rate limited；
- dependency unavailable；
- deadline exceeded；
- version conflict。

对 Classic/VATSIM 客户端的认证失败保持不可枚举，统一映射到协议允许的 generic authentication error（Classic 使用 `$ER006`）。详细原因只进入受限 audit/metrics，不发给未认证客户端。

## 9. 一个 Network 一个 Authority

### 9.1 不变量

```text
Network.identity_authority = exactly one AuthorityId
```

这个 Authority 返回完整准入结果：

- account status；
- membership status；
- ATC/Pilot Rating；
- permission/capability；
- credential/client authorization；
- principal version/lease。

禁止：

```text
external system verifies password
local users table provides Rating
third service provides suspension
Gateway merges all three
```

### 9.2 外部 Authority

外部系统通过 gRPC 实现 `IdentityService`，或由官方 adapter 把其 API 映射为完整 principal。外部 provider 的数据库、密码格式和用户 entity 不进入 Gateway。

如果外部系统只能认证用户、不能提供 Network Profile/Rating，必须在 Identity service 内形成一个明确的权威聚合边界；Gateway 仍只看到一个 Authority，不能自己拼接。

### 9.3 Authority 迁移

切换 Authority 是显式迁移：

- freeze/maintenance policy；
- export account/profile/credential mapping；
- stable subject mapping；
- credential 重新签发或明确失效；
- authorization version 增加；
- active principal revoke/refresh；
- audit 和 rollback plan。

配置 fallback 到另一个 Authority 不构成迁移，也不允许在认证超时时静默切换。

## 10. 多 Network 模型

### 10.1 Control Plane

Identity/Control Plane 可以管理多个 Network：

```text
Organization A
└── Network A

Organization B
├── Network B1
└── Network B2
```

每个 Network 有独立：

- Identity Authority selection；
- Network Profile/Rating；
- role/policy；
- Gateway shard；
- callsign namespace；
- History/event subject；
- public endpoint/config。

### 10.2 Gateway

一个 Gateway runtime/shard：

- 启动时绑定一个 `NetworkId`；
- 只接受该 Network 的 principal；
- 只维护该 Network 的 sessions/callsigns；
- 只发布该 Network 的 event subject；
- 不通过 client packet 动态切换 Network；
- 多 listener 可以承载不同 protocol，但属于同一个 Network。

同一 Network 的多个 Gateway shard 属于 RFC-0004，不改变 Identity scope。

### 10.3 Standalone 默认 Network

Standalone 自动创建或显式初始化：

```text
Authority: local
Organization: local
Network: default
```

配置只要求用户设置可读名称；内部生成稳定 ID 并持久化。重启不能重新生成另一个 NetworkId。

用户无需看到多 Network UI，除非开启 Control Plane 管理能力。

## 11. Rating、角色和 Client admission

### 11.1 Rating 与角色分离

```text
Rating
  表示飞行/管制资质等级

Role/Permission
  表示在平台中能执行什么操作
```

例子：

- 高 ATC Rating 不自动成为 NetworkAdmin；
- NetworkAdmin 不自动获得高 ATC Rating；
- Observer 可以有查看权限但没有 Pilot/ATC admission；
- Dispatch 权限不由 protocol rating 推导。

### 11.2 Client authorization

Network credential 可以限制：

- Pilot；
- ATC；
- Observer；
- protocol dialect；
- approved client id/version；
- expiry；
- source IP/risk policy（可选且不作为唯一身份）。

Identity 返回结构化约束；protocol backend 只解析 client-reported wire 字段，不能自行授予权限。

### 11.3 Rating 变更

Rating mutation：

1. 校验 actor permission；
2. 使用 expected authorization version/CAS；
3. 更新 Membership；
4. 增加 authorization version；
5. 写 audit；
6. 同事务写 outbox；
7. 发布 `MembershipRatingsChanged`；
8. Gateway refresh/re-evaluate active principal。

是否立即断开不再满足准入条件的连接由 Network policy 决定，但必须有确定行为和测试。

## 12. Suspension、Revoke 和活跃连接

### 12.1 Durable events

Identity 至少发布：

```text
AccountSuspended
AccountDisabled
MembershipSuspended
MembershipResumed
MembershipRevoked
MembershipRatingsChanged
MembershipRolesChanged
CredentialCreated
CredentialRevoked
PrincipalInvalidated
```

事件包含：

- event id；
- authority/network/account/membership scope；
- authorization version；
- occurred at；
- actor/correlation；
- redacted reason category；
- schema version。

### 12.2 Gateway 索引

Network Runtime 除 `ConnectionId -> Session` 外，需要能按最小身份定位活跃连接：

```text
(AuthorityId, MembershipId) -> active ConnectionId(s)
```

该索引用于 revoke/suspension，不替代 callsign registry。注册和释放必须与 session lifecycle 一致。

### 12.3 立即失效

收到版本更新的 suspension/revoke event：

1. 校验 Network/Authority scope；
2. 丢弃重复或旧 authorization version；
3. 标记 principal invalid；
4. 对受影响 session 产生 protocol-specific disconnect/control effect；
5. 原子释放 session/callsign/principal index；
6. 记录不含秘密的审计/指标。

事件 at-least-once，处理必须幂等。

### 12.4 Admission lease

为覆盖丢失事件和长时间 Identity 分区，principal 有有界 lease：

- `refresh_after` 前无需同步访问 Identity；
- Gateway 在后台有界 refresh；
- `valid_until` 后未成功 refresh 的 principal 不无限有效；
- Identity 短暂故障不立即断开全部连接；
- 超过最大 lease 后按 Network policy drain/disconnect；
- exact duration 在配置 ADR 中固定，并设平台上限。

## 13. 可用性和失败策略

| 场景 | 新登录 | 已有连接 | mutation | 审计/指标 |
| --- | --- | --- | --- | --- |
| Identity 正常 | 正常 | 正常/后台 refresh | 正常 | 正常 |
| Identity 短暂不可用 | fail closed | lease 内继续 | 失败 | timeout/unavailable |
| Identity 长时间不可用 | fail closed | lease 到期按 policy 断开 | 失败 | readiness/dependency degraded |
| revoke event 重复 | 无影响 | 幂等处理 | 无影响 | duplicate count |
| revoke event 丢失 | 无影响 | refresh/lease 最终失效 | 无影响 | lag/gap alert |
| event transport 不可用 | auth 可按 outbox policy继续 | refresh 仍兜底 | 需要强审计的操作可拒绝 | outbox depth/age |
| database unavailable | fail closed | lease 内继续 | 失败 | DB/pool error |

不能在 Identity timeout 时使用上一次 password 验证结果直接批准新的 Classic 登录。Credential 验证缓存不成为旁路 Authority。

## 14. Password hashing 和资源边界

### 14.1 Argon2

- 使用 Argon2id；
- 每个 hash 自带 salt 和参数；
- 参数版本可升级；
- 成功登录时可按 policy rehash；
- hash 不记录日志、不返回 API；
- pepper 如果使用，由 Secret provider 管理，不和数据库一起保存；
- recovery/export 不回显 hash。

### 14.2 Blocking work

Argon2 使用 bounded blocking execution：

- 独立 semaphore/concurrency limit；
- 登录有 deadline；
- 取消/断线后结果不注册 session；
- 一个恶意连接不能阻塞全局 dispatch；
- queue wait 和 verification elapsed 有指标；
- rate limit 在高成本 hash 前执行不泄漏用户是否存在。

### 14.3 防枚举和限流

- 不存在 account、错误 password、inactive membership 对外使用相同认证失败；
- response timing 不承诺完全一致，但使用 dummy verification/等价成本减少明显差异；
- 按 peer/network/credential hint 做有界 rate limit；
- rate limit 不能只依赖可伪造 callsign；
- 管理员 audit 能区分原因，未认证客户端不能；
- metrics label 不包含 network id 之外的用户高基数秘密。

## 15. 数据 ownership 和 schema

### 15.1 Identity-owned 数据

概念 schema：

```text
identity_authorities
organizations
networks
accounts
external_identities
network_memberships
network_credentials
roles
permissions
role_permissions
membership_roles
web_sessions/ticket metadata（如使用 opaque token）
identity_audit_events
identity_outbox
```

这是 ownership 图，不要求所有表名原样实现。

### 15.2 禁止跨服务访问

- Gateway 不查 Identity 表；
- Web 不直接修改 Membership/Rating 表；
- History 不查 credential/hash；
- Map 不存 Web session；
- Dispatch 通过 `AccountRef/MembershipRef` 引用身份；
- 数据导出由 Identity API/作业完成，不开放数据库共享用户。

即使 Standalone 共用一个物理 SQLite 文件，也保持 repository/schema ownership，不在其他模块建立外键到 credential secret。

### 15.3 Transaction 和 outbox

credential、membership、rating、role、suspension mutation 必须：

- 在一个 Identity transaction 中更新权威状态；
- 同事务记录 audit 和 outbox；
- event publisher at-least-once；
- consumer 使用 event id + authorization version 幂等；
- unknown commit outcome 通过 idempotency key/query 恢复，不盲目重复副作用。

## 16. External Identity 和 account linking

### 16.1 External subject

外部身份使用：

```text
(issuer, subject)
```

不使用 email 作为稳定主键。Email 可能变化、复用或未验证。

### 16.2 Linking

把外部 identity 链接到 Account：

- 验证当前 Account session；
- 验证新 external identity；
- 检查该 `(issuer, subject)` 未绑定其他 Account；
- 使用事务/CAS；
- 记录 audit；
- 高风险 unlink 要求重新认证和 recovery 条件；
- 不根据相同 email 静默合并账号。

### 16.3 外部 Rating

如果某 Network 使用外部 Rating Authority，它必须通过该 Network 的唯一 Identity Authority contract 返回完整 Membership principal。同步任务可以把外部数据投影到 Identity-owned cache，但：

- stale/unknown/error 不自动升级或降级 Rating；
- source/version/observed_at 明确；
- stale result 可以进 history，不能覆盖更新的管理员或 authority state；
- mutation ownership 和同步冲突策略显式配置。

## 17. Web 注册和管理流程

### 17.1 注册

```text
Browser
  -> Axum Web Control Plane
  -> IdentityService.RegisterAccount
  -> Account
  -> optional NetworkMembership request/invite
```

注册账号不自动等于加入所有 Network。单 Network Standalone 可以按配置自动创建 Active Membership，但仍通过同一 service contract。

### 17.2 创建 network credential

```text
authenticated Web session
  -> choose Network Profile
  -> re-auth/MFA when required
  -> CreateNetworkCredential
  -> show secret once
```

响应明确 `secret` 只出现一次。浏览器刷新、列表 API、审计和日志不回显。

### 17.3 管理员操作

管理员 mutation 包含：

- actor；
- target membership；
- network scope；
- reason；
- expected authorization version；
- idempotency key；
- audit result。

管理员不能修改自己无权管理的 Organization/Network，也不能通过直接提交高 Rating 绕过 permission。

## 18. Standalone Profile

### 18.1 装配

```text
asterfsd
├── EmbeddedIdentityService
├── IdentityRepository
├── local default Organization/Network
├── SQLite
└── local durable outbox
```

### 18.2 Bootstrap admin

没有默认 admin password。官方流程使用显式 CLI/one-shot command：

```text
asterfsd identity bootstrap-admin
```

要求：

- 交互模式从 TTY 安全读取 secret；
- 容器模式从 Secret file/provider 读取；
- 第一次成功后不能重复创建第二个 bootstrap owner；
- 输出不打印 password/hash；
- bootstrap 状态有 audit；
- recovery 有显式、可审计流程；
- config.example 不包含默认 credential。

### 18.3 用户体验

Standalone 默认隐藏 Organization/Network/Profile 复杂度：

- UI 显示一个网络；
- 创建 Account 时按配置创建默认 Network Profile；
- app password 页面称“连飞密码/Network App Password”；
- 内部仍写入稳定 `NetworkId` 和 `MembershipId`。

## 19. Distributed/Kubernetes Profile

```text
Axum Web ------Tonic------> Identity Service ------> PostgreSQL
Gateway -------Tonic------> Identity Service
Identity ------JetStream--> Gateway/History/Audit projections
```

要求：

- Identity service 使用独立 PostgreSQL role/database/schema；
- Tonic 9090 使用 mTLS/workload identity；
- NetworkPolicy 只允许批准的 Gateway/Web/Admin caller；
- database 和 gRPC 不直接公开公网；
- migration 使用独立 Job/owner；
- API Pod 不在每次启动隐式执行 migration；
- readiness 区分进程、database、outbox 和 event transport；
- HPA 不导致 migration/bootstrap 并发；
- Secret rotation、backup/restore、PITR 和 principal revoke 有演练；
- rollout 验证 current imageID、Service/EndpointSlice、mTLS、health 和真实认证。

## 20. 审计和隐私

### 20.1 Audit event

必须审计：

- account register/disable/delete；
- external identity link/unlink；
- Web credential/MFA/recovery 变化；
- network credential create/revoke/use（不记录 secret）；
- ticket issue/revoke；
- membership create/status change；
- Rating/role/permission change；
- suspension/resume/revoke；
- 管理员访问和高权限 mutation 失败；
- Authority migration。

### 20.2 Redaction

永不记录：

- password/app password；
- Argon2 hash/salt/pepper；
- Web session/refresh token；
- ticket 原文；
- private key；
- raw login payload；
- database URL credential。

可以记录：

- credential id；
- account/membership/network 强类型 id；
- peer metadata（按 retention/privacy policy）；
- client id/version；
- success/failure category；
- latency/rate-limit result；
- actor/reason category。

### 20.3 数据最小化

Gateway 只接收协议和路由所需的最小 principal。History/Map 使用稳定 pseudonymous id 和按权限解析的 display data，不复制 Identity 的完整 profile。

## 21. Threat model

### 21.1 主要威胁

- Classic plaintext password 被监听或日志泄漏；
- credential stuffing/brute force；
- account enumeration；
- app password 数据库泄漏；
- ticket replay；
- stale principal 在 suspension 后继续有效；
- revoke event 丢失/乱序/重复；
- cross-Network confused deputy；
- service credential 越权；
- admin stale form 覆盖新状态；
- external issuer subject collision；
- Web DB/Gateway DB 双重权威；
- slow Argon2 耗尽 blocking pool。

### 21.2 关键缓解

- network app password 与 Web password 分离；
- Argon2id + one-time display + revoke/expiry；
- generic auth error + rate limit；
- network/audience/client binding；
- principal version + lease + revoke event；
- strong typed NetworkId/AuthorityId/MembershipId；
- mTLS/workload identity；
- CAS/fencing/outbox；
- bounded blocking semaphore；
- Secret redaction/audit；
- single Authority per Network。

## 22. 测试和验收

### 22.1 Domain/model

- Account 与 NetworkMembership scope；
- Membership 状态机；
- Rating enum 全合法/非法值；
- permission 不由 role 字符串和 Rating 隐式推导；
- cross-Network id 拒绝；
- authorization version 单调；
- serde/proto/persistence 映射 round trip。

### 22.2 Credential

- app password 创建只展示一次；
- database/API/log 不出现 secret/hash；
- Argon2 success/failure/rehash；
- expired/revoked/wrong Network/wrong client；
- concurrent revoke vs login；
- rate limit/dummy verification；
- ticket issuer/audience/expiry/replay/key rotation；
- service credential 与 user credential 不能互换。

### 22.3 Membership/authorization

- Pending/Suspended/Revoked 不能登录；
- Resume 后新 principal version；
- Rating/role mutation CAS 冲突；
- duplicate/out-of-order event；
- suspension 立即定位并断开所有 active connection；
- missed event 由 refresh/lease 最终捕获；
- 同 Account 在两个 Network 有不同 Rating/状态；
- 一个 Network suspension 不影响另一个 Network。

### 22.4 Service parity

同一 contract suite 同时运行：

- EmbeddedIdentityService；
- Tonic Identity server/client；
- custom adapter fixture。

覆盖 deadline、cancel、retry、status mapping、request size、mTLS 和 unavailable behavior。

### 22.5 Database

- SQLite temporary runtime；
- PostgreSQL transaction/concurrency/migration；
- feature-gated MySQL contract（若启用）；
- unique constraint；
- unknown commit outcome；
- outbox at-least-once；
- backup/restore；
- migration 与旧 MVP schema 的显式数据处理。

### 22.6 Protocol/TCP

- Classic/VATSIM 登录成功/失败 exact wire；
- generic error 不枚举状态；
- password 不进入 public presence/peer wire；
- disconnect/reconnect；
- requested Rating/client type 校验；
- 真实 Swift/Pilot/ATC 客户端；
- 一个 Network 的 credential 不能登录另一个 listener/network。

### 22.7 故障恢复

- Identity process SIGKILL/restart；
- login commit 前后命名 checkpoint；
- credential create commit 后 response 丢失；
- suspension commit 后 event publish 前崩溃；
- outbox replay；
- revoke event ack 前后 consumer crash；
- stale Gateway refresh；
- lease expiry；
- DB/NATS/Identity network partition；
- audit/secret redaction oracle。

## 23. 已拒绝的设计

### 全局 Account 直接保存 Rating

拒绝：不同 Network 的资质和 policy 会冲突，后续补 `network_id` 容易跨租户泄漏。

### 每个 Gateway 自己维护 users 表

拒绝：造成 Web/Gateway 双重权威，无法安全接入 remote Identity。

### 外部认证 + 本地 Rating 拼接

拒绝：Gateway 成为多个权威的隐式聚合器，故障和 stale 状态不可定义。聚合必须在唯一 Identity Authority 内完成。

### Classic 使用 Web 主密码

拒绝：传统 wire 泄漏影响过大，credential 无法按 Network/client 单独撤销。

### Rating 等同 Role

拒绝：资质和平台权限是不同领域，自动推导会导致越权。

### 只依赖 revoke event 永久授权 session

拒绝：event 可能丢失、延迟或 consumer 故障；必须有 principal version、refresh 和最大 lease。

### Gateway 动态承载任意 Network

拒绝：增加热路径租户选择、跨 Network confused deputy 和 registry scope 风险。Gateway shard 在部署时绑定一个 Network。

### Email 作为跨 Authority 主键

拒绝：email 会变化、复用、未验证，外部 identity 使用 `(issuer, subject)`。

## 24. 实施约束

本 RFC 固定最终模型，不保留旧 users 表访问 facade 或双轨认证：

- 新 Identity contract 一次迁移 Gateway、Web、测试和配置；
- model 中移除 ORM derive，由 persistence 显式转换；
- Rating 从 Account/global user ownership 移到 NetworkMembership；
- Gateway 不直接依赖 repository；
- embedded/gRPC 运行同一 contract suite；
- password 登录改用 network app password；
- public presence 继续清空 password；
- event/outbox 与 principal refresh 同时实现，不留下永久 session 缓存；
- 当前 schema 的破坏性变更使用正式 migration/baseline 决策，不手工伪造数据库；
- config example、README、Web 文案、proto、OpenAPI、测试和 changelog 同步。

## 25. 完成标准

RFC 落地必须同时证明：

1. 一个公司只配置一个 Network 时，Standalone 和 Distributed 都能完整运行。
2. 一个 Identity Authority 管理多个 Network 时，同 Account 的 Profile/Rating/suspension 相互隔离。
3. 一个 Gateway shard 只能接受其配置 Network 的 principal。
4. Web credential、network app password、ticket、service credential 完全分离。
5. Classic/VATSIM 不使用 Web 主密码，password 不进入 peer wire/log/event/snapshot。
6. Embedded 和 Tonic Identity 通过相同 contract suite。
7. Rating、role、permission、suspension 属于 NetworkMembership，且 authorization version 单调。
8. suspension/revoke event 重复、乱序、丢失和 consumer crash 都有确定结果。
9. Identity 短暂故障不立即杀死所有连接，新登录 fail closed；最大 lease 防止无限 stale principal。
10. Identity database 只有 Identity service 访问，其他服务通过 contract/event 获取最小信息。
11. app password hash、Argon2 blocking、rate limit 和 generic error 有边界测试。
12. SQLite/PostgreSQL、真实 client、gRPC、NATS/outbox 和 Kubernetes failure matrix 有实际证据。

## 26. 后续 ADR/RFC

- Identity v1 Protobuf 和 status/error mapping；
- Account/Web authentication、Passkey/OIDC/MFA；
- network app password 格式、显示和 rotation；
- Aster session ticket、signing key 和 replay policy；
- principal lease/refresh duration 和 Gateway revoke index；
- Organization/Network/NetworkMembership schema；
- role/permission capability catalog；
- external Identity Authority adapter；
- [RFC-0005](0005-event-model-and-delivery-semantics.md) 下的 Identity event envelope、outbox/inbox、NATS subject 和 consumer 幂等；
- bootstrap admin/recovery；
- Identity Kubernetes topology、PDB、backup/PITR；
- [RFC-0004](0004-network-runtime-sharding-and-high-availability.md) Network Runtime、Sharding and HA。

这些 ADR 可以细化字段和参数，但不能重新引入全局 Rating、多个 Authority 拼接、Gateway users 表、Web 主密码登录或跨 Network 动态 Gateway。
