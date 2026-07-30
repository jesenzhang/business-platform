# 质量属性场景与架构验收目标

> 文档 ID：ARCH-QA-001  
> 版本：1.0  
> 状态：Baseline  
> 生效日期：2026-07-30  
> 适用范围：服务端性能、可用性、可靠性、安全、可维护性、可扩展性与恢复能力

## 1. 目的

架构必须通过可测量场景验收，而不是只描述“高性能”“高可用”“可扩展”。

本文给出初始目标。具体业务上线前可以根据容量评估调整，但降低目标必须经过评审或 ADR。

## 2. 场景格式

每个场景包含：

```text
Source：谁或什么触发
Stimulus：发生什么
Environment：在什么条件下
Artifact：影响哪个系统部分
Response：系统如何响应
Measure：如何判断通过
```

## 3. 性能

### QA-PERF-001 普通查询

- Source：已认证用户；
- Stimulus：查询单个业务对象或标准分页列表；
- Environment：正常负载、依赖健康；
- Response：返回授权范围内的数据；
- Measure：服务端 P95 ≤ 300ms，P99 ≤ 800ms，不含大型文件传输和外部 AI 时间。

### QA-PERF-002 普通写入

- Source：已认证用户；
- Stimulus：创建或更新普通业务聚合；
- Response：完成权限、版本、事务、审计和 Outbox；
- Measure：服务端 P95 ≤ 500ms，P99 ≤ 1.5s。

### QA-PERF-003 大文件上传

- Stimulus：上传允许范围内的大型文档；
- Response：流式接收、校验并写入 Artifact Store；
- Measure：业务进程内存不随文件大小线性增长；单请求额外常驻内存目标 ≤ 16MiB；上传超时和大小限制可配置。

### QA-PERF-004 长任务创建

- Stimulus：发起文档解析或批量任务；
- Response：持久化业务请求和任务，异步执行；
- Measure：P95 ≤ 500ms 返回 `202 + job_id`，不等待完整处理。

### QA-PERF-005 查询分页

- Measure：禁止无界列表；默认分页 ≤ 50，硬上限 ≤ 200；稳定排序；深分页使用游标或受控策略。

## 4. 可用性与可靠性

### QA-REL-001 API 实例重启

- Stimulus：任一 API 实例异常退出或滚动升级；
- Response：其他实例继续服务，在途请求明确成功或失败；
- Measure：不丢失已提交业务状态，不产生未知重复写入。

### QA-REL-002 Worker 崩溃

- Stimulus：Worker 在任务步骤执行中退出；
- Response：lease 过期后由其他 Worker 重新领取；
- Measure：任务最终恢复或进入可解释失败状态；同一有效 claim 不被两个 Worker 同时提交。

### QA-REL-003 消息重复

- Stimulus：同一事件被重复投递；
- Response：消费者幂等处理；
- Measure：不产生重复正式业务副作用。

### QA-REL-004 消息丢失或 Broker 不可用

- Response：权威任务存储和 Outbox 保留状态，恢复后继续发布或扫描；
- Measure：已提交任务不永久丢失。

### QA-REL-005 外部 AI/OCR 不可用

- Response：受控超时、分类重试、限流和熔断；
- Measure：API 核心业务不被线程或连接耗尽；任务状态和错误可查询。

## 5. 恢复与灾难恢复

### QA-DR-001 数据库备份恢复

- Measure：预生产必须完成从备份恢复到新实例；恢复后 Migration、业务抽检和一致性扫描通过。

### QA-DR-002 对象存储恢复

- Measure：对象数量、随机 checksum 和关键文档可访问性通过验证。

### QA-DR-003 初始目标

在正式容量和业务等级确定前采用：

```text
预生产 RPO：24 小时以内
预生产 RTO：4 小时以内
正式生产目标：上线前单独批准
```

关键合同、财务等生产数据若要求更严格目标，应采用 WAL/PITR、对象复制和对应演练。

### QA-DR-004 任务恢复

- Measure：服务整体重启后，Ready、Running lease 过期、RetryScheduled 和 AwaitingExternal 任务能够继续或进入明确人工处理状态。

## 6. 容量与扩展

### QA-CAP-001 文件限制

初始配置必须显式定义：

- 最大文件大小；
- 最大页数；
- 允许格式；
- 单任务最大分块数；
- 临时对象 TTL。

禁止使用无限默认值。

### QA-CAP-002 并发配额

必须支持：

- 单租户并发；
- 单用户并发；
- 单 Provider 并发；
- 全局 Worker 并发；
- 队列积压阈值；
- 费用和 Token 限额。

