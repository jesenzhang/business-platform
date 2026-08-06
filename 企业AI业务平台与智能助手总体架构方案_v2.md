# 企业 AI 业务平台与智能助手总体架构方案

> 文档 ID：ARCH-OVERALL-001
> 版本：v2.1
> 状态：Baseline
> 日期：2026-08-06
> 替代版本：v2.0
> 适用范围：企业内部管理系统、合同/客户/项目/审批/文档等业务平台，以及其上的 AI 助手能力

---

## 1. 文档目的

本文定义企业业务系统从现有 Python 后端逐步演进为 Rust 后端后的总体架构，并把平台原生数据治理、Runtime Audit、分析与可视化纳入同一权威边界。

本方案明确：

1. Rust 业务后端是完整、独立、权威的系统主体。
2. Web、移动端、开放 API、后台任务和 Agent 都是业务平台的访问渠道。
3. LLM、OCR、文档解析、Embedding 等 AI 能力通过外部 API 接入。
4. Agent 是可选、可替换的智能交互入口，不是系统运行的前置依赖。
5. 所有业务规则、权限、事务、状态机、Runtime Audit 和治理操作必须由 Rust 业务平台掌握。
6. 平台原生分析与可视化消费权威业务数据、领域事件和既有 Runtime Audit，产出可重建派生读模型，不成为业务事实所有者。
7. 第一阶段优先采用模块化单体，只有出现明确的独立扩缩容、故障隔离或安全隔离需求时才拆分微服务。

---

## 2. 建设目标

### 2.1 业务目标

系统应支持：

- 用户、组织、角色和权限管理
- 客户、合同、项目、审批、付款、文件等核心业务
- Web、移动端及第三方系统访问
- 文档上传、解析、OCR、字段抽取、分类和摘要
- AI 自动填充建议与人工复核
- 长任务、定时任务、异步工作流、失败重试和补偿
- 完整的 Runtime Audit、数据完整性治理、追踪和可观测性
- 平台原生指标、Dashboard、报表和受控导出
- 通过 Agent 使用自然语言查询和操作业务
- 在 Agent 不可用时，所有业务能力仍可通过普通 UI 和 API 正常使用

### 2.2 技术目标

- 统一使用 Rust 构建核心业务服务和基础能力
- 将 AI 模型调用封装为稳定的 Provider 接口
- 避免业务代码直接依赖具体模型厂商
- 使用明确的领域边界和应用服务承载业务逻辑
- 以 PostgreSQL 作为权威状态存储，并承载初期可重建分析投影、物化视图和指标快照
- 以 MinIO/S3 管理文档、报表产物和文件
- 以 NATS JetStream 或现有消息平台承载异步任务
- 通过 OpenTelemetry 建立端到端链路追踪
- 允许 Agent Runtime 随时替换，而不影响业务平台

---

## 3. 核心架构原则

### 3.1 业务平台独立于 Agent

系统必须满足：

```text
关闭 Agent Runtime
    ↓
Web、移动端、开放 API、后台任务和 AI 文档能力仍正常运行
```

Agent 只能调用业务平台已经提供的能力，不得成为业务逻辑的唯一入口。

### 3.2 一个业务规则只实现一次

同一个业务操作，无论来自 Web、移动端、第三方系统还是 Agent，都必须进入同一个应用服务。

```text
Web Controller ───────┐
Mobile API ───────────┤
Open API ─────────────┼──→ Application Service
Agent Adapter ────────┘
```

禁止为 Agent 维护另一套业务逻辑。

### 3.3 AI 输出是不可信候选结果

LLM、OCR 和文档解析服务产生的是候选值，不是最终业务事实。

```text
AI 原始结果
→ Schema 校验
→ 类型转换
→ 证据校验
→ 业务规则校验
→ 权限与版本检查
→ 自动应用或人工复核
```

### 3.4 Agent 没有最终执行权

Agent 可以理解意图、补充参数和提出结构化操作，但最终执行由 Rust 业务平台决定。

```text
自然语言
→ Agent 生成工具调用
→ Rust 服务验证
→ 必要时预览和确认
→ 应用服务执行
→ 审计
```

### 3.5 模块化单体优先

初期不应为了“微服务架构”而提前拆分大量服务。

