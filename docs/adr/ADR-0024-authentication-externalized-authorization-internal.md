# ADR-0024：认证外置与授权内置（凭证签发不属平台）

> 状态：Accepted  
> 日期：2026-09-03  
> 决策所有者：Business API（认证）、平台安全  
> 关联文档：[SECURITY_ARCHITECTURE](../architecture/SECURITY_ARCHITECTURE.md)（§4 认证、§23.1 实现状态）、
> [DEPLOYMENT_ARCHITECTURE](../architecture/DEPLOYMENT_ARCHITECTURE.md)、
> [PLAN-0012（已归档）](../plans/archive/2026/PLAN-0012-runnable-v1-auth-ai-provider-observability.md)、
> [PLAN-0012 COMPLETION AUDIT](../reports/PLAN-0012-COMPLETION-AUDIT.md)（真实 IdP 验收 amendment）  
> 替代：无  
> 被替代：无

## 1. 背景

PLAN-0012/v0.1 已交付真实 OIDC/JWT 认证（relying-party 验证路径 `apps/business-api/src/oidc.rs`），
并在 2026-09-03 用自助注册的 Auth0 租户完成了 production 模式端到端验收（E2E 12/12 PASS、
8 项生产 fail-closed 启动负例全部拒绝，含 authorization_code+PKCE 浏览器流程与 refresh 轮换）。
验收过程中暴露了标准 IdP 消费侧的真实复杂度：JWKS discovery 与未知 kid 即时刷新、`roles` 为
Auth0 保留 claim（自定义注入被静默忽略）、Post Login Action 中 `event.accessToken.aud` 在 ROP 流
不可读、refresh token 轮换与 consent 中的 offline access 等。

由此产生一个需要长期固化的边界问题：**平台是否应当实现自己的 IdP 服务（凭证签发）？**
本 ADR 决定认证与授权的归属边界，以及各交付形态（SaaS IdP、企业统一身份、私有化交付）下的
允许做法。本 ADR 不选择具体 IdP 产品——产品选择是部署级运维决策。

## 2. 决策驱动因素

- **爆炸半径**：凭证签发是全系统风险最高的组件；被绕过即全平台失守，无局部降级。Key rotation、
  算法治理、会话/令牌撤销、MFA、暴力破解防护、合规审计需要持续安全投入与渗透测试，属专职产品领域。
- **领域边界（ADR-0003 战略 DDD）**：凭证签发与平台统一语言无交集。平台的业务能力是文档处理、
  契约管理、审计修复、分析；"签发 JWT/管理密码"不是任何既有 bounded context 的能力，也不是
  平台差异化资产。
- **资产定位**：`authN`（你是谁、凭证如何签发与撤销）可外包；`authZ`（租户隔离、claim →
  `ManagementPermission` 权限映射、审计链、跨租户拒绝）是平台必须自持的核心资产。现状实现正是
  该分工：`oidc.rs` 只做签名/issuer/audience/expiry 验证与 claim 映射，授权判定全在平台内。
- **可替换性**：validator 仅依赖标准 OIDC 语义（discovery、JWKS、RS256/ES256 allow-list），
  不绑定任何供应商 SDK；更换 IdP 只改配置（issuer/audience/jwks_url）与 IdP 侧 claim 注入。
- **经验证据**：本次验收仅消费标准 IdP 就涉及上述大量协议细节；自研意味着自己实现并做对全部
  细节，而外置 IdP 数十分钟即可投入使用且由厂商承担演进与合规。
- **基线一致性**：`SECURITY_ARCHITECTURE.md` §4 已规定"正式环境优先采用 OIDC/OAuth2 或公司统一
  身份平台"，并要求生产禁止无签名 token 与开发万能用户（已由配置 fail-closed 落实）。

## 3. 候选方案

### 方案 A：平台自研 IdP 服务（拒绝）

描述：在平台内实现凭证签发——密码/凭据存储、登录流程、token/refresh 签发、JWKS 发布、会话与
撤销、MFA。

拒绝理由：第 2 节全部因素叠加。具体地：(1) 新增一个必须以安全级质量长期维护的 bounded context，
其失败模式（认证绕过）不可接受；(2) 与 ADR-0003"业务能力/统一语言/数据所有权"划分冲突，identity
issuance 不属于任何现有上下文；(3) 放弃厂商承担的关键轮换、合规与攻击面吸收；(4) 无任何业务
能力收益。

### 方案 B：外部标准 OIDC IdP，平台为 relying party（选定）

描述：人员认证一律由外部支持标准 OIDC 的组件承担（SaaS IdP 如 Auth0，或公司统一身份平台）；
平台通过 `oidc.rs` 验证 access token，claim 契约（`sub`/`tenant_id`/`user_id`/`roles`/
`management_permissions`）由 IdP 侧流程（如 Post Login Action / 身份中台）注入。

优点：责任划分清晰、平台代码零签发面、IdP 可替换、验收已实证。
缺点与风险：依赖外部可用性（认证时段故障即登录不可用，已认证会话在 token TTL 内不受影响）；
claim 契约需在 IdP 变更时同步维护（见第 5 节操作约束）。

### 方案 C：私有化交付捆绑开源 OIDC server（作为 B 的部署变体接受）

