# 服务端可观测性架构

> 文档 ID：ARCH-OBS-001  
> 版本：1.0  
> 状态：Baseline  
> 生效日期：2026-07-30  
> 适用范围：日志、指标、分布式追踪、审计、告警和运行诊断

## 1. 目标

可观测性必须支持回答：

- 哪个用户、租户或服务发起了什么操作；
- 请求经过哪些用例、数据库、消息、存储和外部 Provider；
- 长时任务执行到哪一步、尝试了几次、为何失败；
- 系统是否满足性能、可用性和恢复目标；
- 是否发生安全、数据一致性或容量异常；
- 变更后何时开始退化以及影响范围。

## 2. 四类记录

### 2.1 Technical Logs

用于程序和基础设施诊断。

### 2.2 Metrics

用于趋势、容量、SLO 和告警。

### 2.3 Distributed Traces

用于跨入口、用例、适配器和外部依赖的因果链。

### 2.4 Business Audit

用于业务操作追溯和不可抵赖性。Audit 不等同于普通日志。

## 3. 统一关联标识

系统统一传播：

- request_id；
- trace_id/span_id；
- correlation_id；
- causation_id；
- tenant_id；
- principal_id，按脱敏策略；
- job_id/step_id；
- aggregate_id/resource_ref；
- event_id；
- deployment/service version。

禁止将完整 Token、Secret 和敏感文档作为关联字段。

## 4. 结构化日志

日志使用结构化字段，不依赖解析自由文本。

基础字段：

```text
timestamp
level
service.name
service.version
environment
request_id
trace_id
tenant_id
operation
outcome
duration_ms
error.code
```

要求：

- 错误使用稳定分类；
- 同一异常不在多层重复打印完整堆栈；
- 入口记录开始/结束摘要；
- Adapter 记录依赖类型、耗时和结果，不记录 Secret；
- 业务成功事实优先进入 Audit 或 Domain Event，不滥用日志。

## 5. 数据保护

日志、trace 和 metrics 禁止记录：

- Access Token、密码、密钥；
- 完整数据库 URL；
- 完整文档内容；
- 未脱敏 Prompt 和模型响应；
- 身份证号、银行卡号等 Restricted 数据；
- 大型请求/响应体；
- 任意供应商原始错误中的敏感内容。

敏感字段使用：

- 删除；
- 哈希；
- 掩码；
- 分类化值；
- 受控审计引用。

## 6. Trace 边界

每个入口创建或继续 trace：

- HTTP/CLI/Agent；
- Worker message；
- Scheduler；
- Provider callback。

关键 Span：

- Application Use Case；
- 权限判定；
- Repository 操作；
- 事务；
- Outbox 发布；
- 消息消费；
- Artifact 操作；
- 外部 Provider；
- Task Step；
- Process Manager transition。

Span 名称使用稳定操作语义，不包含高基数用户输入。

## 7. 消息追踪

事件 Envelope 传播：

- trace context；
- correlation_id；
- causation_id；
- event_id。

消费者创建新 span 并链接生产者上下文。

重试和重放必须保留原 event_id/correlation，并产生新的消费 attempt 标识。

## 8. 长时任务可观测性

每个 Job 可查询：

- 业务资源；
- 当前状态和步骤；
- 计划版本；
- 每步尝试和 Worker；
- lease 和 heartbeat；
- 外部请求引用；
- 输入输出产物；
- 错误分类；
- 重试时间；
- 取消和人工介入；
- 关联 trace。

任务状态数据库是权威来源，日志仅用于辅助诊断。

## 9. 指标分类

### RED

- Rate；
- Errors；
- Duration。

### USE

- Utilization；
- Saturation；
- Errors。

### 业务指标

- 合同、文档、审批等业务量；
- 解析成功率；
- 人工复核率；
- 字段建议接受率；
- 任务完成时长；
- AI Token/费用；
- 失败和补偿数量。

业务指标必须避免泄漏敏感维度和无限高基数。

## 10. 关键技术指标

API：

- 请求率和延迟分位；
- 状态码和稳定错误码；
- 认证/授权失败；
- 在途请求；
- timeout 和 body limit 拒绝。

Database：

- 连接池使用率；
- 查询延迟；
- 事务失败；
- 死锁；
- 慢查询；
- 存储增长。

Outbox/Messaging：

- 未发布数量；
- 最老事件年龄；
- 发布失败；
- Consumer Lag；
- Redelivery；
- Dead Letter。

Task Runtime：