优先采用：

```text
一个 Rust Workspace
+ 多个领域 crate
+ 少量独立部署进程
```

仅在以下条件成立时拆分服务：

- 需要独立扩缩容
- 需要独立安全边界
- 需要独立故障隔离
- 需要独立发布周期
- 具有明确的数据所有权
- 使用特殊硬件或资源
- 模块复杂度已超过单体边界

---

## 4. 总体逻辑架构

```text
┌────────────────────────── 访问渠道 ──────────────────────────┐
│                                                              │
│  Web UI     移动端     第三方系统     Agent 助手     运维后台 │
│                                                              │
└───────────────┬──────────────┬──────────────┬────────────────┘
                │              │              │
                └──────────────┴──────┬───────┘
                                      ▼
┌──────────────────────────────────────────────────────────────┐
│                  Rust 接入层 / API Gateway                    │
│                                                              │
│ OIDC/SSO、租户识别、路由、限流、请求追踪、API 版本、SSE/WS    │
└──────────────────────────────┬───────────────────────────────┘
                               ▼
┌──────────────────── Rust Business Platform ──────────────────┐
│                                                              │
│ Identity   Organization   Customer   Contract   Project       │
│ Approval   Finance        Document   Notification   Audit     │
│ Runtime Governance   Analytics / Visualization               │
│                                                              │
│ Domain Model / Application Service / Repository / Policy     │
│ Transaction / State Machine / Domain Event / Validation      │
└───────────────┬───────────────────────┬──────────────────────┘
                │                       │
                ▼                       ▼
┌─────────────────────────┐   ┌───────────────────────────────┐
│ Rust Workflow Platform  │   │ Rust AI Application Layer    │
│                         │   │                               │
│ 长任务、定时任务、重试   │   │ OCR、LLM、文档解析、Embedding │
│ 补偿、状态恢复、事件处理 │   │ 抽取、分类、摘要、自动填充建议 │
└──────────────┬──────────┘   └───────────────┬───────────────┘
               │                              │
               │                              ▼
               │                  ┌────────────────────────────┐
               │                  │ 外部 AI 服务               │
               │                  │ LLM / OCR / VLM / Parser   │
               │                  │ Embedding / Reranker       │
               │                  └────────────────────────────┘
               ▼
┌──────────────────────── 基础设施层 ──────────────────────────┐
│ PostgreSQL    MinIO/S3    NATS/Kafka    Redis（可选）         │
│ OpenTelemetry Prometheus  Grafana       Loki/Tempo           │
└──────────────────────────────────────────────────────────────┘


可选 Agent 扩展：

┌──────────────────────────┐
│ 开源 Agent Runtime       │
│ nanobot / Agno / Goose   │
└────────────┬─────────────┘
             │ MCP / HTTP
             ▼
┌──────────────────────────┐
│ Rust Agent Adapter       │
│ Skill / ActionPlan       │
│ 身份委托 / 确认 / 审计   │
└────────────┬─────────────┘
             │
             ▼
┌──────────────────────────┐
│ Rust Business Platform   │
└──────────────────────────┘
```

---

## 5. 部署单元

建议初期采用五个部署单元。

### 5.1 `business-api`

系统主体，对外提供业务 API。

职责：

- 用户和组织
- 客户、合同、项目
- 审批和财务
- 文件元数据
- 权限和租户
- AI 任务创建
- 查询和写操作
- SSE/WebSocket 状态推送
- OpenAPI 文档

### 5.2 `business-worker`

后台工作执行器。

职责：

- 领域事件处理
- 长任务
- 定时任务
- 通知发送
- 工作流推进
- 重试和补偿
- Outbox 消息发布
- 失败恢复

### 5.3 `ai-worker`

AI 应用执行器。

职责：

- 调用 OCR API
- 调用 LLM/VLM API
- 调用文档解析 API
- 调用 Embedding/Reranker API
- 结构化结果校验
- 字段标准化
- 生成自动填充建议
- 记录模型、Prompt、Token、费用和耗时

初期可以与 `business-worker` 合并，规模增长后再拆分。

### 5.4 `agent-adapter`

可选的 Rust Agent 接入服务。

职责：

