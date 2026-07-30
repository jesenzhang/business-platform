# 业务能力与 Bounded Context Map

> 文档 ID：ARCH-BC-001  
> 版本：1.0  
> 状态：Baseline  
> 生效日期：2026-07-30  
> 适用范围：服务端业务边界、模块划分、数据所有权与跨上下文协作

## 1. 目的

本文从战略 DDD 角度定义业务能力、Bounded Context、统一语言和上下文关系。

边界首先是业务、数据和治理边界，不自动等于：

- Rust crate；
- 数据库 schema；
- HTTP 服务；
- Worker；
- 消息 Topic；
- 前端菜单。

代码和部署可以随阶段演进，但不得破坏本文定义的数据所有权与业务职责。

## 2. 领域分类

### 2.1 核心域

直接承载企业业务价值和差异化规则：

- Contract Management；
- Approval Management；
- Document Intelligence；
- Project Management；
- Finance Operations。

### 2.2 支撑域

支撑核心业务并包含稳定业务语义：

- Customer Management；
- Document Management；
- Organization；
- Identity and Access；
- Policy；
- Audit；
- Notification。

### 2.3 通用平台域

提供可复用执行与技术能力，不拥有具体业务规则：

- Durable Task Execution；
- Integration Gateway；
- AI Provider Integration；
- Object and Artifact Storage；
- Messaging；
- Observability。

## 3. 上下文总览

```text
Identity and Access ───────┐
Organization ──────────────┼────→ 所有业务上下文的调用上下文
Policy ────────────────────┘

Customer ─────┐
              ├──→ Contract ───→ Approval ───→ Finance
Project ──────┘        │              │
                        │              └──→ Notification
                        ▼
                 Document Management
                        │
                        ▼
                 Document Intelligence
                        │
                        └──→ 生成候选建议，回到业务用例正式应用

所有上下文 ───→ Audit
业务用例 ─────→ Durable Task Execution
外部系统 ─────→ Integration Gateway / Anti-Corruption Layer
Web / Agent ──→ 同一 Application API
```

## 4. Identity and Access Context

### 职责

- 用户身份引用；
- 登录主体和会话语义；
- 服务身份；
- 身份委托；
- 认证结果向内部调用上下文的转换。

### 拥有

- Principal；
- Subject Mapping；
- Service Identity；
- Delegation Grant；
- Authentication Session，若由本系统管理。

### 不拥有

- 组织结构；
- 业务角色含义；
- 合同、项目等资源权限规则；
- 外部身份供应商内部用户模型。

### 对外能力

- ResolvePrincipal；
- ValidateDelegation；
- RevokeSession；
- 发布身份状态变化事件。

## 5. Organization Context

### 职责

- 企业、部门、团队和岗位关系；
- 组织成员关系；
- 组织层级与有效期；
- 业务上下文使用的组织引用。

### 拥有

- OrganizationUnit；
- Membership；
- Position；
- ReportingRelation。

### 不拥有

- 登录凭证；
- 业务资源权限规则；
- 客户组织的业务资料。

## 6. Policy Context

### 职责

- 跨业务共用的授权决策框架；
- Role、Permission、Attribute 和 Policy 的稳定语义；
- 对业务上下文提供授权判定能力。

### 约束

业务上下文拥有“什么操作在什么状态下允许”的业务规则；Policy 提供统一授权机制，不应吞并业务不变量。

## 7. Customer Management Context

### 职责

- 客户主体；
- 联系人和客户关系；
- 客户生命周期；
- 客户分类和业务状态。

### 拥有

- Customer；
- CustomerContact；
- CustomerRelationship；
- CustomerStatus。

### 不拥有

- 合同生命周期；
- 项目交付状态；
- 客户上传文档的文件版本。

### 对外能力

- CreateCustomer；
- UpdateCustomerProfile；
- ChangeCustomerStatus；
- GetCustomerSummary。

## 8. Contract Management Context

### 职责

- 合同身份和版本；
- 合同条款与正式字段；
- 合同生命周期；
- 合同变更、签署、履约和终止规则；
- 对 AI 字段建议执行正式业务校验和应用。

### 拥有

- Contract；
- ContractVersion；
- ContractParty；
- ContractTerm；
- ContractLifecycleState。

### 不拥有

- OCR 原始结果；
- 任务租约和重试；
- 审批任务执行机制；
- 文件二进制对象。

### 关键不变量示例

- 已签署版本不可被直接覆盖；
- 字段应用必须基于明确版本；
- 变更必须保留历史；
- 状态转换必须满足业务前置条件。

## 9. Project Management Context

### 职责

- 项目身份和生命周期；
- 项目成员和业务角色；
- 项目与客户、合同的业务关联；
- 里程碑和交付状态。

### 拥有

- Project；
- ProjectMember；
- Milestone；
- ProjectStatus。

### 约束

合同上下文可引用 ProjectId，但不得直接修改项目状态；项目上下文也不得直接修改合同字段。

## 10. Approval Management Context

### 职责

- 审批定义和实例；
- 审批人、会签、或签、加签和转交；
- 审批意见与决定；
- 人工等待和审批业务状态。

### 拥有

- ApprovalDefinition；
- ApprovalInstance；
- ApprovalTask；
- ApprovalDecision。

### 不拥有

