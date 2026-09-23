# PLAN-0014 Wave 0：Identity / Organization / Policy 真实基线

> Review date: 2026-09-23<br>
> Branch: `feat/PLAN-0014-wave1-iam-policy-reference-conformance`<br>
> Base: `8365de0211390a6a3367b536c5ceaa5256bbb5cf`（同步后的 `main`）<br>
> Scope: PLAN-0013 集成后的 Identity、Organization、Policy，以及 Business API 的认证/授权组合根
> Status: Wave 0 baseline reconstructed; R2 is not claimed

## 1. 证据边界

本报告按 PLAN-0014 的 review-first 规则从源码、测试、migration、公开 DTO、组合根和架构门禁重建现状。PLAN-0013 的计划、完成审计和历史 CI 只作为历史证据索引，不能替代当前源码事实。

本次同步事实：

- `main` 在同步前后均为 `8365de0211390a6a3367b536c5ceaa5256bbb5cf`；
- 本分支从该 SHA 创建，未在 `main` 上修改；
- 当前没有外部 OpenFGA、SpiceDB、Cerbos、Logto、ZITADEL 或 ABP runtime 依赖；
- PostgreSQL 的最新已知证据来自 PLAN-0013 合并前后的 CI；本机当前 PostgreSQL lane 未执行，不能写成当前运行 PASS；
- 本报告中的 “PASS” 仅表示对应测试在本次运行或被明确标注的历史固定 SHA 上通过，不等价于独立发布、真实生产或 R2 接纳。

## 2. 权威与数据所有权重建

| 能力 | 当前权威 | 持久化所有者 | 外部输入 | 明确不属于当前权威的内容 |
|---|---|---|---|---|
| 外部身份映射 | Identity `PlatformUser` + `ExternalIdentity` | `identity` PostgreSQL；SQLite 本地单进程镜像 | OIDC issuer + subject；签名 JWT 由外部 IdP 验证 | IdP credential/session/token/MFA；JWT roles 不直接成为平台授权 |
| 租户准入 | Identity `TenantMembership` | Identity adapter | `tenant_id` 作为已验证 principal 上下文的一部分，并由 membership 重新确认 | Handler 自带 tenant、客户端 `x-*` header、first-user-is-admin |
| 组织关系 | Organization `OrganizationUnit` + `OrganizationMembership` | Organization adapter | Identity 提供只读 `TenantMembershipReader` bridge | 其他上下文直接写 Organization 私有表 |
| 角色/权限/范围 | Policy `PermissionDefinition`、`RoleDefinition`、`RolePermission`、`RoleBinding`、`ResourceScope` | Policy adapter | Application command、可信资源元数据、Identity/Organization ports | 外部授权引擎、客户端 roles/permissions、裸 SQL/表名 |
| 决策 | Policy `Authorize` / `ExplainDecision` | 当前进程内 Policy application + policy query ports | 每次决策重读主体状态、catalog、binding、scope | 外部 IdP、缓存中的未验证 grant、未激活 provider |
| 审计 | 既有 Audit context/transaction | 各拥有者 adapter 在本地事务内写 Audit | actor、tenant、operation、reason | JWT、原始 subject、storage key、provider DTO |

数据边界符合当前架构 Baseline：Identity 不拥有 Organization membership；Organization 通过 composition-root port 读取 Identity；Policy 不复制用户或组织权威数据；跨上下文关系使用 UUID/port，不使用私有表 FK。

## 3. Identity 真实实现

### 3.1 Domain / Application / Ports

- `crates/identity/src/domain/user.rs:17-151`：`PlatformUser` 私有字段，`Active | Disabled` 生命周期，`create`/`rehydrate` 校验，状态切换递增版本；没有物理删除用例。
- `crates/identity/src/domain/external_identity.rs:28-140`：`(issuer, subject) -> PlatformUser` 稳定绑定，issuer/subject 有长度、空白、控制字符和 NUL 校验。
- `crates/identity/src/domain/membership.rs:4-240`：`TenantMembership` 只有 `Active | Suspended`；无独立 `Expired` 或 `Removed` 状态，复杂 IAM lifecycle 明确不在 PLAN-0013 范围。
- `crates/identity/src/application/resolve.rs:26-151`：`ResolveAuthenticatedUser` 先按 issuer/subject 查找，再以确定性 UUIDv5 作为缺省 user id，首次接触只 provision user，不自动给 tenant grant；已绑定 subject 与 claimed user 不一致时拒绝。
- `crates/identity/src/application/access.rs:17-105`：`TenantAccessChecker` 把不存在 user、无 membership、suspended membership、disabled user 分成有界结果，只有 active user + active membership 为 Active。
- `crates/identity/src/application/membership.rs:153-303`：membership 创建、suspend/reactivate、expected version、同 actor 不能自行 reactivate；审计上下文由 use case 创建。
- `crates/identity/src/ports.rs:124-360`：Identity application ports、tenant-scoped query/command、resolve commit、idempotency fingerprint、optimistic version 和 audit contract。