- 暴露 MCP Tool 或 Agent 专用 REST API
- 将业务能力转换为 Agent Skill
- 传播用户委托身份
- 生成 ActionPlan
- 写操作预览和确认
- Agent 调用审计
- 工具白名单和风险控制

### 5.5 `agent-runtime`

可替换的开源 Agent 服务。

候选：

- HKUDS/nanobot：轻量 PoC 和快速改造
- Agno AgentOS：服务端多用户 Agent
- Goose：Rust 桌面助手或单用户助手

Agent Runtime 不拥有业务数据库，不保存业务权威状态。

---

## 6. Rust Workspace 结构

```text
enterprise-platform/
├── Cargo.toml
├── apps/
│   ├── business-api/
│   ├── business-worker/
│   ├── ai-worker/
│   ├── agent-adapter/
│   └── migration/
│
├── crates/
│   ├── shared-kernel/
│   ├── identity/
│   ├── organization/
│   ├── customer/
│   ├── contract/
│   ├── project/
│   ├── approval/
│   ├── finance/
│   ├── document/
│   ├── notification/
│   ├── audit/
│   ├── workflow/
│   ├── ai-application/
│   ├── agent-integration/
│   ├── policy/
│   ├── object-storage/
│   ├── messaging/
│   └── observability/
│
├── migrations/
├── config/
├── deploy/
├── docs/
└── tests/
```

### 6.1 领域 crate 内部结构

```text
contract/
├── domain/
│   ├── entity.rs
│   ├── value_object.rs
│   ├── aggregate.rs
│   ├── event.rs
│   ├── repository.rs
│   └── error.rs
├── application/
│   ├── command/
│   ├── query/
│   ├── service/
│   └── dto/
├── infrastructure/
│   ├── persistence/
│   └── integration/
└── api/
    ├── handler.rs
    ├── request.rs
    └── response.rs
```

---

## 7. 后端技术栈

### 7.1 Web 与异步运行时

- Rust stable
- Tokio
- Axum
- Tower
- tower-http
- Serde
- Reqwest
- Rustls

### 7.2 数据库

- PostgreSQL
- SQLx
- 数据库迁移：SQLx migrations 或独立 migration app
- Redis：仅用于缓存、限流、短期令牌等非权威状态

### 7.3 消息和任务

优先选择现有基础设施；若没有统一消息平台，建议：

- NATS JetStream：任务、事件、重放
- Kafka：已有 Kafka 时继续使用
- Outbox Pattern：保证数据库状态与事件发布的一致性

### 7.4 文件

- MinIO 或兼容 S3 的对象存储
- PostgreSQL 保存文件元数据和业务关系
- 消息队列只传递 `object_key`，不传递文件本体

### 7.5 API

- 前端：REST/JSON
- 状态推送：SSE 或 WebSocket
- 内部服务：REST 或 Tonic/gRPC
- Agent：MCP Streamable HTTP + REST
- 外部集成：版本化 OpenAPI

### 7.6 可观测性

- tracing
- OpenTelemetry
- OpenTelemetry Collector
- Prometheus
- Grafana
- Loki 或 Elasticsearch
- Tempo 或 Jaeger

---

## 8. 核心业务层设计

### 8.1 领域模型

业务规则应位于领域模型或应用服务，不得散落在 Controller、Agent Skill 或数据库脚本中。

示例：

```text
Contract
├── 状态
├── 当前版本
├── 签署方
├── 金额
├── 有效期
├── 审批状态
└── 可执行操作
```

### 8.2 应用服务

所有入口共享应用服务：

```text
SubmitContractUseCase
ApproveApplicationUseCase
ApplyExtractedFieldsUseCase
CreateCustomerUseCase
StartDocumentExtractionUseCase
```

应用服务负责：

- 身份与权限检查
- 输入校验
- 加载领域对象
- 执行业务规则
- 管理事务
- 产生领域事件
- 写审计记录

### 8.3 数据版本与并发控制

重要业务对象必须包含版本号。

```text
contract_id
version
updated_at
updated_by
```

更新时使用乐观锁：

```text
UPDATE contract
SET ..., version = version + 1
WHERE id = ? AND version = ?
```

版本不一致时拒绝执行，避免 UI、Agent 或多个用户相互覆盖。

