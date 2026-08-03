# 工作流与长时任务架构

> 文档 ID：ARCH-WORKFLOW-001  
> 版本：1.0  
> 状态：Baseline  
> 生效日期：2026-07-30  
> 适用范围：文档解析、批量导入、报表生成、外部同步、审批等待及其他长时流程

## 1. 目的

本文定义业务流程、人工工作流和可靠任务执行之间的边界，确保：

- 业务状态不被技术执行状态替代；
- 长时任务不依赖单个进程内存；
- 通用执行能力不吞并具体业务规则；
- 任务可重试、取消、恢复、审计和水平扩展；
- 基础设施实现可以替换而不改变核心语义。

## 2. 四类模型

### 2.1 Domain State Machine

由具体业务上下文拥有，表达业务对象允许的状态和不变量。

例如：

```text
合同草稿 → 审批中 → 已批准 → 已签署
```

### 2.2 Process Manager

协调多个 Bounded Context 的长期业务过程，记录业务流程状态、等待条件、超时和补偿。

例如：

```text
发起合同审批
→ 等待审批决定
→ 创建签署任务
→ 等待签署结果
→ 更新合同业务状态
```

### 2.3 Human Workflow

表达人工作业、审批、复核、领取、转交和截止时间。

### 2.4 Durable Task Execution

负责技术执行语义：

- 任务和步骤；
- claim、lease、heartbeat；
- 重试和退避；
- 超时和取消；
- Worker 崩溃恢复；
- 执行进度；
- 技术产物引用。

## 3. 根本边界

```text
业务上下文
    拥有业务意义、业务状态和正式结果

Process Manager / Application
    拥有跨上下文协调意图

Durable Task Execution
    拥有可靠执行状态

Infrastructure Adapters
    实现持久化、通知、存储和外部调用
```

Durable Task Execution 不决定：

- 合同是否可以签署；
- 哪类审批人必须参加；
- 哪个 AI 字段建议可正式应用；
- 付款是否满足业务条件。

## 4. 业务状态与执行状态

### 业务状态示例

- 文档已提交解析；
- 解析结果待复核；
- 字段建议已应用；
- 合同等待审批；
- 批量导入等待业务确认。

### 执行状态示例

- Pending；
- Ready；
- Running；
- AwaitingExternal；
- RetryScheduled；
- CancelRequested；
- Failed；
- Completed。

业务状态变化必须由所属业务用例决定。任务成功只表示技术步骤完成，不自动等于业务操作成功。

## 5. 核心任务模型

### Job

表示一次可靠执行请求：

- JobId；
- JobKind；
- CorrelationId；
- TenantId；
- SubjectRef；
- PipelineVersion；
- Status；
- Priority；
- CancellationState；
- Progress；
- CreatedAt / StartedAt / FinishedAt。

### JobStep

表示可独立检查点、重试和恢复的执行单元：

- StepId；
- StepKind；
- Status；
- AttemptCount；
- RetryPolicy；
- AvailableAt；
- ExecutionLease；
- InputArtifactRef；
- OutputArtifactRef。

### Attempt

记录一次具体执行尝试：

- WorkerId；
- AttemptNumber；
- StartedAt / FinishedAt；
- Outcome；
- ErrorClass；
- ExternalRequestRef；
- Metrics。

## 6. 端口语义

核心可以定义稳定能力端口，例如：

- DurableTaskStore；
- TaskWakeupPort；
- ArtifactStore；
- ExecutionClock；
- WorkerIdentityProvider；
- ExternalCapabilityProvider；
- ProgressPublisher。

端口必须表达能力语义，不包含数据库语句、Broker Topic、Bucket 或供应商 Endpoint。

## 7. 创建任务

业务用例创建任务时必须：

1. 验证身份、租户、权限和资源版本；
2. 验证业务对象当前允许发起该过程；
3. 生成稳定幂等键；
4. 记录业务过程状态；
5. 创建执行请求；
6. 在需要时记录审计和待发布事件；
7. 返回 JobId，不保持同步 HTTP 请求等待完整执行。

