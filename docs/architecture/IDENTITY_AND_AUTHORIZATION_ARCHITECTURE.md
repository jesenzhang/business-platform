# Identity and Authorization Architecture

> 文档 ID：ARCH-IAM-001  
> 版本：1.0  
> 状态：Baseline  
> 生效日期：2026-09-18  
> 适用范围：人员身份映射、租户成员、组织、角色、权限、资源范围、业务授权、Agent 委托

## 1. 目的

本 Baseline 将 ADR-0024 的“认证外置、授权内置”落实为可实现边界。

当前系统已经能够以生产 OIDC/JWT 建立可信的 AuthenticatedPrincipal，包含 tenant_id、user_id、subject、roles 和少量管理权限；但 crates/identity 与 crates/organization 仍为骨架，尚不存在完整 User/Tenant Membership/Role/RoleBinding/Resource Policy 管理。

因此下一阶段不是重新实现登录系统，而是补齐业务平台内部授权。

## 2. 核心边界

~~~text
External OIDC IdP
  owns:
    login / password / MFA
    credential issuance
    session / refresh
    federation
        |
        v
Business Platform Authentication Boundary
  validates:
    signature / issuer / audience / expiry
    trusted claims
        |
        v
Identity & Tenant Membership
        |
        v
Organization + Policy
        |
        v
Business Application Authorization
        |
        v
Domain invariant / transaction / audit
~~~

平台不得成为人员 OAuth/OIDC Authorization Server。私有化部署如需本地 IdP，应作为独立组件交付。

## 3. 数据所有权

### 3.1 External IdP

拥有外部认证事实：

- issuer + subject；
- 登录凭证；
- MFA；
- IdP session；
- refresh token；
- federation/account recovery。

这些数据不进入业务领域模型。

### 3.2 Identity and Access

拥有平台身份映射和租户关系：

~~~text
PlatformUser
- user_id
- issuer
- subject
- display/profile references
- lifecycle status
- created_at / updated_at

TenantMembership
- tenant_id
- user_id
- status
- joined_at
- suspended_at
- source
- version
~~~

issuer + subject 必须在平台内稳定映射到 user_id。OIDC Token 中的临时显示属性不能取代平台资源 ID。

### 3.3 Organization

PLAN-0013 只实现授权所需的最小组织模型：

~~~text
OrganizationUnit
- org_unit_id
- tenant_id
- parent_org_unit_id?
- type
- name
- lifecycle status

OrganizationMembership
- tenant_id
- user_id
- org_unit_id
- membership_type
- status
~~~

完整 HR、岗位编制、绩效、工资和员工主数据不进入该计划。

### 3.4 Policy

Policy 拥有通用授权定义和绑定：

~~~text
PermissionDefinition
RoleDefinition
RolePermission
RoleBinding
ResourceScope
PolicyDecision
~~~

业务上下文继续拥有业务状态授权规则。Policy 不能替代 Contract/Finance/Approval 等领域不变量。

## 4. Permission

Permission 使用稳定、版本可治理的业务动作标识，例如：

~~~text
audit.read
integrity.read
integrity.scan
repair.dry-run
repair.execute

document.read
document.review

contract.read
contract.create
contract.update
contract.review
contract.archive
~~~

本 Baseline 不要求一次定义全平台所有权限。权限应由已实现业务能力贡献并进入统一 catalog，未知 permission 必须 fail closed。

现有 ManagementPermission 可作为兼容输入，但不得永久成为第二套授权模型。

## 5. Role 与 Binding

Role 是权限集合，不直接拥有业务数据：

~~~text
RoleDefinition
- role_id
- tenant_id or system scope
- stable_key
- display_name
- status
- version

RoleBinding
- binding_id
- tenant_id
- principal/user id
- role_id
- scope
- effective_at / expires_at?
- status
- version
~~~

RoleBinding 必须显式带作用域。首版支持：

- Tenant；
- Organization Unit；
- Resource Type；
- 单一 Resource（仅确有业务需要时）。

不先实现任意 Policy DSL 或无限 ABAC 表达式。

## 6. 授权计算

授权决策必须由服务端完成：

~~~text
Authenticated Principal
  + Active TenantMembership
  + Active RoleBindings
  + Resource Scope
  + Organization relation
  + Resource classification / ownership attributes
  + Business state rule
  + Requested action
        =
Policy Decision
~~~

规则：

1. 默认拒绝；
2. tenant 不匹配立即拒绝；
3. suspended/disabled membership 拒绝；
4. 未知 role/permission/scope 拒绝；
5. 资源必须由可信 Repository/Application Query 重新加载，不能信任客户端属性；
6. 业务状态规则由资源 Owner Context 最终确认；
7. 决策必须能关联 request/trace/audit；
8. 权限缓存不能跨 tenant，撤销后必须在明确 SLA 内失效。

## 7. OIDC Claims 的使用

OIDC claims 可以提供：

- issuer / subject；
- tenant claim（经过受信映射时）；
- user id claim（兼容当前实现）；
- display profile；
- enterprise group/role hints。