---

## 9. AI 应用层设计

### 9.1 AI Provider 抽象

业务代码不得直接依赖模型厂商 SDK。

```rust
trait LlmProvider {
    async fn generate(&self, request: LlmRequest)
        -> Result<LlmResponse, LlmError>;
}

trait OcrProvider {
    async fn recognize(&self, request: OcrRequest)
        -> Result<OcrResult, OcrError>;
}

trait DocumentParserProvider {
    async fn parse(&self, request: ParseRequest)
        -> Result<ParsedDocument, ParseError>;
}
```

具体适配器：

```text
OpenAICompatibleProvider
InternalLlmProvider
VendorOcrProvider
VendorDocumentParserProvider
```

底层优先使用 `reqwest` 直接调用稳定的 HTTP API。

### 9.2 文档处理流程

```text
上传文档
→ 保存到 MinIO/S3
→ 创建 DocumentJob
→ OCR
→ 文档结构解析
→ LLM 字段抽取
→ Schema 校验
→ 字段标准化
→ 证据关联
→ 生成填充建议
→ 人工复核或规则自动应用
```

### 9.3 AI 结果模型

AI 结果应包含：

```json
{
  "schema_version": "contract-v1",
  "pipeline_version": "2026-07",
  "model_provider": "internal",
  "model_name": "model-x",
  "prompt_version": "extract-v3",
  "fields": {
    "contract_amount": {
      "raw_value": "人民币壹佰万元整",
      "normalized_value": 1000000,
      "confidence": 0.96,
      "evidence": {
        "page": 3,
        "text": "合同总金额：人民币壹佰万元整"
      }
    }
  }
}
```

### 9.4 自动填充决策

```text
AI 候选值
→ 类型与格式校验
→ 目标字段权限检查
→ 现有值冲突检查
→ 数据版本检查
→ 跨字段一致性校验
→ 决策
```

建议决策枚举：

```text
AutoApply
ApplyAndReview
SuggestOnly
Reject
```

---

## 10. 工作流和异步任务

### 10.1 任务状态机

```text
Pending
→ Running
→ WaitingExternal
→ Validating
→ AwaitingReview
→ Applying
→ Completed

异常路径：
→ RetryScheduled
→ Failed
→ Cancelled
```

### 10.2 可靠性要求

每个任务必须具备：

- 唯一任务 ID
- 幂等键
- 当前状态
- 当前步骤
- 尝试次数
- 下一次重试时间
- 输入摘要
- 输出引用
- 错误分类
- 任务超时
- 取消状态
- Trace ID

### 10.3 重试规则

仅对暂时性错误重试：

- 连接失败
- HTTP 429
- HTTP 502/503/504
- 外部服务明确返回的临时错误

不得盲目重试：

- 认证失败
- 参数错误
- 文件格式不支持
- 上下文超限
- 业务校验失败
- 权限拒绝
- 已产生副作用但状态不明确的非幂等请求

---

## 11. Agent 集成架构

### 11.1 Agent 的定位

Agent 是业务平台的一个自然语言客户端。

传统 UI：

```text
点击页面
→ 填写表单
→ 调用 API
```

Agent：

```text
自然语言
→ 意图识别
→ 参数补全
→ 调用同一个业务能力
```

### 11.2 Agent 可调用的能力

允许：

```text
contract.search
contract.get
contract.prepare_update
contract.submit_for_approval

document.start_extraction
document.get_job_status
document.get_fill_suggestions
document.prepare_apply_fields

approval.list_pending
approval.get
approval.prepare_decision
```

禁止：

```text
execute_sql
run_shell
call_any_api
write_database
http_request
```

### 11.3 Agent Skill 与业务 API 的关系

Agent Skill 是业务 API 的受控适配层，不是新的业务实现。

```text
contract.search
→ Agent Adapter
→ Contract Query Service
→ PostgreSQL
```

```text
contract.prepare_update
→ Agent Adapter
→ Contract Application Service
→ 返回 ActionPlan
```

---

## 12. Agent 写操作安全模型

### 12.1 风险分级

