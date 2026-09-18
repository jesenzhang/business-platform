# PLAN-0013: Identity and Authorization Foundation

> Status: Active (PLAN-0013 implementation in progress on branch `feat/PLAN-0013-identity-authorization-foundation`; Completion Definition unchanged)  
> Revision: 0  
> Date: 2026-09-18  
> Owner: Platform Foundation / Identity & Policy  
> Base SHA: b9eadf8f1b2b46c88f40f2defc88ffcb8bdb0b34  
> Integration Mode: review branch → independent review → merge to main  
> Stop Policy: blockers-only  
> Architecture: ADR-0024 + IDENTITY_AND_AUTHORIZATION_ARCHITECTURE.md

## 1. Goal

在不重建登录/凭证系统的前提下，建立 Business Platform 内部最小、可生产使用的用户与业务授权闭环，为后续 Contract 真实业务切片和 PLAN-0006 Agent Capability 提供统一权威。

完成后系统必须能够：

~~~text
External OIDC authenticated subject
  -> PlatformUser
  -> TenantMembership
  -> OrganizationMembership
  -> RoleBinding
  -> Permission + ResourceScope
  -> Policy Decision
  -> Application Use Case
  -> Audit
~~~

## 2. Why this is next

v0.1 已经证明生产 OIDC、tenant/user principal、Document、Governance、AI provider、恢复与可观测性，但授权仍不完整：

- crates/identity 仍为 TODO 骨架；
- crates/organization 仍为 TODO 骨架；
- 当前管理面只使用固定 ManagementPermission enum；
- OIDC roles 已进入 principal，但没有完整 Role/Permission/Binding 生命周期；
- 没有 User/Tenant Membership 管理 API；
- 没有部门/组织作用域；
- 没有 Contract 等真实业务资源的数据级权限；
- PLAN-0006 的 Capability Grant 不能建立在空的业务授权模型之上。

因此先补授权闭环，再做真实业务垂直切片，比继续扩张通用 Agent/Plugin Runtime 更低风险。

## 3. Non-goals

本计划不实现：

- 用户名/密码/MFA/refresh token/OAuth Authorization Server；
- SCIM/LDAP 全量同步；
- 完整 HR、岗位编制、绩效、薪酬；
- 通用 Zanzibar/ReBAC 图；
- 任意 ABAC/Policy 脚本语言；
- Contract 正式业务模型；
- Agent Workspace/Capability Grant；
- Marketplace/dynamic plugin；
- 跨租户共享；
- UI 设计系统重做。

## 4. Authoritative boundaries

### External IdP

继续拥有 authentication/credential issuance。现有 OIDC validator 不重写。

### Identity

拥有：

- PlatformUser；
- external identity mapping；
- TenantMembership；
- user lifecycle/status。

### Organization

只拥有授权需要的最小 OrganizationUnit/OrganizationMembership。

### Policy

拥有：

- PermissionDefinition；
- RoleDefinition；
- RolePermission；
- RoleBinding；
- ResourceScope；
- PolicyDecision。

### Business owner context

拥有最终业务状态授权。例如未来 Contract Owner 决定“已归档合同是否可修改”，Policy 不复制该状态机。

## 5. Invariants

1. issuer + subject 稳定映射到一个 PlatformUser。
2. 没有 Active TenantMembership 就没有该 tenant 业务权限。
3. RoleBinding 不得跨 tenant 生效。
4. 未知 Permission/Role/Scope fail closed。
5. 外部 IdP roles 只能通过显式映射影响平台 RoleBinding，不能直接获得任意业务权限。
6. ResourceScope 只能缩小访问范围。
7. Delivery/Handler 不承载散落授权业务规则。
8. 权限变更必须审计。
9. 被停用用户的历史业务引用不能被删除。
10. PostgreSQL 是生产权威；SQLite 若实现必须满足相同 contract semantics。

## 6. First permission catalog

把现有管理权限纳入统一 catalog：

~~~text
audit.read
integrity.read
integrity.scan
repair.dry-run
repair.execute
repair.approve
repair.cancel
~~~

再为下一 Contract 垂直切片预留稳定键但不实现业务逻辑：

~~~text
document.read
document.review
contract.read
contract.create
contract.update
contract.review
contract.archive
~~~

“预留稳定键”不代表能力已交付；没有对应 Application Use Case 时授权成功也不能产生业务操作。

## 7. Work packages

| ID | Scope | Required evidence |
|---|---|---|
| WP-00 | 同步 Identity/Authorization Baseline、Threat Model、OpenAPI 设计 | 文档审查 |
| WP-01 | PlatformUser + external identity mapping + TenantMembership domain/application | pure domain tests |
| WP-02 | 最小 OrganizationUnit + OrganizationMembership | hierarchy/scope tests |
| WP-03 | Permission/Role/RolePermission/RoleBinding/ResourceScope | domain invariant tests |
| WP-04 | Policy evaluator + Authorize / ExplainDecision contracts | matrix/property tests |
| WP-05 | PostgreSQL adapter + migrations；SQLite contract adapter 若保留 | adapter contract tests |
| WP-06 | 把现有 ManagementPermission 接到统一 Policy，保持 API 兼容 | governance regression |
| WP-07 | User/Membership/Role/Binding 管理 REST contracts | OpenAPI + negative tests |
| WP-08 | Business Console 最小用户/角色/绑定管理页 | frontend contract/build tests |
| WP-09 | Audit/Outbox/observability/redaction | audit/security tests |
| WP-10 | 真实 OIDC principal → Policy → Governance API E2E | PostgreSQL E2E |
| WP-11 | Contract 下一切片授权契约与 fixture，不实现 Contract domain | compatibility/fixture tests |
| WP-12 | Architecture Fitness、Runbook、Completion Audit | CI evidence |

