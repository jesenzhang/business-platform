# 服务端部署架构

> 文档 ID：ARCH-DEPLOY-001  
> 版本：1.0  
> 状态：Baseline  
> 生效日期：2026-07-30  
> 适用范围：开发、测试、预生产与生产环境的进程、节点、网络和扩缩容

## 1. 原则

- 业务边界不自动等于部署边界；
- 模块化单体优先，按客观运行需求拆分；
- API 尽量无状态，任务状态外部持久化；
- 数据、消息、对象和外部 Provider 通过适配器接入；
- 配置和 Secret 按环境和进程隔离；
- 所有部署单元支持健康检查、优雅关闭和可观测性；
- 环境越接近生产，持久化、权限和恢复要求越严格。

## 2. 初始部署单元

### `business-api`

职责：

- HTTP/OpenAPI；
- 认证、租户和请求上下文；
- 同步业务用例；
- 长任务创建、查询和取消；
- SSE/WebSocket；
- 健康检查。

不执行长时间 OCR、LLM 或批处理。

### `business-worker`

职责：

- 业务后台用例；
- Process Manager；
- Outbox 发布；
- 通知和补偿；
- 恢复与一致性扫描；
- 定时任务。

### `ai-worker`

职责：

- OCR、文档解析、LLM/VLM、Embedding；
- 资源限制和 Provider 并发；
- 处理产物和候选结果；
- 不直接修改目标业务上下文正式数据。

### `migration`

职责：

- 数据库 schema 迁移；
- 状态和兼容性检查；
- 只在受控部署阶段运行；
- 使用独立权限。

### `agent-adapter`

可选部署单元：

- 将业务 Application API 暴露为 Skill/Tool；
- 传播委托身份；
- 预览、确认和审计；
- 不拥有业务数据。

## 3. 初始拓扑

```text
Client / Web / Agent
        ↓
Reverse Proxy / API Gateway
        ↓
 business-api × N
        ↓
Application / Domain
        ↓
Persistence / Message / Artifact / Providers

 business-worker × N
 ai-worker × N
 migration（部署阶段）
```

## 4. 模块化单体映射

初期多个 Bounded Context 可以由同一 `business-api` 和数据库实例承载，但必须保持：

- 代码边界；
- 数据所有权；
- 公开 Application API；
- 独立迁移归属；
- 不跨上下文直接写入。

共享进程是部署优化，不是取消边界。

## 5. 拆分条件

只有满足至少一项可测量需求时拆分独立服务：

- 独立扩缩容；
- 特殊 CPU/GPU/内存需求；
- 独立安全或网络边界；
- 独立故障隔离；
- 独立发布周期；
- 明确数据所有权和团队责任；
- 单体已造成可测量交付或运行问题。

拆分必须通过 ADR，包含协议、数据迁移、一致性、可观测性和回滚。

## 6. 环境

### Local Development

- Docker Compose；
- 本地 PostgreSQL、S3 兼容存储和消息系统；
- 开发专用凭证；
- 仅绑定本机；
- 可一键重建。

### Automated Test

- Unit 不依赖外部服务；
- Integration 使用 Testcontainers；
- E2E 使用隔离 Compose；
- 测试完成自动清理。

### Staging

- 独立持久化数据；
- 独立凭证和网络；
- 具备监控、备份、恢复和故障演练；
- 使用脱敏数据；
- 与生产拓扑和配置语义尽量一致。

### Production

- 高可用目标按质量属性批准；
- Secret Manager；
- TLS 和网络隔离；
- 自动备份和恢复演练；
- 变更审批和审计；
- 容量、配额和告警。

## 7. 网络分区

建议分区：

```text
Ingress Zone
Application Zone
Data Zone
Management/Observability Zone
External Provider Egress
```

要求：

- 数据库、Broker 和对象存储不直接暴露公网；
- Worker 不开放不必要的入站端口；
- 外部 Provider 出站使用允许列表、代理或受控 egress；
- 管理端口和业务端口分离；
- 健康和指标端点有网络访问控制。

## 8. 配置

- 公共配置与进程配置分离；
- Secret 单独注入；
- 启动时完整校验，失败即退出；
- 不为缺失生产配置使用不安全默认值；
- 配置版本和环境可追踪；
- Feature Flag 不能绕过权限和数据迁移。

## 9. 健康检查

### Liveness

只证明进程和运行时存活，不执行昂贵依赖检查。

### Readiness

检查：

- 必需配置；
- 数据库连接和 migration 兼容；
- 必需依赖的基本可用性；
- 应用未进入 shutdown；
- Worker 是否可领取任务。

### Startup