外部 API 一般返回：

```text
202 Accepted + job_id + status_url
```

## 8. 任务计划与步骤

第一阶段优先使用代码定义、版本化的确定性 Pipeline，不立即建设任意可视化 DAG 产品。

文档解析示例：

```text
ValidateSource
→ DetectDocumentType
→ RunOcr
→ ParseStructure
→ ExtractFields
→ Normalize
→ Validate
→ GenerateSuggestions
→ AwaitBusinessReview
```

Pipeline 版本必须写入任务，避免系统升级后无法解释历史执行。

## 9. 检查点

每个高成本、可恢复或有外部副作用的步骤完成后必须持久化检查点。

检查点包括：

- 输入引用与 checksum；
- 输出引用与 schema version；
- 步骤版本；
- 外部请求 ID；
- 完成时间；
- 下一步状态。

进程崩溃后应从最近成功检查点继续，而不是默认重做整个流程。

## 10. Claim 与 Lease

多个 Worker 通过权威任务存储原子领取步骤。

语义要求：

- 同一时刻只有一个有效 claim；
- claim 有确定过期时间；
- Worker 定期 heartbeat；
- lease 过期后可安全重新领取；
- 旧 Worker 在失去 lease 后不得提交成功结果；
- 完成操作验证 ClaimToken 或等价 fencing token。

具体数据库锁和查询属于适配器实现。

## 11. 重试

错误至少分为：

- Transient：网络、429、临时不可用；
- Permanent：格式不支持、权限、业务校验失败；
- Ambiguous：外部副作用可能已发生但结果未知；
- Cancelled；
- LeaseLost；
- PoisonInput。

RetryPolicy 定义：

- 最大尝试次数；
- 退避策略；
- jitter；
- 每次尝试超时；
- 总截止时间；
- 进入人工处理或 Dead Letter 的条件。

不得对所有错误无差别重试。

## 12. 幂等与副作用

步骤幂等键建议由以下内容构成：

```text
job_id + step_kind + step_version + input_checksum
```

对于外部副作用：

- 优先使用供应商幂等键；
- 保存 provider_request_id；
- 超时后优先查询状态；
- 重复执行前检查既有成功产物；
- 正式业务写入仍由业务 Application Use Case 执行。

## 13. 取消

取消采用协作式语义：

- API 记录 CancelRequested；
- Worker 在步骤边界和可中断点检查；
- 支持外部取消时调用外部能力；
- 不支持时等待当前不可中断操作结束；
- 迟到结果不得自动推进后续业务状态；
- 取消过程必须审计。

不得承诺无法实现的瞬时强制终止。

## 14. 外部异步能力

外部供应商返回其任务 ID 时，当前步骤进入 `AwaitingExternal`，保存：

- Provider；
- ProviderJobId；
- ProviderStatus；
- NextPollAt；
- CallbackCorrelation；
- Deadline。

回调或轮询只更新执行状态并触发后续验证，不直接信任外部数据成为业务事实。

## 15. 分块和并行

大文档或批量任务可以拆分为 Child Step：

```text
Split
→ N 个并行 Chunk
→ Aggregate
```

必须限制：

- 单任务并发；
- 单租户并发；
- 单 Provider 并发；
- 全局并发；
- 文件大小、页数、Token 和费用；
- 聚合等待和部分失败策略。

## 16. 进度

进度必须基于可测量单位：

- 已完成步骤权重；
- 页数；
- 分块数；
- 已处理记录数。

无法准确估算的外部调用不得伪造线性百分比。

## 17. 消息与唤醒

消息系统主要用于：

- 唤醒 Worker；
- 削峰和分发；
- 发布执行和业务事件。

消息不是权威任务状态。Worker 收到消息后仍需从 DurableTaskStore 原子 claim。