描述：完全离线/强隔离交付环境无 SaaS IdP 可用时，交付方案捆绑成熟开源 OIDC server（Keycloak
或等价，即 PLAN-0012 T3.3 后置路线）作为外部组件部署。

接受理由：这仍是"认证外置"——开源 IdP 是被捆绑的独立组件，平台代码不包含任何签发逻辑；与
方案 A 有本质区别。约束：只能集成，不得 fork 出平台自有的签发实现。

## 4. 决策

1. **平台不实现凭证签发服务。** 人员面向的认证（凭据存储、登录、token/refresh 签发、密钥发布、
   会话撤销、MFA）一律由支持标准 OIDC 的外部组件承担。平台核心不得包含 credential 签发代码路径；
   `dev_auth` 仅限开发环境，生产配置 fail-closed（现状 `config.rs` 校验即门禁）。
2. **授权内置于平台。** 平台拥有并负责：claim → `AuthenticatedPrincipal` 映射、租户隔离、
   `ManagementPermission` 授权判定、审计链、跨租户拒绝。claim 契约（`sub` 非空、`tenant_id`
   非 nil UUID、`user_id`、可选 `roles`、`management_permissions`）为版本化公开契约，IdP 侧
   注入变更必须与 API 契约同步评审。
3. **服务到服务身份独立。** 按 `SECURITY_ARCHITECTURE.md` §4，服务身份不复用人员凭证；未来采用
   专用机制（mTLS/workload identity），同样不构成人员面向的 IdP。
4. **私有化交付通过捆绑开源 OIDC server 满足**（方案 C 约束），不通过自研满足；如需 demo compose
   与 console 登录流程，恢复 T3.3 并以独立计划执行。
5. **B2B 用户目录是业务数据，不是签发服务。** 若未来需要企业成员、邀请、外部 IdP 联邦等能力，
   可立 directory/identity 业务上下文管理这些业务事实；凭证签发层仍归外部 IdP。
6. **生产 IdP 集成加固清单**（验收沉淀，运维必须满足）：`roles` 经 RBAC API 下发而非自定义
   claim；claim 注入 Action 必须按 audience guard；生产实例从不含 repo dev 配置文件
   （`config/default.toml` 含 `dev_secret`）的工作目录启动，配置校验拒绝否则 fail-closed。

## 5. 边界与非目标

决定：签发/验证的职责归属；claim 契约的契约地位；交付形态约束；服务身份独立原则的重申。

不决定：
- 不选择具体 IdP 产品或供应商（部署级运维选择）；不实现 SSO 联邦、SCIM、密码找回等 IdP 侧功能。
- 不引入新的公开 API/事件；不改数据所有权。
- 不在本 ADR 实现 Keycloak demo compose（T3.3，如需另立计划）。

## 6. 后果

正面：安全签发责任外置；平台认证面只有只读验证路径，可替换、可审计；与 v0.1 现状零差异——
现有实现（`oidc.rs` + 生产配置 fail-closed）即本决策的落地形态，已经真实 IdP 验收证明。

负面与成本：登录可用性依赖外部 IdP；claim 注入契约需在 IdP 变更时人工同步（无协议层强制）；
私有化交付需维护捆绑 IdP 的部署物；第 4.6 节加固清单是运维责任而非代码门禁，需在 Runbook/交付
检查单中跟踪。

风险与缓解：IdP 供应商锁定→ validator 只依赖标准 OIDC，更换属配置迁移；claim 注入错误导致越权
→ `tenant_id` UUID 校验与跨租户拒绝契约测试兜底（v0.1 已覆盖）。

## 7. 实施

- 本 ADR 为文档决策；代码现状即合规（`apps/business-api/src/oidc.rs`、`apps/business-api/src/config.rs`
  生产校验、`scripts/check-architecture.ps1` 无签发路径相关断言可作后续增项）。
- 文档同步：`docs/adr/README.md` 登记表新增本行；`SECURITY_ARCHITECTURE.md` §4/§23.1 已描述
  现状，无需改动。
- 回滚：本 ADR 为决策记录；若未来推翻，须以新 ADR 替代（"替代：ADR-0024"），不得静默实现。

## 8. 验证证据

- 真实 IdP 生产验收（2026-09-03，Auth0 租户，production 模式 `business-api`）：E2E 12/12 PASS、
  8 项 fail-closed 启动负例全部拒绝、authorization_code+PKCE 浏览器全流程；见
  `docs/reports/PLAN-0012-COMPLETION-AUDIT.md` "Slice C IdP leg" amendment 与
  `F:\Workspace\business-platform-staging\evidence\20-*`。
- Main CI `33705531597` 全绿、tag `v0.1` → `2383651`。

## 9. 后续复审条件

以下任一事实变化时重新评估本 ADR：
- 监管或客户要求平台直接掌控凭证签发（届时也优先考虑捆绑/白标开源 IdP，而非自研）。
- B2B 用户目录需求立项（按第 4.5 节立业务上下文，评估与 IdP 的分工细化）。
- OIDC 协议生态出现影响 relying-party 语义的重大变更（token 格式、验证模型）。
- 服务到服务身份机制选型时（另立 ADR，不扩展本 ADR 范围）。
