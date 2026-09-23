# SaaS Module Independent Release Standard

> 文档类型：Standard  
> 状态：Proposed  
> 日期：2026-09-23  
> 适用范围：Platform Capability Module、Business Module、AI/Agent Module、外部服务 Adapter  
> 目标：任何功能模块可独立版本化/发布，并可在其他产品或部署形态中承担同一能力

## 1. 核心定义

### 1.1 Module

Module 是具有明确 Authority、公开合同、依赖和生命周期的能力单元。

Module 不是：

- 一个页面；
- 一组数据库表；
- 一个 Handler；
- 一个 Cargo crate 名；
- 一个微服务进程；
- 一个供应商 SDK wrapper。

### 1.2 Independent Release

“可独立发布”表示模块：

- 有独立 module identity/version；
- 有可单独发布的 contract artifact；
- 有独立 compatibility/migration 规则；
- 可以在不修改消费者业务代码的前提下替换 implementation；
- 可以被其他 Rust 应用嵌入；
- 需要跨语言/跨进程时，可以由 remote adapter 承担同一 Port。

它不要求当前就拆成独立仓库或容器。

### 1.3 Release forms

每个模块允许三种等价承载：

1. **Embedded Rust**：以 crate/package 链接进宿主；
2. **Standalone Service**：通过 HTTP/gRPC/Event contract 提供能力；
3. **External Provider Adapter**：由成熟 OSS/SaaS 服务承担能力。

消费者必须面向同一个 Application Port/Contract。

## 2. 模块类别

### Platform Capability Module

与具体业务无关，可跨产品复用：

- tenancy；
- identity mapping；
- authorization；
- audit；
- messaging；
- object storage；
- secrets；
- commercial/metering；
- notification；
- observability；
- generic job；
- module registry。

### Business Module

拥有具体业务 Bounded Context：

- Contract；
- Customer；
- Project；
- Finance；
- Approval；
- Document；
- Legal；
- People 等。

### AI/Agent Module

面向 Agent 产品能力：

- Agent Registry；
- Model Policy；
- Tool/MCP Registry；
- Knowledge Binding；
- Runtime Binding；
- Eval；
- Capability Grant。

三类 Module 使用同一发布/依赖标准，但 Authority 不互相替代。

## 3. 强制发布构成

每个新模块必须能够形成如下发行单元：

```text
<module>
├── module.manifest
├── contracts/
│   ├── commands
│   ├── queries
│   ├── events
│   └── resources
├── core/
│   ├── domain
│   ├── application
│   └── ports
├── adapters/
│   ├── persistence
│   ├── remote
│   └── provider
├── migrations/
├── contributions/
│   ├── ui
│   ├── semantic
│   ├── agent
│   └── policy/capability requirements
├── contract-tests/
├── CHANGELOG
└── release metadata
```

物理目录可以不同，但这些责任必须可定位。

## 4. Manifest 要求

复用现有 `BusinessModuleManifest`，不得再创建第二套 SaaS manifest。

至少包含：

- `module_id`；
- `module_version`；
- `manifest_schema_version`；
- `owned_bounded_contexts`；
- required/optional platform capabilities；
- published commands/queries/events；
- resource kinds；
- data classification；
- migration namespace；
- semantic/UI/agent contributions；
- module dependencies；
- platform compatibility。

新增 SaaS capability 时，如现有字段不足，应扩展该 manifest schema，而不是另建配置权威。

## 5. 依赖规则

### SAAS-MOD-001 — Core no concrete adapter dependency

Domain/Application 不得依赖：

- sqlx row；
- Axum request；
- S3 SDK；
- OpenFGA SDK；
- OpenMeter SDK；
- vendor DTO；
- other module private repository。

### SAAS-MOD-002 — Cross-module public contract only

Module A 调用 Module B，只能依赖：

- B contracts；
- B published port；
- ResourceRef；
- versioned event/projection。

### SAAS-MOD-003 — Capability dependency, not provider dependency

Manifest 声明：

```text
authorization.check >= v1
secret.resolve >= v1
metering.record >= v1
```

不得声明：

```text
openfga
infisical
openmeter
```

供应商属于 composition/runtime binding。

### SAAS-MOD-004 — No cross-module write transaction

每个模块只提交自己的权威状态。跨模块流程使用 Command/Event/Saga/Process Manager；Outbox/Inbox 保证可靠传播。

