# 项目代码架构规范

> 文档 ID：ARCH-CODE-001  
> 版本：1.0  
> 状态：Baseline  
> 生效日期：2026-07-30  
> 适用范围：整个 Rust Workspace

## 1. 目标

本规范定义代码模块、分层、依赖方向和运行时边界，确保：

- 业务规则不依赖 Web 框架、数据库和外部供应商；
- UI、OpenAPI、Worker 和 Agent 复用同一应用服务；
- 模块化单体可以在需要时安全拆分为微服务；
- 基础设施可替换且可通过真实契约测试验证；
- Codex 等编码 Agent 不会因为局部实现便利破坏长期边界。

## 2. 顶层模型

```text
访问入口
  Web / Mobile / OpenAPI / Worker / Agent
                    │
                    ▼
              Application Use Cases
                    │
                    ▼
                 Domain Model
                    ▲
                    │ 通过端口 trait
                    │
       Infrastructure Adapters / Providers
```

权威规则：

> 所有入口进入同一应用服务；领域层不感知入口和基础设施。

## 3. Workspace 角色

### 3.1 `apps/*`

应用进程是 composition root，只负责：

- 加载和校验配置；
- 初始化 tracing；
- 创建数据库、消息、存储和外部 Provider；
- 将具体实现注入应用服务；
- 启动 HTTP Server 或 Worker Loop；
- 处理优雅关闭；
- 暴露健康检查。

禁止：

- 在 `main.rs` 编写业务规则；
- 在应用进程中直接实现 Repository；
- 在不同进程复制同一业务用例；
- 让配置对象作为全局可变状态传播。

### 3.2 领域 crate

例如：

```text
identity
organization
customer
contract
project
approval
finance
document
notification
audit
```

每个领域 crate 按需要采用：

```text
src/
├── domain/
├── application/
├── infrastructure/
├── api/
└── lib.rs
```

在规模较小时可以减少物理目录，但逻辑依赖必须保持一致。

### 3.3 能力 crate

- `workflow`：长任务、状态机和恢复语义；
- `ai-application`：AI Provider、Pipeline 和候选结果模型；
- `agent-integration`：Agent Skill、ActionPlan 和适配模型；
- `policy`：授权策略模型和适配；
- `object-storage`：对象存储端口及 S3 适配；
- `messaging`：领域事件、Outbox 和消息适配；
- `observability`：tracing、metrics 和 telemetry 初始化。

### 3.4 `shared-kernel`

只允许放置真正跨领域且稳定的纯类型，例如：

- 强类型实体 ID 基础；
- 租户上下文值对象；
- 分页值对象；
- 与协议无关的错误分类基础；
- 时间、版本和幂等值对象。

禁止放置：

- Axum Handler、Response 或 `IntoResponse`；
- SQLx 类型或 `sqlx::Error` 转换；
- HTTP 状态码；
- 外部 SDK 客户端；
- 领域特有 DTO；
- 为避免循环依赖而临时搬入的代码。

当前 `shared-kernel` 对 Axum 和 SQLx 的依赖属于初始实现债务，后续应拆分为协议层错误映射和基础设施层错误转换。

## 4. 分层职责

## 4.1 Domain

包含：

- Aggregate、Entity、Value Object；
- 领域服务；
- 领域事件；
- 领域错误；
- Repository/Provider 端口 trait；
- 业务不变量和状态转换。

允许依赖：

- Rust 标准库；
- `serde`，仅在确有跨边界数据需要时；
- `uuid`、`chrono` 等纯数据类型；
- 最小化 `shared-kernel`；
- 同一领域内的纯模块。

禁止依赖：

- Axum、Tower；
- SQLx；
- Reqwest；
- AWS/MinIO SDK；
- NATS/Kafka SDK；
- OpenTelemetry；
- 具体 LLM/OCR SDK；
- `apps/*`；
- 其他领域的 infrastructure/api。

领域对象必须能在不启动数据库、网络或 Tokio Runtime 的情况下测试。

## 4.2 Application

包含：

- Command/Query Use Case；
- 应用服务；
- 输入、输出模型；
- 权限和事务边界编排；
- 跨端口协调；
- 幂等、版本检查和审计触发。