| 级别 | 示例 | 策略 |
|---|---|---|
| R0 | 查询、统计、查看进度 | 权限通过后直接执行 |
| R1 | 创建草稿、生成建议 | 简单确认或直接执行 |
| R2 | 修改正式数据、提交审批、批量更新 | 预览、显式确认、版本检查 |
| R3 | 删除、付款、权限修改、不可逆操作 | 二次认证、审批或禁止 Agent 执行 |

### 12.2 两阶段执行

```text
Prepare
→ Preview
→ Confirm
→ Execute
```

第一阶段生成服务端 ActionPlan：

```json
{
  "action_plan_id": "ap_01...",
  "skill": "contract.bulk_extend",
  "summary": "将为 9 份合同提交延期 30 天申请",
  "affected_resources": ["C001", "C002"],
  "excluded_resources": [],
  "expires_at": "2026-07-30T12:00:00+08:00"
}
```

ActionPlan 必须绑定：

- 用户
- 租户
- Skill 名称和版本
- 具体资源
- 输入参数
- 对象版本
- 权限决策
- 计划哈希
- 失效时间

用户确认后只能提交 `action_plan_id`，不得由 Agent 再次生成参数。

---

## 13. 身份与权限

### 13.1 统一认证

建议使用企业 OIDC/SSO。

调用链必须传播：

```text
服务身份
+
最终用户委托身份
```

### 13.2 权限模型

建议：

- RBAC：角色级基础权限
- ABAC：租户、部门、金额、状态、资源范围等条件
- 默认拒绝
- 业务服务二次校验

权限请求示例：

```text
Principal：user:1001
Action：contract.extend
Resource：contract:C10086
Context：
  tenant_id
  department_id
  amount
  current_status
  batch_size
  authentication_level
```

### 13.3 多租户隔离

必须在以下层次同时实施：

- Token 中的 tenant claim
- API Gateway
- Application Service
- Repository 查询条件
- 数据库约束或 RLS
- 缓存键
- 对象存储路径
- 消息事件
- Agent 会话

---

## 14. 数据与存储

### 14.1 PostgreSQL

保存：

- 权威业务数据（由各 Bounded Context 持有）
- 用户和权限
- 工作流和任务
- AI 抽取结果
- ActionPlan
- Agent 调用记录
- AuditEvent、Finding、Repair 和 Ledger（按 Runtime Governance 所有权）
- 分析投影、物化视图、指标版本和查询元数据（可重建派生数据）
- Outbox

### 14.2 MinIO/S3

保存：

- 原始文档
- OCR 中间产物
- 解析后的结构化文件
- 大型 AI 输出
- 导出文件
- 历史版本

### 14.3 平台原生数据治理、分析与可视化

Runtime Governance 已由 PLAN-0005 集成，负责统一 Audit、完整性 Finding、受控修复、Repair Ledger 和 Lease/Fence Recovery；本总体方案不把这些能力描述为未来首次建设。Analytics/Visualization 是后续平台能力，只拥有可重建投影、指标/版本、Dataset、Dashboard/Report 定义、查询元数据、快照和报表产物。

其统一数据流为：

```text
权威业务事务
→ 领域事件 / Outbox / 受控 AuditEvent 读取
→ 可重建分析投影
→ 版本化指标语义层
→ Analytics Query Service
→ UI / Open API / Report / Agent
```

UI、API、报表和 Agent 共享同一受控指标语义和查询服务。Agent 位于分析平台之上，不得生成或执行任意 SQL、查询任意表、浏览 Schema、导出未脱敏数据或持有分析服务外数据库凭证。初期使用 PostgreSQL 投影；只有可测量的延迟、并发、扫描量、重建恢复或资源隔离证据触发时，才评估独立 `analytics-worker` 或 OLAP。

---

### 14.4 消息系统

消息中只传递：

```text
event_id
tenant_id
aggregate_id
job_id
object_key
schema_version
trace_id
```

不得在消息中传递大文件或敏感完整文档。

---

## 15. 核心数据表

建议至少包含：

```text
users
roles
permissions
organizations
tenants

customers
contracts
contract_versions
projects
approvals
documents
document_versions

jobs
job_steps
job_attempts
outbox_events

ai_requests
ai_results
prompt_definitions
prompt_versions
model_usage

agent_sessions
agent_runs
tool_calls
action_plans
action_plan_resources
action_confirmations
action_executions

audit_events
security_events
```