### 3.2 Adapter / Migration / Failure

- `crates/identity-postgres/src/lib.rs` 是 production adapter；事务内完成 provision/membership/status/idempotency/audit，idempotency key 由 `(operation, tenant, key)` 隔离，并以 PostgreSQL advisory transaction lock 串行化相同请求。
- `crates/identity-sqlite/src/lib.rs` 是 local single-process adapter；显式 single-writer/SQLite transaction 语义，与 fake 和 PostgreSQL 共享 port contract，不宣称 multi-worker production 支持。
- `migrations/019_identity_authorization_foundation.sql:28-64` 建立 `platform_users`、`external_identities`、`tenant_memberships`，状态 CHECK、`(issuer, subject)` 唯一、`(tenant_id, user_id)` 唯一和审计/幂等 ledger。
- 外部 identity 的 `UNIQUE (user_id)` 表示当前一个 PlatformUser 只绑定一条 ExternalIdentity；这是现状约束，不应推断为多 IdP 账户合并能力。
- store unavailable/failed 不会变成 allow；API resolver 将基础设施失败映射为 retryable service unavailable，身份不匹配映射为 unauthorized。

### 3.3 当前测试证据

- Domain/Application：`crates/identity` 单元测试和 `contract_semantics.rs` 覆盖 rehydrate、稳定映射、suspend/reactivate、版本冲突和自反应保护。
- Shared adapter contract：`crates/identity-authorization-contracts/src/identity_contract.rs`；本次 `identity-sqlite` contract 通过；该 suite 覆盖跨租户 idempotency、同 key replay/conflict、状态转换、bootstrap ledger 和租户列表边界。
- HTTP adversarial：`apps/business-api/tests/iam_adversarial_sqlite.rs` 覆盖 header spoof、foreign user claim、roles claim、compat grant 边界、自升级、跨租户读写和 explain 租户边界。
- 历史 PostgreSQL：PLAN-0013 completion audit 记录 CI `35418654378` 在固定代码 head 上通过 PostgreSQL identity contract 和 full-chain E2E；本机本次没有 PostgreSQL 数据库，当前 run 记为 `NOT RUN`。

### 3.4 Identity maturity

**R1 foundation / R2 blocked。** 当前已有真实 domain/application/ports、PG/SQLite adapters、migration、API 组合和行为契约；尚无独立 module SemVer、manifest、digest、迁移映射、可重复 release artifact、consumer build 和独立发布证据。Service identity / M2M 不是当前已实现能力。

## 4. Organization 真实实现

### 4.1 Domain / Application / Ports

- `crates/organization/src/domain/unit.rs:46-260`：`OrganizationUnit` 支持 company/department/team、parent hierarchy、active/disabled、深度/环校验和版本化 rehydrate。
- `crates/organization/src/domain/membership.rs:3-236`：`OrganizationMembership` 是 `Active | Inactive` soft lifecycle；remove 保留历史并写 `deactivated_at`，re-add 复用历史行并递增版本。
- `crates/organization/src/application/units.rs`：创建、更新、move、parent/tenant/status 校验；store 侧再次检查 cycle、depth、tenant 和并发版本。
- `crates/organization/src/application/members.rs:135-319`：add/remove/re-add，目标用户必须通过 Identity `TenantMembershipReader` 为当前租户 active member；跨租户、无效 unit 和 inactive unit 拒绝。
- `crates/organization/src/ports.rs:318-360`：Organization 只依赖 typed reader/command/query port，不直接读 Identity 表；scope bridge 在组合根实现。

### 4.2 Adapter / Migration / Failure