Application 可以依赖 Domain 和端口 trait，不依赖具体适配器。

应用服务的推荐流程：

```text
验证调用上下文
→ 权限检查
→ 加载聚合
→ 执行业务方法
→ 持久化
→ 写 Outbox / Audit
→ 提交事务
→ 返回结果
```

禁止：

- 返回 Axum Response；
- 读取 Header/Cookie；
- 直接调用全局连接池；
- 在应用层拼接供应商 HTTP 请求；
- 将数据库模型直接返回给 API。

## 4.3 Infrastructure

包含：

- SQLx Repository；
- NATS/Kafka Publisher 和 Consumer；
- S3/Object Storage 适配；
- LLM/OCR HTTP Provider；
- OIDC、邮件和第三方系统适配；
- 数据库行与领域模型转换。

Infrastructure 实现 Domain/Application 定义的端口。

禁止让基础设施类型向上泄漏：

- `PgPool` 不进入 Handler 或 Domain；
- `sqlx::Error` 不作为公开错误；
- `reqwest::Response` 不进入 Application；
- 供应商 DTO 不进入领域模型。

## 4.4 API

包含：

- 路由；
- Request/Response DTO；
- 协议校验；
- 身份和租户上下文提取；
- 应用错误到 HTTP 错误的映射；
- OpenAPI 描述；
- SSE/WebSocket 协议适配。

Handler 应保持薄：

```text
提取参数
→ 构造应用命令
→ 调用 Use Case
→ 映射响应
```

禁止：

- 在 Handler 中写 SQL；
- 在 Handler 中直接使用 `PgPool`；
- 在 Handler 中实现状态机；
- 在 Handler 中调用 LLM/OCR；
- 在 Handler 中决定领域权限。

当前 `AppState` 直接公开 `PgPool` 属于过渡实现。目标状态应注入用例接口或应用服务集合。

## 5. 依赖方向

允许：

```text
apps
  → api / infrastructure / application / domain

api
  → application / protocol DTO

infrastructure
  → application ports / domain

application
  → domain / ports

domain
  → minimal shared-kernel
```

禁止：

```text
domain → infrastructure
application → api
application → SQLx/Reqwest
shared-kernel → Axum/SQLx
领域 A infrastructure → 领域 B infrastructure
```

跨领域调用优先使用：

1. 对方公开的 Application API；
2. 明确的领域端口；
3. 领域事件，用于异步解耦。

禁止直接查询其他领域私有表来绕过应用服务。

## 6. 模块化单体与微服务

当前默认是模块化单体。crate 是代码边界，不自动等于部署服务。

只有满足至少一个客观条件时考虑拆分：

- 独立扩缩容；
- 独立安全边界；
- 独立故障隔离；
- 独立发布周期；
- 独立数据所有权；
- 特殊硬件或运行时；
- 单体边界已经造成可测量的交付或运行问题。

拆分必须通过 ADR，说明：

- 数据所有权；
- 同步和异步协议；
- 失败语义；
- 一致性和补偿；
- 可观测性；
- 迁移和回滚。

## 7. 数据访问

### 7.1 PostgreSQL

- PostgreSQL 是权威业务状态；
- Repository 对外返回领域对象或应用 DTO；
- 正式写入必须在应用服务定义事务边界；
- 重要聚合采用乐观锁；
- 数据库迁移不可修改历史文件；
- 跨业务写入与事件使用 Outbox。

### 7.2 Outbox

目标模型必须支持：

- 事件唯一 ID；
- claim/lease 或 `FOR UPDATE SKIP LOCKED`；
- 发布尝试次数；
- 下一次重试时间；
- 最后错误；
- 发布时间；
- 多 Worker 安全并发；
- 消费端幂等。

当前 `fetch_unpublished + published bool` 仅适合作为概念骨架，不能直接用于多 Worker 生产环境。

### 7.3 对象存储

- 使用正式 S3 SDK 或完整兼容实现；
- 不自行实现 AWS Signature V4；
- 支持流式 I/O，不默认将大文件全部载入内存；
- object key 必须进行租户、资源和版本约束；
- Local Adapter 必须阻止绝对路径和 `..` 路径穿越；
- 异步代码不直接执行阻塞式 `std::fs`。