用于冷启动和初始化时间较长场景，避免误杀。

健康响应不得泄漏底层地址、凭证和完整错误。

## 10. 优雅关闭

API：

- 停止接收新流量；
- 完成受控范围内在途请求；
- 关闭连接；
- flush telemetry。

Worker：

- 停止领取新任务；
- 尝试完成当前可短时完成步骤；
- 续租或安全释放；
- 无法完成时让 lease 过期恢复；
- 不在失去 lease 后提交结果。

## 11. 扩缩容

### API

基于：

- CPU；
- 请求率；
- P95 延迟；
- 连接数。

### Worker

基于：

- Ready/Retry 队列长度；
- 最老任务等待时间；
- Provider 限额；
- CPU/内存；
- 单租户公平性。

AI Worker 扩容必须受 Provider 和费用上限约束。

## 12. 资源治理

每个进程配置：

- CPU/内存 request/limit；
- 最大连接；
- 最大在途请求；
- body/file size；
- 外部调用并发；
- Worker 并发；
- 超时；
- 缓冲和日志上限。

内存不足和磁盘不足必须有告警和明确失败行为。

### PLAN-0004 runtime profile

The local profile runs SQLite, local object storage, and one inline
`business-worker` in an isolated directory. SQLite rejects production mode,
parallel workers, and separate AI mode. Production selects PostgreSQL and an
S3-compatible private bucket (MinIO in CI); the business worker and
independent AI worker use durable job/AI-task tables, lease fencing, and
graceful shutdown. MinIO is never represented as a local SQLite equivalent.

Revision 1 requires PostgreSQL pool capacity for concurrent UoW transactions
and runs the business and AI workers as independent processes. A worker stops
new claims on shutdown, joins heartbeat/drain tasks, and leaves expired leases
for the recovery scanner. SQLite remains one process, one inline worker, and
uses `BEGIN IMMEDIATE`; production or separate-AI settings fail closed.

## 13. 数据服务

### 权威状态存储

- 持久卷；
- 连接池；
- 备份、PITR 目标和恢复演练；
- 独立 Migration 权限；
- 慢查询和容量监控。

### Artifact Store

- 私有访问；
- 版本、生命周期和复制；
- checksum；
- 临时区清理；
- 容量和请求错误监控。

### Message Broker

- 持久化和保留；
- consumer/stream 权限；
- 积压和重放；
- Dead Letter；
- 重复投递可接受。

具体产品由 ADR 和部署配置决定。

## 14. 发布策略

推荐：

```text
CI 构建不可变镜像
→ 安全扫描和 SBOM
→ Staging 部署
→ Migration 兼容检查
→ Smoke/E2E
→ Production 滚动发布
→ 指标观察
```

数据库变更采用 expand → migrate → switch → contract，保证滚动期间兼容。

## 15. 回滚

- 应用镜像可回滚到上一兼容版本；
- 已执行 Migration 不依赖简单 down 回滚，优先向前修复；
- 破坏性清理在兼容窗口后执行；
- Feature Flag 回滚不能破坏已写数据；
- 回滚前确认事件和任务 Schema 兼容；
- 记录回滚触发条件和验证步骤。

## 16. 备份和恢复

部署方案必须包含：

- 数据库逻辑/物理备份；
- WAL/PITR，生产按目标采用；
- Artifact 复制或备份；
- 配置和 Bucket Policy 备份；
- 恢复到新环境；
- 一致性扫描；
- 恢复后的应用和任务验证。

未演练恢复的备份不视为有效。

## 17. 多租户容量隔离

- 配额按租户配置；
- 限流和并发公平；
- 大租户不能耗尽全局 Worker；
- 导出、解析和批量操作单独限额；
- 存储和费用可按租户度量；
- 资源耗尽时优先保护核心同步业务。

## 18. 故障隔离

- 外部 AI 失败不拖垮普通业务 API；
- AI Worker 与 API 分离资源池；
- 重任务使用队列和并发限制；
- 慢 Provider 使用隔离连接池/限流；
- 单个异常任务不能阻塞整个队列；
- 运维扫描和补偿有独立优先级。

## 19. 部署验收

- [ ] 所有进程具有 liveness/readiness；
- [ ] 优雅关闭经过验证；
- [ ] API 和 Worker 可水平扩展；
- [ ] 数据服务不暴露公网；
- [ ] Secret 与配置分离；
- [ ] Migration 支持滚动兼容；
- [ ] 备份恢复和故障演练通过；
- [ ] 资源限额和多租户配额明确；
- [ ] 外部 Provider 故障被隔离；
- [ ] 部署变化不破坏 Bounded Context 和数据所有权。