---

## 16. 可观测性和审计

### 16.1 端到端追踪

一次操作必须可以追踪：

```text
用户请求
→ API Gateway
→ Application Service
→ Workflow/AI Worker
→ 外部 AI API
→ 数据库事务
→ 领域事件
→ 审计事件
```

Agent 场景还应包含：

```text
Agent Message
→ Agent Run
→ Tool Call
→ ActionPlan
→ Confirmation
→ Application Service
→ Audit Event
```

### 16.2 指标

正式业务状态、AuditEvent 和相关 Outbox 必须由数据所有者在同一本地事务中写入。AuditEvent 写入失败时，业务事务必须失败并回滚；Outbox 只负责后续事件发布，不替代权威审计记录。审计载荷以 `change_summary`、`changed_field_names`、`resource_version`、策略允许时的 `redacted_before_after` 和 `stable_failure_code` 为边界，不强制保存完整敏感 Before/After；Secret、凭证、内部路径、完整文件和未脱敏个人数据不得进入审计载荷。

分析指标必须标注来源、Metric Version、租户、时间基准、延迟和脱敏策略；分析延迟或缺口不能改变权威业务状态。

业务指标：

- 请求成功率
- 任务完成率
- 审批耗时
- 文档处理量
- 自动填充采用率
- 人工修正率

技术指标：

- P50/P95/P99
- CPU、内存
- 数据库连接池
- 消息积压
- 外部 AI 调用延迟
- 重试率
- 失败率

AI 指标：

- OCR 准确率
- 字段准确率和召回率
- 结构化输出失败率
- Token 使用量
- 单任务费用
- 模型与 Prompt 版本表现

Agent 指标：

- Tool 选择正确率
- 参数补充轮次
- 写操作取消率
- 权限拒绝率
- 任务完成率

---

## 17. 安全要求

### 17.1 Prompt Injection

文档、邮件、OCR 文本和外部工具结果均视为不可信数据。

必须分离：

```text
控制平面：
System Prompt、Skill Definition、权限策略

数据平面：
用户文档、检索内容、OCR 结果、业务数据
```

文档内容不得改变 Agent 权限或工具白名单。

### 17.2 API 安全

- OIDC/OAuth 2.1
- TLS
- 最小权限
- 请求限流
- 参数 Schema 校验
- 幂等键
- 防重放
- 租户隔离
- 敏感字段脱敏
- 审计追加写
- 密钥集中管理

### 17.3 Agent 安全

- 仅加载白名单 Skill
- 禁止通用 Shell、SQL、任意 HTTP
- 限制单回合 Tool 调用次数
- 限制批量操作数量
- 高风险操作强制确认
- Agent Runtime 不直接访问生产数据库
- Agent Runtime 不持有高权限长期凭证

---

## 18. 部署拓扑

### 18.1 初期

```text
business-api × 2
business-worker × 2
ai-worker × 1～N
agent-adapter × 1～2（可选）
agent-runtime × 1～N（可选）

PostgreSQL
NATS JetStream
MinIO
OpenTelemetry Collector
Prometheus + Grafana
Loki + Tempo
```

### 18.2 扩容原则

- `business-api`：按 HTTP 请求量扩容
- `business-worker`：按任务积压扩容
- `ai-worker`：按外部 AI 并发和文档量扩容
- `agent-runtime`：按会话数和模型调用量扩容
- PostgreSQL：优先优化索引、连接池和查询，再考虑读副本
- 消息系统：按吞吐和可用性扩容

---

## 19. 分阶段实施

### 已完成：基础平台与 Runtime Governance

以下能力已在主干集成并归档：

- Rust 服务基座、核心业务垂直切片和文档处理固定 Pipeline；
- Runtime Audit；
- Integrity Finding；
- Controlled Repair；
- Repair Ledger；
- Lease/Fence Recovery。

PLAN-0005 已 Integrated / Archived。其既有审计原子性、Finding、Repair 和 Ledger 语义继续由对应 Baseline 约束；本架构不重新定义它们。

### 后续阶段：平台原生 Analytics/Visualization