### QA-CAP-003 水平扩展

- API 尽量无状态；
- Worker 通过 claim/lease 协调；
- 扩容不需要修改业务代码；
- 扩容前后幂等和顺序语义保持一致。

## 7. 安全

### QA-SEC-001 跨租户访问

- Stimulus：租户 A 请求租户 B 资源；
- Response：默认拒绝；
- Measure：API、Application、Repository 和对象 key 测试均证明无法跨租户读取或写入。

### QA-SEC-002 未认证访问

- Response：公开健康端点之外的受保护接口返回 401；生产配置不得静默启用开发认证。

### QA-SEC-003 越权操作

- Response：稳定 403 或业务拒绝，不泄漏目标资源敏感信息。

### QA-SEC-004 Secret 泄漏

- Measure：配置 Debug、错误、日志、trace、CI Artifact 中不出现测试 Secret；建立自动化扫描。

### QA-SEC-005 不可信文档和 AI 输出

- Response：大小、类型、Schema、证据、权限和业务规则校验；
- Measure：外部内容不能直接触发正式业务写入或通用工具执行。

### QA-SEC-006 高风险写入

- Response：Prepare → Preview → Confirm → Execute；
- Measure：确认绑定用户、租户、目标版本和操作摘要，过期或变更后不可复用。

## 8. 可维护性

### QA-MAINT-001 核心独立测试

- Measure：Domain 和核心 Application 用例无需数据库、网络、容器和供应商即可运行单元测试。

### QA-MAINT-002 基础设施替换

- Stimulus：更换对象存储、消息或外部 Provider；
- Response：修改适配器、配置、部署和契约测试；
- Measure：业务不变量和公开用例不需要重写。

### QA-MAINT-003 架构违规反馈

- Measure：CI 在一次构建内发现禁止依赖、跨层引用或未登记迁移，失败信息可定位。

### QA-MAINT-004 变更影响

- Measure：公开 API、事件、Schema、上下文边界和部署变化有对应文档与 ADR；不存在无法追踪的隐式架构变化。

## 9. 可观测性

### QA-OBS-001 请求追踪

- Measure：所有入口生成或传播 request_id/trace_id；数据库、消息、对象存储和外部 Provider 调用可关联。

### QA-OBS-002 长任务追踪

- Measure：可通过 job_id 查询当前步骤、尝试、错误、外部请求引用、业务资源和 trace。

### QA-OBS-003 告警

必须至少覆盖：

- API 高错误率或高延迟；
- 数据库连接池耗尽；
- Outbox 积压；
- 任务积压和超时；
- lease 过期异常；
- 外部 Provider 错误和限流；
- 对象存储失败；
- 备份失败；
- 一致性扫描异常。

## 10. 兼容性

### QA-COMP-001 API

- 非破坏性扩展保持兼容；
- 破坏性变更使用版本和迁移窗口；
- 客户端错误码保持稳定。

### QA-COMP-002 事件

- 消费者可忽略未知可选字段；
- 事件表达已发生事实；
- 破坏性 Schema 变化发布新版本或新事件类型；
- 历史消息可被当前消费者明确处理或拒绝。

### QA-COMP-003 数据迁移

- 使用 expand → migrate → switch → contract；
- 新旧版本在滚动发布期间兼容。

## 11. 测试环境层级

| 目标 | 测试层级 |
|---|---|
| 业务规则 | Domain Unit |
| 用例协调 | Application + Fake Ports |
| SQL、锁、事务 | PostgreSQL Integration |
| 对象协议 | S3/MinIO Contract |
| 消息重复和重放 | Broker Contract |
| API 安全和兼容 | API Test |
| 完整链路 | E2E |
| 恢复和故障 | Staging Exercise |
| 容量和延迟 | Load/Performance Test |

## 12. 架构决策输入

任何技术选型必须说明它如何支持或影响：

- 延迟；
- 吞吐；
- 可用性；
- 一致性；
- RPO/RTO；
- 安全；
- 运维复杂度；
- 成本；
- 可替换性。

仅以流行度或开发便利性不能作为长期架构决策依据。

## 13. 当前验收优先级

PLAN-0001 至少应证明：

- CI 和架构门禁可运行；
- Domain/Application 可独立测试；
- API 安全基线；
- 对象存储流式与契约测试；
- Migration 可重复；
- Outbox 多 Worker 可靠性；
- 首个垂直切片租户、版本、事务和审计正确。

文档解析正式上线前，再补充真实容量、Provider SLA、费用与恢复目标。