### SAAS-MOD-005 — Tenant context is mandatory

所有业务/public capability 调用必须能接受可信 tenant/principal context 或明确声明 system-scope。

客户端提供的 `tenant_id` 不能单独成为授权依据。

## 6. Contract 要求

公开 Contract 必须：

- stable ID；
- explicit version；
- bounded payload；
- tenant/principal semantics；
- idempotency semantics；
- error taxonomy；
- timeout/cancellation semantics；
- classification；
- compatibility policy。

跨语言 surface 使用 OpenAPI/JSON Schema/Protobuf 等语言中立合同；Rust DTO 只是其中一种生成/实现形式。

## 7. Embedded 与 Remote 等价

每个可远程化能力必须定义同一 Port：

```text
Caller
  |
  v
CapabilityPort
  |
  +-- InProcessAdapter
  |
  +-- RemoteServiceAdapter
  |
  +-- ExternalProviderAdapter
```

契约测试必须能针对多个 adapter 执行同一 behavior suite。现有 `identity-authorization-contracts` 和 `document-persistence-contracts` 是标准范式。

禁止消费者：

- 区分“本地还是远程”后改变业务规则；
- 直接访问 provider SDK；
- 根据 provider-specific error 编写领域判断。

## 8. Versioning

### Module version

采用 SemVer。Breaking public contract 必须 major bump。

`BusinessModuleManifest.module_version` 是功能模块的 canonical release version。当前 workspace 的 `version.workspace = true` 只能代表仓库/产品版本，**不得**被解释为模块独立版本。进入 R2 前，模块必须建立独立 release version（或明确的 module-version → crate-version 映射），并由 release artifact/manifest/digest 固定。一个模块由多个 crate 组成时，这些 crate 必须作为一个 module release bundle 有可审计的版本映射。

### Contract version

Command/Query/Event/Resource 可拥有独立 schema version。Module patch/minor 不应无故破坏 public schema。

### Compatibility

Manifest 必须声明：

- platform min/max；
- required capability version；
- module dependency version；
- migration compatibility。

### Package digest

编译/发布产物必须有 deterministic digest。部署、升级、审计和回滚引用 digest，不只引用可变 tag。

## 9. Persistence 与 Migration

每个拥有持久事实的模块必须有独立 migration namespace。

规则：

- migration 只改 owner schema/data；
- 禁止直接迁移其他模块 private table；
- downgrade 是否支持必须明确；
- uninstall != purge；
- purge 是独立授权操作；
- adapter schema 可以不同，但 behavior contract 必须相同。

PostgreSQL 是生产权威；SQLite 只有模块声明支持时才是 local adapter，并执行相同 contract suite。

## 10. Security

每个模块必须声明：

- required permission/policy；
- resource scope；
- data classification；
- secrets requirements；
- audit actions；
- tenant isolation strategy；
- privileged operation confirmation/approval。

Unknown permission/capability/provider state 必须 fail closed。

模块 manifest/contribution 是声明，绝不自动授予权限。

## 11. Audit、Usage 与 Observability

模块写操作至少产生可关联：

- tenant_id；
- principal_id/service identity；
- module_id/version；
- command/action；
- resource ref；
- correlation/causation；
- outcome；
- timestamp。

需要计量的模块额外产生标准 `UsageEvent`。

Telemetry 可经 OpenTelemetry/Langfuse 导出，但不得替代 Audit/Runtime authority。

## 12. Failure model

每个模块公开说明：

- unavailable；
- timeout；
- retryable；
- terminal；
- conflict；
- authorization denied；
- outcome unknown（如存在外部副作用）；
- degradation policy。

Provider 不可用时是否允许 fallback 必须是平台 policy，而不是 adapter 静默决定。

## 13. Release artifact

模块发布至少包含：

- module package；
- manifest；
- public schema；
- digest；
- migration set；
- changelog；
- compatibility matrix；
- contract-test result；
- security/license metadata；
- SBOM/供应链信息（生产发行时）。

推荐 release identity：

```text
module_id
module_version
manifest_schema_version
package_digest
source_revision
contract_digest
migration_revision
build_toolchain
```

## 14. 独立发布成熟度

### R0 — Internal only

只有代码目录，没有稳定 contract。不得称为可发布模块。

### R1 — Isolated

边界清楚、无 private cross-module dependency，有 Port/contract tests。

### R2 — Independently versioned package