消息丢失时，存储扫描应能发现 Ready 任务；消息重复时，claim 和幂等应阻止重复副作用。

## 18. 恢复扫描

系统必须定期发现：

- lease 已过期的 Running Step；
- 已到重试时间的步骤；
- 长期 AwaitingExternal；
- 长期 CancelRequested；
- 已完成步骤但缺少产物；
- 任务与业务过程状态不一致；
- 未发送的通知和事件。

## 19. 人工介入

以下情况可以进入人工处理：

- 多次重试失败；
- 输入不可自动修复；
- 外部结果状态不明确；
- 业务冲突；
- 需要人工复核；
- 补偿无法自动执行。

人工处理必须通过业务或运维用例完成，不能直接修改数据库状态。

## 20. 文档解析边界

Document Management 拥有文档和版本；Document Intelligence 拥有识别、抽取和建议；Durable Task Execution 拥有执行状态；目标业务上下文拥有正式字段。

```text
Document Version
→ Processing Request
→ Durable Job
→ Extraction Result / Suggestion
→ Target Business Use Case
→ Formal Business State
```

## 21. 部署角色

- `business-api`：创建、查询、取消业务过程和任务；
- `business-worker`：Process Manager、业务后台用例、恢复扫描；
- `ai-worker`：OCR、解析、LLM 等资源型步骤；
- Scheduler：触发定时扫描和计划；
- Adapter：实现任务存储、通知、产物和外部 Provider。

部署角色可以调整，但核心模型和端口语义保持稳定。

## 22. 测试

### Domain/Application

使用 Fake/In-Memory 端口测试：

- 状态机；
- 重试分类；
- 取消；
- 幂等；
- Process Manager；
- 业务结果应用。

### Adapter Contract

真实依赖验证：

- 原子 claim；
- 多 Worker；
- lease 过期；
- crash recovery；
- 消息重复和丢失；
- 大型产物；
- 外部超时。

### E2E

验证从业务请求到最终候选结果、人工复核和正式业务写入的完整链路。

## 23. 禁止事项

- 使用裸 `tokio::spawn` 作为唯一长任务可靠性机制；
- 在业务聚合中保存数据库锁、Broker Offset 或 SDK 对象；
- 让 Worker 直接修改其他上下文业务表；
- 把 Job Completed 等同于业务完成；
- 把任意 DAG 引擎作为第一阶段前置条件；
- 忽略任务版本和历史恢复兼容性。

## 24. 验收清单

- [ ] 业务状态和执行状态分离；
- [ ] 任务状态持久化，不依赖进程内存；
- [ ] 每个高成本步骤有检查点；
- [ ] claim/lease 支持多 Worker 和 fencing；
- [ ] 重试按错误分类；
- [ ] 取消语义明确；
- [ ] 外部副作用具备幂等或状态查询；
- [ ] 消息不是权威任务状态；
- [ ] 业务正式写入通过所属上下文用例；
- [ ] 崩溃恢复和重复执行经过测试。

## 25. PLAN-0004 durable document processing profile

PLAN-0004 instantiates this baseline with a persisted `ProcessingJob` and the
fixed six-step pipeline documented in
[`DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md`](DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md).
The job row is execution-state authority; Document Management owns content
revision and storage reference; Document Intelligence owns candidate and
review state. PostgreSQL is the production multi-worker authority and SQLite
is explicitly local single-process. Workers stop claiming on shutdown and
rely on lease expiry/reclaim after a crash.

Revision 1 closes the transaction boundary around each execution transition:
the worker calls an adapter-owned `ProcessingExecutionUnitOfWork`, dispatches
one `current_step` per claim, and persists the text artifact before enqueuing
`ExtractFields`. AI completion, bounded retry/backoff, reclaim, candidate
creation, cancellation, audit, and outbox effects are atomic. Heartbeat tasks
are joined during drain, and stale fences fail closed.
