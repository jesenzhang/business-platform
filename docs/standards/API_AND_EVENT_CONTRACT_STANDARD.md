# API 与事件契约规范

> 文档 ID：STD-CONTRACT-001  
> 版本：1.0  
> 状态：Baseline  
> 生效日期：2026-07-30  
> 适用范围：HTTP API、Worker 命令、领域事件、集成事件与回调协议

## 1. 原则

- API 和事件是正式架构资产，不是 Controller 或 Broker 的内部细节；
- 对外契约使用业务语言，不暴露数据库表和供应商 DTO；
- 命令表达意图，事件表达已经发生的事实；
- 契约必须版本化、可兼容、可测试、可追踪；
- Web、Agent、Worker 和第三方入口复用同一 Application Use Case；
- 协议层不得承载业务规则。

## 2. 命令、查询与事件

### Command

请求系统改变状态，例如：

- CreateDocumentMetadata；
- StartDocumentProcessing；
- ApplyContractFieldSuggestions；
- RequestApproval。

Command 必须明确主体、租户、目标、期望版本和幂等语义。

### Query

读取业务状态或 Read Model，不产生正式副作用。

### Domain Event

表达领域内部已经发生的业务事实，例如：

- ContractApproved；
- DocumentVersionCreated；
- ExtractionSuggestionGenerated。

### Integration Event

为跨上下文或外部系统发布的稳定、版本化事实。可以由 Domain Event 映射生成，不直接暴露内部聚合结构。

### Execution Event

表达任务执行事实，例如 StepRetryScheduled。它不等同于业务事件。

## 3. HTTP 路径

推荐：

```text
/api/v1/{resources}
/api/v1/{resources}/{id}
/api/v1/{resources}/{id}/{subresources}
/api/v1/{resources}/{id}:action
```

动作型端点用于无法自然表示为资源创建或更新的明确业务命令。

禁止把技术实现写入路径，例如：

```text
/run-sql
/send-nats
/call-ocr-http
```

## 4. 请求上下文

所有受保护 API 通过安全中间件建立：

- Principal；
- TenantContext；
- RequestId；
- TraceContext；
- Locale，可选；
- DelegationContext，可选。

业务请求体不应允许客户端任意覆盖服务端已验证的主体和租户。

## 5. API 版本

- 主版本出现在路径或明确媒体类型中；
- 非破坏性字段扩展不提升主版本；
- 删除、重命名、改变语义或收紧有效值属于破坏性变化；
- 破坏性变化需要新版本、兼容窗口和迁移说明；
- 旧版本废弃前必须有使用情况、通知和关闭日期。

## 6. 请求和响应 DTO

- DTO 与 Domain Model、数据库 Row 分离；
- 输入字段使用允许列表，防止 Mass Assignment；
- 明确必填、可选和 nullable；
- 时间使用带时区 ISO-8601；
- ID 使用稳定字符串格式；
- 金额使用 decimal + currency，不使用浮点数；
- 枚举新增值时考虑旧客户端行为；
- 大型内容使用上传或 ArtifactRef，不嵌入普通 JSON。

## 7. 统一响应

成功响应应保持资源或结果结构清晰，不强制无意义的多层包装。

异步操作返回：

```json
{
  "job_id": "...",
  "status": "pending",
  "status_url": "/api/v1/jobs/..."
}
```

并使用 `202 Accepted`。

## 8. 错误模型

统一错误至少包含：

```json
{
  "code": "DOCUMENT_VERSION_CONFLICT",
  "message": "...",
  "request_id": "...",
  "trace_id": "...",
  "details": {}
}
```

要求：

- `code` 稳定且可程序处理；
- `message` 面向用户或调用方；
- `details` 只包含安全、结构化信息；
- 不暴露 SQL、连接字符串、Token、堆栈或供应商原始错误；
- Validation Error 指明字段和规则；
- Not Found 与 Forbidden 的表现需避免资源枚举。

## 9. 状态码

- 200：成功查询或命令结果；
- 201：资源创建；
- 202：异步受理；
- 204：成功且无响应体；
- 400：协议或请求结构错误；
- 401：未认证；
- 403：已认证但无权限；
- 404：资源不存在或不可见；
- 409：版本、幂等或业务冲突；
- 412：前置版本条件失败，可选；
- 422：结构合法但业务输入不可处理；
- 429：限流；
- 500：内部错误；
- 502/503/504：依赖或服务可用性问题。

具体业务拒绝可使用 409 或 422，但必须统一。

## 10. 分页、过滤和排序