## 8. Application contracts

最低合同：

~~~text
ResolveAuthenticatedUser
GetUser
ListUsers
ChangeTenantMembershipStatus

CreateOrganizationUnit
UpdateOrganizationUnit
AddOrganizationMember
RemoveOrganizationMember
ListOrganizationTree

CreateRole
UpdateRole
SetRolePermissions
BindRole
RevokeRoleBinding
Authorize
ExplainDecision
~~~

具体 Rust API 可调整，但必须保持 owner/contract 语义。

## 9. Authorization matrix

至少覆盖：

| Case | Expected |
|---|---|
| valid OIDC + active membership + role permission | allow |
| valid OIDC + no tenant membership | deny |
| suspended membership | deny |
| role in another tenant | deny |
| org-scoped role accessing sibling org resource | deny |
| expired/revoked binding | deny |
| unknown permission | deny |
| client-supplied role/permission header | ignored/deny |
| built-in management role permission | same behavior as current API |
| role revoked after token issue | deny on next policy evaluation |

## 10. Management bootstrap

系统必须能够在“尚无平台 RoleBinding”的首次部署中建立初始管理员，但不能形成永久万能后门。

允许的实现：

- server-controlled bootstrap mapping；
- one-time migration/bootstrap command；
- trusted IdP group → explicit platform binding migration。

必须满足：

- 仅明确环境/tenant；
- 可审计；
- 可撤销；
- 生产默认关闭重复 bootstrap；
- 不接受请求端自报管理员。

## 11. API and UI

API 继续使用现有 OIDC authentication boundary。

Business Console 只需要最小管理体验：

- Users；
- Tenant memberships；
- Organization units；
- Roles；
- Role bindings；
- Permission read view；
- decision explanation（仅管理员）。

不做完整 IAM 产品。

## 12. Migration / compatibility

1. 不修改现有 OIDC token validation 语义。
2. AuthenticatedPrincipal.user_id 在兼容期可继续来自可信 claim，但进入 Application 前必须 resolve/match PlatformUser。
3. 当前 ManagementPermission 先注册为 built-in catalog，再逐步改用 Authorize。
4. 所有现有 Governance negative tests 必须保持。
5. 不删除已有 audit actor/user references。

## 13. Security

必须补自动化验证：

- cross-tenant IDOR；
- role/binding mass assignment；
- disabled membership；
- unknown permission；
- privilege escalation by role edit；
- self-grant admin；
- stale binding/cache；
- OIDC role spoof/mapping；
- org scope escape；
- audit sensitive-data leakage。

## 14. Quality attributes

初始预算需实测记录：

- Authorize P95；
- list users/roles keyset pagination；
- policy cache hit/miss；
- revoke propagation delay；
- OIDC→resolve principal overhead。

禁止以缓存换取跨租户或撤销正确性。

## 15. Architecture fitness

至少新增规则：

1. business handlers 不直接解析角色字符串；
2. Domain 不依赖 JWT/OIDC SDK；
3. Identity/Policy 不依赖 Contract/Finance 等具体领域；
4. 业务模块不能直接读取 RoleBinding 表绕过 Policy port；
5. cross-tenant binding fixture 必须失败；
6. Permission stable key collision fail closed；
7. Agent/Workspace 不在本计划引入；
8. external IdP credential issuance 不进入仓库。

## 16. Completion definition

PLAN-0013 只有满足以下条件才成为 Accepted Candidate：

- PlatformUser/TenantMembership/Organization/Role/Permission/Binding/Scope 已实现；
- 统一 Authorize 可替代当前 Governance 固定权限路径且无行为回退；
- 最小管理 API 与 Console 可用；
- OIDC→PlatformUser→Policy E2E 通过；
- revoke/suspend/cross-tenant/unknown permission 等负例全绿；
- PostgreSQL 真实 CI 通过；
- SQLite 若声明支持则 contract tests 通过；
- Audit/Outbox 与现有一致性规则通过；
- OpenAPI、Architecture Fitness、fmt/check/clippy/test/security gates 通过；
- Completion Audit 记录 PASS/NOT RUN 与环境限制；
- 不实现 Contract domain、Workspace、Agent Capability 或 OAuth server。

Accepted Candidate 之后停止，等待独立审阅和显式主干集成指令。

## 17. After PLAN-0013

下一运行时计划必须是 **Contract Business Vertical Slice**，用真实业务验证本计划，而不是继续设计新的通用 IAM 抽象。

首个 Contract flow：

~~~text
Contract List
→ Contract Detail
→ Document Revision
→ Preview
→ AI Extract
→ Evidence
→ Review
→ Apply Candidate
→ Contract Version Update
~~~

它必须证明：tenant/org/resource authorization、Document 权限继承/收紧、证据 revision binding、审计、UI/API 一致。

PLAN-0006 Revision 1 仅在该真实业务切片形成稳定 Published Query/Tool contract 后激活。