- 被审批业务对象的正式状态机；
- 通用 Worker 的租约与重试；
- 通知发送实现。

### 协作方式

业务上下文发起审批并保存自己的“等待审批”业务状态；Approval 发布决定事件；业务上下文验证版本后完成自身状态转换。

## 11. Finance Operations Context

### 职责

- 付款计划；
- 收款、付款和核销业务；
- 财务状态；
- 合同和项目财务关联。

### 拥有

- PaymentPlan；
- PaymentRecord；
- Settlement；
- FinanceStatus。

### 约束

不在初期将其设计为会计总账系统；若未来扩展为财务核心系统，应单独通过 ADR 调整边界和合规要求。

## 12. Document Management Context

### 职责

- 文档身份；
- 文件版本和元数据；
- 文档与业务资源关联；
- 文档生命周期；
- 文件访问授权语义；
- 产物引用的业务登记。

### 拥有

- Document；
- DocumentVersion；
- DocumentLink；
- FileMetadata；
- DocumentLifecycleState。

### 不拥有

- OCR、结构解析和字段抽取结果；
- 合同正式字段；
- 通用任务执行状态；
- 具体对象存储 Bucket 和 SDK 类型。

## 13. Document Intelligence Context

### 职责

- 文档识别请求；
- OCR 和结构解析的内部稳定模型；
- 字段抽取、置信度和证据；
- 规范化和校验报告；
- 面向业务上下文的候选建议。

### 拥有

- ProcessingRequest；
- OcrResult；
- ParsedStructure；
- ExtractionResult；
- Evidence；
- FillSuggestion；
- PipelineVersion。

### 不拥有

- 原始文档生命周期；
- 合同等业务上下文的正式字段；
- 通用执行任务的租约和调度状态。

### 关键原则

AI 输出始终是候选事实。正式业务写入必须由目标业务上下文的 Application Use Case 执行权限、版本、冲突和业务规则校验。

## 14. Durable Task Execution Context

### 职责

- 持久化任务和步骤；
- claim、lease、heartbeat；
- 重试、取消、超时和恢复；
- 执行进度和技术产物引用；
- Worker 并发协调。

### 拥有

- Job；
- JobStep；
- Attempt；
- ExecutionLease；
- RetrySchedule；
- ExecutionEvent。

### 不拥有

- 合同、审批、文档复核等业务状态；
- 具体业务规则；
- 正式业务数据。

## 15. Audit Context

### 职责

- 不可抵赖的业务操作记录；
- 谁在何时以何种身份执行何种业务动作；
- 变更摘要、原因、来源和关联 trace；
- 审计查询和保留策略。

### 约束

审计记录不能替代领域历史和事件；应用用例负责产生审计意图，Audit Context 负责持久化与查询。

## 16. Notification Context

### 职责

- 通知意图；
- 收件人解析；
- 模板和渠道策略；
- 发送状态和退避；
- 用户通知偏好。

### 不拥有

- 业务状态决定；
- 审批规则；
- 外部邮件或短信供应商模型。

## 17. Integration Gateway Context

### 职责

- 对遗留系统和第三方系统建立 Anti-Corruption Layer；
- 外部 DTO 与内部稳定模型转换；
- 同步、回调、批量导入和数据同步协议；
- 外部契约版本与兼容性。

Agent Adapter 属于入口适配，不单独拥有业务数据。

## 18. 上下文关系模式

### Customer/Supplier

拥有数据和规则的一方为 Supplier，调用方依赖其 Application API 或事件。

### Published Language

跨上下文事件和稳定 API 使用版本化 Schema，不共享数据库模型。

### Anti-Corruption Layer

所有遗留系统、外部供应商、AI/OCR 和第三方平台必须通过 ACL 转换为内部模型。

### Separate Ways

没有业务必要关联的上下文保持独立，避免为了“统一”建立无边界共享模型。

## 19. 跨上下文禁止事项

禁止：

- 直接写入其他上下文私有表；
- 直接复用其他上下文的数据库 Row 作为公开模型；
- 在 Shared Kernel 放置跨上下文可变业务实体；
- 通过公共数据库事务任意修改多个上下文；
- 将消息消费者直接当作业务规则所有者；
- 为 Agent 建立另一套跨上下文业务逻辑。

## 20. 代码与部署映射

初期允许一个 crate 暂时包含一个上下文的 Domain、Application、Delivery 和 Infrastructure 模块，也允许一个进程承载多个上下文。

必须保持：

```text
业务上下文边界
≠ 自动等于 crate
≠ 自动等于进程
≠ 自动等于微服务
```

但代码、数据库迁移和测试必须能够识别每个上下文的所有权。

## 21. 演进规则

新增、合并或拆分 Bounded Context 必须通过 ADR，并说明：

- 业务能力变化；
- 统一语言变化；
- 数据所有权迁移；
- API 和事件影响；
- 一致性和补偿；
- 迁移、回滚和兼容期；
- 部署影响。

## 22. 当前实施优先级

```text
第一批：Identity / Organization / Policy 基础调用上下文
第一批：Document Management 最小垂直切片
第二批：Document Intelligence 与 Durable Task Execution
第三批：Customer / Contract / Approval 核心业务
后续：Project / Finance / Notification 深化
```

具体顺序可由计划调整，但不得绕过数据所有权和上下文关系设计。