但外部 roles 不能成为业务平台唯一授权权威。

允许的模式：

~~~text
OIDC group/role
   -> explicit trusted mapping
   -> Platform RoleBinding
   -> Policy Decision
~~~

禁止：

- 客户端 header 注入权限；
- 任意字符串 role 自动获得业务权限；
- Token 中存在 role 就绕过 TenantMembership；
- Handler 直接解析 JWT 决定领域写权限。

## 8. 资源与数据级权限

企业业务需要超过“接口级 RBAC”的控制。

最小 Resource Scope 至少能表达：

- tenant-wide；
- organization-unit；
- owned-by / responsible-by；
- explicit shared resource；
- resource type；
- concrete resource id（少量场景）。

字段级策略由业务模块声明可公开分类，Policy 决策给出允许字段集合或 policy reference。敏感字段不得由前端隐藏按钮代替服务端过滤。

Contract 后续切片至少需要验证：

- 用户只能看到授权范围内的合同；
- Document/Attachment 权限不能比所属 Contract 更宽；
- Legal/Finance 专业字段可以比通用 Contract Summary 更窄；
- 跨部门共享必须有明确来源、范围和审计。

## 9. Agent 授权

Agent 不建立新的业务权限体系：

~~~text
Current User Authority
    >= Delegated Agent Authority
        >= Task Capability Grant
~~~

PLAN-0006 实现后：

1. Tool Catalog 先声明所需 Permission/Resource Scope；
2. 每个 Turn/Invocation 按当前用户重新解析；
3. Capability Grant 只能缩小、不能扩大用户权限；
4. 权限撤销、成员停用、资源版本变化后 fail closed；
5. Tool 调用仍由业务 Application Use Case 进行最终授权。

## 10. 管理面

最小管理能力：

- list/get users；
- suspend/reactivate tenant membership；
- list/create/update/disable roles；
- attach/detach permission；
- assign/revoke role binding；
- list organization units / memberships；
- explain authorization decision（受管理权限保护）；
- audit role/membership/policy changes。

所有写入要求幂等/版本控制/审计。高风险平台管理员授权不得静默自授。

## 11. API 与 Application 边界

Delivery 层只做认证、输入验证和调用 Application Use Case。

建议 Application contracts：

~~~text
Identity
- ResolvePrincipal
- GetUser
- ListUsers
- ChangeTenantMembershipStatus

Organization
- CreateOrganizationUnit
- MoveOrganizationUnit
- AddOrganizationMember
- RemoveOrganizationMember
- ListOrganizationTree

Policy
- DefineRole
- ChangeRolePermissions
- BindRole
- RevokeRoleBinding
- Authorize
- ExplainDecision
~~~

具体 API 路径由 PLAN-0013 实现决定，必须进入 OpenAPI/public contracts。

## 12. 持久化与一致性

- PostgreSQL 为生产权威；
- SQLite 仅作为本地适配器时必须保持相同 contract semantics；
- User、Membership、Role、Binding、Organization Unit 使用 tenant-scoped keys；
- 授权变更和 Audit/Outbox 按既有一致性规则处理；
- 外部 IdP 删除/停用不会自动删除业务历史 user reference；
- Uninstalled != Data Purged 同样适用于身份与角色历史。

## 13. 迁移策略

从现有实现渐进迁移：

1. 保留当前 OIDC validator 与 AuthenticatedPrincipal；
2. 引入 PlatformUser/TenantMembership；
3. 把当前固定 Management Permission 注册为 built-in PermissionDefinition；
4. 引入 Role/RoleBinding/Policy evaluator；
5. Management API 改为调用统一 Authorize；
6. 再接入 Document 与 Contract 业务权限；
7. PLAN-0006 在此基础上实现 Capability Grant。

不得用大爆炸方式重写认证。

## 14. 安全门禁

自动测试至少证明：

- invalid issuer/audience/signature/expiry fail closed；
- unknown user/membership policy；
- cross-tenant role binding 无效；
- disabled membership 无权限；
- unknown permission fail closed；
- role revoke 后不可继续授权；
- org scope 不泄漏同 tenant 其他组织资源；
- resource scope 不能被请求参数扩大；
- Management Permission 兼容层与统一 Policy 结果一致；
- Agent delegated capability 不超过当前用户。

## 15. 非目标

本 Baseline 不要求近期实现：

- 自建 OAuth/OIDC credential issuer；
- 密码/MFA/账号恢复；
- SCIM 全量同步；
- 完整 HR Employee/Position/Payroll；
- 任意 Policy scripting/DSL；
- Zanzibar/ReBAC 图数据库；
- 通用 ACL 给所有表自动套规则；
- 跨租户共享；
- 长期 Agent super-token。

## 16. 实施顺序

PLAN-0013 实现最小 Identity/Authorization 闭环。

完成后，首个 Contract 业务切片必须成为权限模型的真实消费者；只有经过真实业务验证后才扩大抽象。

之后 PLAN-0006 Revision 1 复用同一 Authorize/Resource Scope，为 Agent 生成更窄的 task-scoped Capability Grant。