- Ready/Running/Retry 数量；
- 最老等待时间；
- lease 过期；
- attempts；
- cancel latency；
- stuck jobs。

Artifact：

- put/get 延迟；
- 错误率；
- 流量和容量；
- orphan/missing；
- 临时对象清理。

Provider：

- 调用量、延迟、错误、429；
- Token、费用；
- Provider job 等待时间；
- Schema/validation failure。

## 11. SLI、SLO 与 Error Budget

正式上线前为关键用户旅程定义 SLI/SLO：

- 登录和普通查询；
- 业务写入；
- 文档上传；
- 长任务创建；
- 解析最终完成；
- 审批决定；
- 恢复和备份。

SLO 未明确前，不声称“高可用”。

Error Budget 用于决定是否暂停功能发布并优先修复可靠性。

## 12. 告警

告警必须可行动，并包含：

- 症状；
- 影响范围；
- 关键指标；
- 关联 Dashboard；
- Runbook；
- 最近部署版本。

初始告警：

- API 错误率/延迟超阈值；
- 数据库连接池饱和；
- Outbox 最老事件超过阈值；
- Job 队列和 stuck job；
- Provider 429/5xx；
- Artifact 错误和容量；
- 一致性扫描异常；
- 备份失败；
- Secret/安全事件；
- 审计写入失败。

## 13. 审计

Audit Event 至少记录：

- actor 和 delegation；
- tenant；
- action；
- resource；
- before/after 摘要或版本；
- reason/comment；
- outcome；
- request/trace；
- occurred_at；
- source channel。

审计失败对高风险写入采用 fail-closed 或可靠 Outbox，具体策略由业务风险定义。

审计记录受严格访问控制、保留和防篡改策略保护。

## 14. Dashboard

至少建立：

- Platform Overview；
- API；
- Database；
- Messaging/Outbox；
- Durable Tasks；
- Document Intelligence；
- External Providers；
- Storage；
- Security；
- Backup/Recovery。

Dashboard 与告警和 Runbook 对应。

## 15. 环境策略

Local：可读日志和可选本地 trace。  
CI：捕获失败测试日志，限制敏感信息。  
Staging：接近生产的指标、追踪和告警演练。  
Production：结构化日志、采样 trace、完整关键指标和审计。

采样不能丢失错误、高延迟和安全事件的必要诊断信息。

## 16. 保留策略

不同数据分别定义：

- 技术日志；
- trace；
- metrics；
- 业务审计；
- 安全事件。

保留时间由故障分析、合规、成本和数据分类共同决定。

## 17. Runbook 关联

每个 P1/P2 告警必须关联 Runbook：

- 判断影响；
- 立即缓解；
- 验证恢复；
- 升级路径；
- 数据修复；
- 复盘输入。

## 18. 测试

- request/trace 传播；
- 消息 context 传播；
- Secret 不泄漏；
- 错误分类和稳定字段；
- 指标注册和高基数检查；
- Audit 生成；
- 告警规则语法；
- Dashboard/Runbook 链接；
- 服务关闭 telemetry flush。

## 19. 验收清单

- [ ] 所有入口具有关联 ID；
- [ ] 用例和外部依赖可追踪；
- [ ] 长任务可从权威状态诊断；
- [ ] 日志无 Secret 和完整敏感内容；
- [ ] 指标覆盖 API、数据、消息、任务和 Provider；
- [ ] 告警可行动并关联 Runbook；
- [ ] Audit 与普通日志分离；
- [ ] 质量属性场景可由指标验证；
- [ ] 最近部署版本可关联异常。

## 20. PLAN-0004 processing fields

Processing logs and metrics use the bounded fields `tenant_id`, `document_id`,
`job_id`, `step_kind`, `attempt_number`, `worker_id`, `lease_fence`, `status`,
`duration_ms`, and `failure_code`. A lease token is represented only by a
short one-way hash prefix. The MVP records job creation/completion/failure/
cancellation, duration, retry, lease-loss, queue-age, and pending-AI-task
signals without logging raw document text, prompts, storage URLs, or secrets.

Revision 1 additionally records UoW action names (`step_started`,
`step_completed`, `ai_task_enqueued`, `ai_task_completed`, `ai_task_failed`,
`review_finalized`, `processing_cancelled`, and reclaim/release actions) in the
tenant-scoped processing audit table. Metrics distinguish retry/backoff,
reclaim, heartbeat loss, stale-fence rejection, and graceful-drain duration;
audit details remain structured and never contain object keys, raw text, lease
tokens, prompts, credentials, or database URLs.