有 manifest/SemVer/digest/migration/compatibility，可发布到独立 artifact registry。**所有 Platform/Business Module 的目标最低等级。**

进入 R2 前必须完成 **Reference Conformance Review**：

- 至少两个同领域成熟参考项目；
- 至少一个是真实生产型 OSS/平台而不是 starter；
- 固定 reviewed revision/release；
- 检查源码、测试、失败/安全/恢复语义，不只读 README；
- C0=0、C1=0；
- 独立 reviewer 对固定 SHA 给出 PASS；
- review 使用 `docs/templates/REFERENCE_CONFORMANCE_REVIEW_TEMPLATE.md`；
- 对齐结论允许 KEEP/DEFER/REJECT，不要求复制参考项目。

### R3 — Deployment-independent

存在 remote adapter，模块可以独立进程运行，消费者无需改业务逻辑。

### R4 — Cross-product reusable capability

可被 business-platform、jarvis-rs、hdbot 或其他产品通过稳定合同消费；语言/进程无关。

不是所有模块必须立即实现 R3/R4，但设计不得阻断升级到 R3/R4。

## 15. Rust workspace 推荐物理模式

现有仓库不要求立即 mass rename。新增/拆分时优先：

```text
crates/<module>                 # domain + application + ports
crates/<module>-contracts       # 只有跨包/跨产品消费者时拆
crates/<module>-postgres
crates/<module>-sqlite          # 若明确支持 local
crates/<module>-<provider>      # 外部 provider adapter
```

如果一个模块需要独立服务，再增加：

```text
apps/<module>-service
```

而不是把 Domain 移进 HTTP app。

## 16. 外部 OSS 的接入规则

采用外部 OSS 时：

1. 先定义本平台 Port/Contract；
2. provider adapter 放在 infrastructure/integration 层；
3. external ID 与 internal stable ID 显式映射；
4. provider schema 不能变成 Domain schema；
5. provider outage/fallback 明确；
6. provider migration/export 有 runbook；
7. 许可证和版本单独登记；
8. 关键安全决策仍由平台 fail closed。

## 17. Fitness Functions

后续 architecture-check 至少增加：

- Platform Core 不依赖 concrete business crates；
- module core 不依赖 provider SDK；
- cross-module persistence import fail；
- module manifest 与发布 contract 一致；
- duplicate module/contract ID fail；
- unsupported capability/version fail；
- contract suite 对每个 declared adapter 执行；
- module uninstall 不触发 purge；
- tenant-required command 无 trusted tenant context fail；
- provider adapter 不得返回 provider-specific domain decision。

## 18. 现有模块评估

| Module | 当前等级 | 主要缺口 |
|---|---:|---|
| identity | **R1** | Domain/Application/Port 与双数据库行为契约强；但仍 `version.workspace=true`，无独立 module manifest/release artifact/remote surface |
| organization | **R1** | 同上 |
| policy | **R1** | Domain/Application/Port 已成型；无独立 module release，external engine adapter/remote surface 可后置 |
| document | **R1** | Domain/ports/adapters 较成熟；缺 module manifest、独立 SemVer/artifact/release metadata |
| document-processing | **R1** | contracts/adapters/durable semantics 较强；仍缺独立 module release identity/artifact |
| audit | **R1** | 已有 owner/adapters；缺独立 capability manifest/release artifact |
| object-storage | **R1** | provider-neutral seam 已有；缺独立 capability version/artifact |
| messaging | **R1** | Outbox/Inbox 已有；published capability/version/release artifact 未正式登记 |
| observability | R1 | 主要为平台内部 capability |
| customer/contract/project/finance/approval | R0/R1 不等 | 需由真实业务切片逐个提升 |
| commercial/entitlement | 尚无 | 新建 owner |
| module registry | compiler foundation | 缺 runtime registry/install executor |

## 19. 禁止事项

- 为“独立发布”立即拆几十个微服务；
- 为“通用”创建万能 Entity/Field/JSON 元模型；
- 每个模块自建一套 Tenant/User/Role；
- 业务模块直接使用 IdP role 当最终授权；
- Agent 绕过 Application API；
- 把 OpenFGA/OpenMeter/Infisical 的对象模型复制成业务领域模型；
- provider SDK 类型穿透 public contracts；
- 以 UI 隐藏代替后端授权；
- 共享数据库视为共享所有权。