1. **分析投影基座**：事件/Outbox 消费、Inbox、offset、幂等、重放、重建、缺口、血缘和质量检查。
2. **指标语义层**：Metric、Measure、Dimension、Time Dimension、Dataset、Metric Version、Filter Policy 和 Lineage。
3. **Analytics Query Service**：身份、租户、行列策略、脱敏、查询预算、超时、并发、扫描量、结果限制和查询审计。
4. **声明式 Dashboard 与报表**：共享受控查询、下钻、快照、报表产物、导出和恢复。
5. **Agent 受控分析技能**：只读白名单工具、口径/新鲜度披露和受限导出，仍不得直接 SQL 或访问数据库。

每个后续阶段必须以独立 PLAN 实施，提供契约、性能、恢复、安全、可观测性和架构门禁证据；本 PR 只更新架构文档。

---

## 20. 架构决策摘要

### 决策 1：Rust 后端是主体

业务平台不依赖 Agent 运行。

### 决策 2：Agent 是可插拔入口

Agent Runtime 可以从 nanobot 切换到 Agno、Goose、jarvis-rs 或其他实现。

### 决策 3：AI 能力属于业务平台

AI 文档解析、字段抽取和自动填充由 Rust 后端统一编排，UI 和 Agent 均调用同一能力。

### 决策 4：外部 AI API 优先

LLM、OCR 和文档解析已服务化时，生产业务侧无需保留 Python。

Python 仅用于：

- 离线评估
- 数据分析
- Prompt 实验
- 特定供应商 SDK 适配

### 决策 5：模块化单体优先

先建立稳定领域边界，再按客观需要拆微服务。

### 决策 6：业务规则不可放入 Agent

Agent 只生成结构化意图；权限、事务和最终执行由 Rust 应用服务负责。

---

## 21. 第一阶段推荐交付物

1. Rust 服务模板
2. Workspace 模块结构
3. 统一配置系统
4. 统一错误协议
5. OIDC/SSO 接入
6. PostgreSQL/SQLx 基座
7. MinIO/S3 文件模块
8. NATS/Outbox 任务基座
9. OpenTelemetry 可观测性
10. AI Provider 抽象
11. 文档解析任务状态机
12. 首批业务领域模块
13. Agent Adapter 接口规范
14. 首批只读 Skill
15. ActionPlan 数据模型
16. 安全与审计规范
17. 集成测试和压测基线
18. 部署和回滚脚本

---

## 22. 验收标准

### 业务独立性

- Agent Runtime 停止后，Web 和 API 业务功能正常
- AI 文档功能仍可由 UI 发起
- 后台任务可独立恢复

### 业务一致性

- UI 和 Agent 调用相同应用服务
- 不存在 Agent 专用业务规则
- 所有正式写入均经过权限、事务和版本校验

### AI 可靠性

- AI 输出经过 Schema 和业务校验
- 记录模型、Prompt、版本、Token 和证据
- 自动填充可追踪、可复核、可回滚

### 安全

- Agent 不直接访问数据库
- Agent 不拥有通用 Shell、SQL 和任意 HTTP 能力
- 高风险写操作具有预览和确认
- 多租户隔离通过测试
- 审计链条完整

### 运维

- 核心服务具备健康检查
- 关键请求具有 Trace ID
- 任务可重试、取消和恢复
- 部署支持回滚
- 关键指标和告警可用

---

## 23. 最终架构定位

本系统不是“以 Agent 为中心的业务系统”，而是：

```text
完整的 Rust 企业业务平台
+
平台原生数据治理与 Runtime Audit
+
平台原生分析与可视化
+
内建的 AI 业务能力
+
可选、可替换的 Agent 智能入口
```

权威关系如下：

```text
业务事实        → 各业务 Bounded Context
Runtime Audit   → Runtime Governance
分析投影与指标  → Analytics（可重建派生数据）
业务规则        → Domain/Application Service
权限与事务      → Rust Business Platform
AI 候选结果     → AI Application Layer
自然语言理解    → Agent Runtime
Agent 工具适配  → Rust Agent Adapter
```

最终原则：

> 业务平台可以没有 Agent；Agent 不能没有业务平台。

> Agent 负责理解用户，Rust 后端负责正确地完成业务。