- `crates/organization-postgres/src/lib.rs` 与 `crates/organization-sqlite/src/lib.rs` 使用同一 port contract；parent placement、membership reactivation、idempotency 和 audit 在 adapter transaction 内完成。
- `migrations/019_identity_authorization_foundation.sql:98-143` 建立 organization units/memberships；unit status 为 `active | disabled`，membership status 为 `active | inactive`，唯一键为 tenant/unit/user/type。
- Organization subtree scope 在 `crates/policy/src/application/authorize.rs:280-415` 通过 `OrganizationScopePort` 判断 bound unit 和 resource host active；store/bridge failure 记为 `DenyInternal`，不降级放行。
- 当前没有 time-bounded OrganizationMembership `effective_at/expires_at`；Policy binding 自身有有效期，不能把它误写成组织成员资格有效期。

### 4.3 当前测试证据与 maturity

- `crates/identity-authorization-contracts/src/organization_contract.rs` 覆盖 parent/cycle/depth、tenant isolation、remove/inactive/reactivate、同 key replay/conflict、版本冲突和列表过滤；本次 `organization-sqlite` contract 通过。
- `crates/organization/tests/contract_semantics.rs` 与 `crates/policy/tests/decision_matrix.rs` 覆盖组织 scope exact/subtree、disabled bound/host fail closed。
- 历史 PostgreSQL contract 由 PLAN-0013 CI 记录；本次本机 PG `NOT RUN`。

**R1 foundation / R2 blocked。** 组织关系可以支持当前 RBAC scope，但没有独立 module release identity，也没有通用 group/explicit-share/relation graph；这些不能仅因 Logto/OpenFGA/SpiceDB 有对应功能就推断为当前缺陷。

## 5. Policy 真实实现

### 5.1 Domain / Application / Evaluation

- `crates/policy/src/domain/permission.rs`：PermissionKey grammar 和 catalog active/reserved；未知、retired、malformed key 默认拒绝。
- `crates/policy/src/domain/role.rs:66-340`：system/tenant role、状态、version、permission replacement；system role immutable。
- `crates/policy/src/domain/binding.rs:39-260`：active/revoked binding、effective/expires window、tenant/user/role/scope/version。
- `crates/policy/src/domain/scope.rs:43-260`：Tenant、OrganizationUnit(+subtree)、ResourceType、Resource 四类范围；scope 只缩小不扩大。
- `crates/policy/src/domain/decision.rs:17-155`：15 个有界 `DecisionReason`，`DenyInternal` 永不 allow，policy reference 为稳定 `policy:v1:*` 形式。
- `crates/policy/src/application/authorize.rs:88-475`：`AuthorizationContext` 只有 resolved user/tenant + bounded seven-key compat grant；evaluation 顺序是主体状态、permission catalog、compat bridge、binding/status/window/role/permission/scope；`Authorize` 和 `ExplainDecision` 共用 Engine。
- `apps/business-api/src/platform_authorization.rs:64-207`：认证后重新 resolve external subject，安装 policy context；roles 和客户端 `x-*` headers 不进入 policy authority；所有管理 route 经过 `authorize_permission`。

### 5.2 API / Persistence / Failure

- `apps/business-api/src/auth.rs:41-170`：`AuthenticatedPrincipal` 由 OIDC signed claims 或 dev server config 建立；管理权限是固定七键 compat vocabulary；JWT roles 不是 IAM grant。
- `apps/business-api/src/oidc.rs`：issuer/JWKS/signature/exp/aud/alg 校验，未知 kid refresh；生产 transport/issuer fail closed。
- `apps/business-api/src/routes/iam_admin.rs:177-200`：写请求要求 `Idempotency-Key`；`1241-1270` 的 explain endpoint 只接受当前 caller tenant 内的 target user。
- `migrations/019_identity_authorization_foundation.sql:149-226`：catalog/roles/role_permissions/role_bindings，role visibility、binding status、validity 和 scope CHECK；跨上下文 user/role relation 由 application re-read，不用私有 FK。
- PG/SQLite policy adapters 都有 tenant-scoped queries、store-side caps、版本条件和 idempotency；任何查询/bridge failure 返回 `DenyInternal`，delivery 映射为 retryable 503。
- `create_role` API 当前把 create 与初始 permission set 作为两个 command；PLAN-0013 Stage 8 review 将其记录为可重试但非单提交的 NIT。Wave 1 不把已接受的 C2 兼容限制升级为大范围重构。

### 5.3 当前测试证据与 maturity