- 所有列表有默认和最大分页；
- 排序字段使用允许列表；
- 返回稳定排序和 continuation token/cursor；
- HTTP cursors are opaque, versioned tokens. PLAN-0003 uses a v1 JSON payload
  encoded as unpadded base64url; malformed, oversized or unknown-version tokens
  return 400. Database keyset fields are never accepted as separate public
  query parameters.
- 过滤语义公开且版本化；
- 禁止把任意 SQL 条件暴露为过滤接口；
- 跨租户过滤永远不可用。

Document responses must not expose `object_key`, bucket names, storage keys or
internal filesystem paths. Those values remain adapter/application internals.

## 11. 乐观锁

重要资源更新必须携带版本：

- request body 中 `expected_version`；或
- `If-Match`/ETag。

版本冲突返回稳定错误，不自动覆盖。

## 12. API 幂等

可重试写命令接受 `Idempotency-Key`。

要求：

- 作用域包含租户、主体和用例；
- 相同键和相同请求返回原结果；
- 相同键但请求摘要不同返回冲突；
- 保存有效期和状态；
- 客户端超时后可安全查询结果。

## 13. 批量接口

批量命令必须定义：

- 全部成功或部分成功；
- 单项错误结构；
- 最大数量；
- 幂等；
- 异步处理阈值；
- 结果查询和导出。

大批量操作优先创建长时任务。

## 14. SSE 与 WebSocket

用于进度、状态和通知时：

- 事件包含 sequence/event_id；
- 支持断线重连和 last-event-id；
- 不把实时通道作为权威状态；
- 客户端重连后可以通过查询恢复；
- 每条消息执行租户和资源授权；
- 限制连接数、心跳和缓冲。

## 15. 事件命名

事件使用过去时业务事实：

```text
DocumentVersionCreated
DocumentProcessingRequested
ExtractionSuggestionGenerated
ApprovalDecisionRecorded
ContractFieldsApplied
```

禁止：

```text
InsertDocumentRow
CallOcr
SendNatsMessage
UpdateTable
```

执行事件应明确命名空间，避免与业务事件混淆。

## 16. 事件 Envelope

集成事件至少包含：

```text
event_id
event_type
schema_version
occurred_at
producer
correlation_id
causation_id
trace_id
tenant_id
subject_ref
payload
```

敏感内容最小化，大型内容使用 ArtifactRef。

## 17. 事件版本与兼容

- 消费者忽略未知可选字段；
- 新增可选字段通常兼容；
- 改变字段语义、类型或必填性属于破坏性变更；
- 破坏性变更发布新 schema_version 或新事件类型；
- 发布方在兼容期可双写；
- 消费方必须声明支持版本；
- 历史消息重放行为经过测试。

## 18. 事件顺序

默认不承诺全局顺序。

需要顺序时明确：

- 按 aggregate_id 或 subject_ref 分区；
- 使用 aggregate_version；
- 消费者检测缺失和乱序；
- 不以 Broker 到达顺序替代业务版本。

## 19. 事件幂等

- event_id 全局唯一；
- 消费者记录处理结果或业务唯一键；
- 重复事件不得产生重复正式副作用；
- 处理失败必须区分可重试和永久失败；
- Dead Letter 不自动成为业务终态。

## 20. 回调协议

第三方回调必须：

- 验证签名、时间戳和重放窗口；
- 使用 correlation/provider_request_id；
- 快速返回受理结果；
- 不直接信任回调内容完成正式业务写入；
- 支持重复回调；
- 保存原始摘要和验证结果；
- 通过应用用例推进状态。

## 21. Anti-Corruption Layer

外部 DTO、枚举和错误在 Adapter/Translator 中转换。

内部核心不得引用：

- 供应商字段名；
- HTTP Response；
- SDK Result；
- 外部数据库 Row；
- 遗留系统状态码。

## 22. OpenAPI 与 Schema

- OpenAPI 从代码或契约生成并进入 CI；
- 破坏性差异自动检测；
- 示例不得包含真实敏感数据；
- 事件 Schema 使用 JSON Schema/Protobuf 等可验证形式；
- 契约与实现同一变更更新。

## 23. 测试

至少包括：

- Request/Response Schema；
- 错误码；
- 认证、租户和权限；
- 幂等；
- 乐观锁；
- 分页稳定性；
- API 版本兼容；
- 事件重复、乱序和重放；
- 未知字段；
- 回调签名和重放；
- ACL 转换。

## 24. 变更要求

修改以下内容必须同步规范、OpenAPI/Schema、测试和变更说明：

- 公开路径和动作；
- 错误码；
- 事件类型或版本；
- 幂等和并发语义；
- 租户和授权范围；
- 回调和第三方协议。

破坏性变化或全局策略变化必须通过 ADR。