当前 `S3Client` 未实现签名和真实预签名 URL，只能视为 Stub；当前 LocalStorage key 未做安全规范化，不能接收不可信 key。

## 8. 配置架构

配置分为：

- 公共运行配置；
- 每个应用进程专属配置；
- Secret；
- 测试配置。

规则：

- 不要求每个进程加载自己不使用的配置；
- Secret 类型不派生或输出完整 `Debug`；
- 日志不得打印连接字符串、密码、Token 和密钥；
- 启动时进行完整配置校验，失败即退出；
- 生产 Secret 通过环境或 Secret Manager 注入。

## 9. HTTP 和中间件

统一顺序应明确并通过测试：

```text
Request ID
→ Trace
→ 安全 Header
→ CORS
→ Body Limit
→ Timeout
→ Authentication
→ Tenant Context
→ Authorization
→ Handler
```

规则：

- 生产默认禁止 `CorsLayer::permissive()`；
- CORS 使用配置化白名单；
- readiness 不向客户端输出底层错误；
- 错误响应包含稳定错误码和 trace/request ID；
- `/health/live` 只表示进程存活；
- `/health/ready` 检查必要依赖和迁移版本。

## 10. AI 应用边界

- AI Provider 位于 infrastructure；
- Prompt、Pipeline 和候选结果模型位于 `ai-application`；
- AI 输出不得直接修改业务表；
- 应用服务负责 Schema、证据、权限、冲突和版本校验；
- 记录 Provider、模型、Prompt、Schema、Token、费用和 trace；
- 大多数测试使用 Mock Provider，供应商兼容测试独立运行。

## 11. Agent 边界

Agent 是可选入口，不是业务核心。

```text
Agent Runtime
→ agent-adapter
→ Application Use Case
```

Agent 只获得业务级 Skill，不获得：

- 通用 SQL；
- Shell；
- 任意文件系统；
- 任意 HTTP；
- 生产数据库凭证。

高风险写操作使用：

```text
Prepare → Preview → Confirm → Execute
```

## 12. 可观测性

每个应用进程必须统一初始化：

- service name/version；
- request/trace ID；
- structured logs；
- metrics；
- graceful flush；
- 依赖调用 span。

禁止日志中记录：

- 完整文档；
- Prompt 全文，除非脱敏且明确允许；
- Access Token；
- 数据库 URL；
- 对象存储密钥；
- 用户敏感字段。

## 13. 测试架构

- Domain：纯单元测试；
- Application：Fake Port 测试；
- Repository：真实 PostgreSQL 集成测试；
- Object Storage：Memory 与 MinIO 共用契约测试；
- Messaging：真实 Broker 的重复和重放测试；
- API：路由、认证、错误和协议测试；
- E2E：完整文档处理和审计链路。

## 14. 当前允许的过渡状态

初始骨架中允许暂时存在：

- 尚未实现的领域 crate；
- 尚未启动循环的 Worker；
- 未完成的 Migration 应用；
- Noop Publisher；
- 健康检查先只验证数据库。

但必须满足：

- 不将占位实现描述为生产能力；
- 不在占位实现之上继续扩展错误抽象；
- 在首个对应实施计划中修正本规范已标记的边界债务。

## 15. 架构验收清单

- [ ] Domain 无 Axum、SQLx、Reqwest 和供应商 SDK；
- [ ] Handler 不直接访问数据库；
- [ ] Application 定义事务和权限边界；
- [ ] Infrastructure 类型不向上泄漏；
- [ ] `shared-kernel` 保持纯净和最小；
- [ ] UI、Worker、OpenAPI 和 Agent 复用应用服务；
- [ ] 对象存储使用真实 S3 签名实现；
- [ ] Local Storage 不存在路径穿越；
- [ ] Outbox 支持多 Worker 安全处理；
- [ ] 生产 CORS、错误和 Secret 行为安全；
- [ ] 关键依赖具有真实集成测试。

## 16. Runtime configuration boundary

Runtime configuration is owned by each `apps/*` composition root. A process
may compose small runtime value types, but `shared-kernel`, Domain, and
Application do not load environment variables, configuration files, or
infrastructure topology. `AppState` contains no full configuration object,
database pool, or secret connection URL.