- `crates/policy/tests/decision_matrix.rs`：default deny、unknown/retired key、role/binding validity/revocation、tenant visibility、resource kind/org subtree、store failure、explain candidate parity。
- `crates/policy/tests/contract_authorization_fixture.rs:227-340`：cross-tenant binding 与下一次 decision 即时 revocation。
- `crates/identity-authorization-contracts/src/policy_contract.rs`：system role immutability、role/permission replacement、tenant visibility、idempotent replay/conflict、binding validity/revoke 和 store caps；本次 `policy-sqlite` contract 通过。
- `apps/business-api/tests/e2e_admin_bootstrap_sqlite.rs`、`tests/harness/mod.rs:447-590`：bootstrap → membership → role/binding → allow/explain → suspend → stale version → reactivate → revoke 的完整 SQLite chain。
- PostgreSQL adapter contract、IAM E2E 和 perf harness 在 PLAN-0013 的固定 CI run 中通过；本次本机 PG/real integration `NOT RUN`。

**R1 foundation / R2 blocked。** Policy authority 已经是内部 typed domain/application，外部授权引擎只有未来 port 位置，当前不存在 provider adapter 或 remote consistency evidence。

## 6. 认证、租户和公开契约检查

1. OIDC credential/session/token 由外部 IdP 验证；平台将 issuer+subject 绑定到 PlatformUser，membership 与 policy 再确认 tenant access。
2. `AuthenticatedPrincipal`、`AuthorizationContext`、`TenantContext` 尚未组成稳定跨进程 `PrincipalContext` contract；当前仅有请求内 typed context。
3. public IAM DTO 位于 `crates/public-api-contracts/src/iam.rs`，使用 closed/tagged scope enum 和 `deny_unknown_fields`；不泄漏 object key、storage key、bucket 或内部路径。
4. `ResourceTarget` 在 Policy application 中被定义为 trusted resource metadata；当前 IAM explain body 可提供 resource shape，但该 endpoint 是管理诊断而非业务资源写授权。真实业务 owner 尚未接入 resource-loading use case，故不能宣称完整 resource ownership proof。
5. Audit actor 当前区分 User/Bootstrap/Migration；审计中存在 Service 类型映射，但没有可供 API 调用的 `ServiceIdentity`/workload principal，不能把 Bootstrap/Migration 当成已完成 M2M capability。

## 7. Wave 0 inventory 与 gap 预判

| 项 | 事实 | 初步分类 | Wave 1 处理 |
|---|---|---|---|
| Identity/Organization/Policy authority | 三个独立 crate + PG/SQLite adapters + composition bridge | KEEP | 不改变 authority |
| Cross-tenant/default-deny/revocation | 当前代码和 adversarial/fixture 测试覆盖 | C0=0（当前证据） | 复跑并记录 |
| Expired tenant/org membership | Tenant membership 无 expired；org membership remove=inactive；binding 有 expiry | C2 SHOULD | 不增加新 lifecycle；写入 backlog |
| Service/M2M/workload identity | 当前无真实 consumer 或 public contract | C3 DEFER | 不创建 speculative model |
| ReBAC/group/explicit share | 当前只支持 typed RBAC + resource/org scopes | C2/C3 | 只记录 future port/consumer trigger |
| Explain semantics | binding path 与 Authorize 共用 Engine；target-user explain 不模拟另一个 token 的 compat claim | C2 contract clarification | 文档化，不改 policy engine |
| create role + initial grants | 两个可重试 command，历史 review 已接受 | C2 | 不扩大 Wave 1 |
| R2 package/release | 无独立 SemVer/manifest/digest/artifact/consumer build evidence | C1 R2 blocker | 标记 R2 BLOCKED，交由 packaging/R2 pilot |

## 8. Baseline conclusion

当前源码支持的结论是：Identity、Organization、Policy 已从 PLAN-0013 的基础实现进入可验证的 **R1 foundation**，并且在本地 fake/domain 与 SQLite adapter 证据上保持 default-deny、tenant-scoped、versioned、idempotent 和 auditable 语义。它们仍不是 R2 独立发布模块。

没有发现需要在 Wave 1 直接引入外部授权 runtime、通用 relation graph、service identity 或巨大 PrincipalContext 的 C0 安全缺口。配套的正式 conformance review 已固定六个参考项目的 tag/commit，并逐项记录 ADOPT/ADAPT/KEEP/DEFER/REJECT 决策、17 个 adversarial scenario 的现有证据、NOT RUN 项与 C2/C3 边界。
