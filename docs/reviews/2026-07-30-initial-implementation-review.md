# 初始实现审查报告

> 文档 ID：REVIEW-2026-07-30-001  
> 审查对象：提交 `9a55d2332403c77a92f03fcc76a46341770fcf60`  
> 审查日期：2026-07-30  
> 结论：可作为项目骨架接受，不可作为生产实现接受

## 1. 审查范围

本次静态审查覆盖：

- Workspace 和依赖定义；
- `business-api` 启动、路由、状态和健康检查；
- `shared-kernel` 配置与统一错误；
- `object-storage`；
- `messaging` 和 Outbox；
- `observability`；
- Worker、Migration 和 Agent Adapter 骨架；
- 总体架构和基础设施文档。

仓库未提供 GitHub Actions 状态、构建日志或测试报告；提交上没有已登记的 CI 检查。本次结论不代表 `cargo check`、Clippy 或测试已经通过。

## 2. 正面结论

初始提交已正确完成以下基础工作：

1. 建立了清晰的 Cargo Workspace 和应用/领域 crate 初始边界；
2. 明确区分 `business-api`、Worker、AI Worker、Agent Adapter 和 Migration；
3. 使用 Axum、Tokio、Tower、SQLx 和 tracing 作为统一技术基座；
4. 初步建立统一配置、错误模型、实体 ID、租户、分页和响应类型；
5. 引入 Outbox 概念而不是直接在业务事务中发布消息；
6. 对象存储和消息能力已有 trait 抽象意识；
7. 总体架构坚持“业务平台为主体、Agent 为可插拔入口”；
8. 基础设施方案覆盖本地、集成测试、CI、预生产和恢复。

因此，该提交可以作为 **Phase 0 工程骨架** 保留。

## 3. 阻断项

### B-01：S3 客户端并未实现可用的 S3 认证

位置：`crates/object-storage/src/client.rs`

现状：

- `access_key` 和 `secret_key` 未参与请求；
- PUT/GET/DELETE/HEAD 只是普通 HTTP 请求；
- `x-amz-content-sha256: UNSIGNED-PAYLOAD` 不能替代 Signature V4；
- 预签名 URL 返回 `X-Amz-Signature=TODO`；
- object key 未进行 URL 编码；
- 客户端名称和注释却将其描述为 MinIO/S3 客户端。

影响：

- 默认私有 MinIO/S3 bucket 无法工作；
- 预签名 URL 不可用；
- 容易让后续代码误认为存储能力已经完成；
- 若为使其工作而开放匿名 bucket，将造成严重安全问题。

处理要求：

- 将当前实现明确标记为 Stub，不能接入正式路径；
- 使用成熟 S3 SDK 实现签名、流式上传、错误映射和预签名 URL；
- 增加真实 MinIO 契约测试；
- 不自行实现 Signature V4。

优先级：P0，任何文档上传功能开始前完成。

### B-02：本地存储存在路径穿越风险

位置：`LocalStorageClient::object_path`

现状：

```text
base_dir.join(key)
```

没有拒绝：

- `../`；
- 绝对路径；
- Windows 盘符和 UNC 前缀；
- 规范化后逃出 `base_dir` 的路径。

影响：

如果 key 可受用户、导入数据或 Agent 影响，可能读写或删除存储根目录外的文件。

处理要求：

- 引入经过验证的 `ObjectKey` 值对象；
- 拒绝绝对路径、父目录和平台前缀；
- 规范化后证明目标仍位于根目录；
- 增加 Linux/Windows 路径穿越测试。

优先级：P0，在任何不可信 key 进入实现前完成。

## 4. 高优先级架构问题

### H-01：`shared-kernel` 被 Web 和数据库框架污染

位置：

- `crates/shared-kernel/Cargo.toml`
- `crates/shared-kernel/src/error.rs`

现状：

- `shared-kernel` 直接依赖 Axum、SQLx 和 tracing；
- `AppError` 实现 `IntoResponse`；
- `From<sqlx::Error>` 位于共享核心。

影响：

- Domain 依赖共享核心时会间接耦合 HTTP 和数据库；
- 无法保持领域层纯净；
- API 和基础设施错误语义混合；
- 后续 crate 容易把 `AppError` 当作万能错误。

处理要求：

- `shared-kernel` 只保留纯值对象和稳定错误分类；
- HTTP 映射移动到 API 层；
- SQLx 错误转换移动到 Repository/Infrastructure；
- Domain、Application 和 API 使用分层错误。

优先级：P1，在第一个真实领域实现前修正。

### H-02：API 层直接暴露 `PgPool`

位置：`apps/business-api/src/state.rs`

现状：`AppState` 公开 `PgPool`，readiness Handler 已直接执行 SQL。

影响：

- 后续 Handler 很容易直接查询数据库；
- 应用服务和 Repository 边界被绕过；
- 测试需要真实数据库才能覆盖 Handler；
- UI、Worker 和 Agent 难以复用同一用例。

处理要求：

- 业务 Handler 注入应用用例或 facade；
- 数据库连接池只存在于 composition root 和 infrastructure；
- 健康检查使用独立 DependencyHealth 接口。

优先级：P1。

### H-03：默认开放所有 CORS

位置：`apps/business-api/src/routes/mod.rs`

现状：`CorsLayer::permissive()` 无条件启用。

影响：

- 未来使用 Cookie、浏览器 Token 或内部 API 时扩大攻击面；
- 开发默认可能未经注意进入预生产或生产。

处理要求：

- 默认拒绝跨域；
- 开发环境按明确 origin 白名单开启；
- 生产配置启动时校验；
- CORS 策略增加测试。

优先级：P1，在开放真实 API 前完成。

### H-04：认证配置存在，但没有认证和租户中间件

位置：`AppConfig::auth`、Router

现状：加载了 OIDC issuer 配置，但请求链没有认证、用户或租户上下文。

影响：

当前只有健康接口，尚未直接造成业务越权；但如果后续直接 nest 业务路由，将形成默认匿名 API。

处理要求：

- 业务 Router 与公开健康 Router 分离；
- 默认所有 `/api/v1` 路由需要身份和租户；
- 权限仍由应用服务二次验证；
- 测试匿名、错误租户和过期 Token。

优先级：P1，在首个业务 API 前完成。

### H-05：readiness 暴露底层数据库错误

位置：`apps/business-api/src/routes/health.rs`

现状：数据库错误通过 `format!("error: {e}")` 返回客户端。

影响：

可能暴露主机、数据库名、连接状态和内部错误细节。

处理要求：

- 响应只返回稳定状态，例如 `database: unavailable`；
- 详细错误写入受控日志；
- 附带 request/trace ID；
- readiness 最终检查迁移、对象存储和必要消息依赖。

优先级：P1。

### H-06：配置 Secret 可被 `Debug` 输出

位置：`AppConfig`、`StorageConfig`、`AuthConfig`、`S3Config`

现状：配置结构派生 `Debug`，包含对象存储密钥和开发 JWT Secret。

影响：

任何调试日志、panic 或结构化输出都可能泄漏 Secret。

处理要求：

- 使用脱敏 Secret 类型；
- 自定义 Debug 或禁止对聚合配置打印 Debug；
- 错误消息不包含完整连接字符串；
- 增加 Secret 不出现在日志中的测试。

优先级：P1。

### H-07：LocalStorage 在 async 方法中执行阻塞 I/O

位置：`crates/object-storage/src/client.rs`

现状：异步 trait 方法内部使用 `std::fs`。

影响：

文件 I/O 会阻塞 Tokio Worker，降低并发并造成尾延迟。

处理要求：使用 `tokio::fs`、流式接口或隔离到 `spawn_blocking`。

优先级：P1。

### H-08：Outbox 不支持多 Worker 安全 claim

位置：`crates/messaging/src/outbox.rs`

现状：

- 只查询 `published = false`；
- 没有 claim/lease；
- 没有 `FOR UPDATE SKIP LOCKED`；
- 没有尝试次数、下一次重试、最后错误和发布时间；
- 多 Worker 可同时获取相同事件。

影响：

- 重复发布；
- 热循环处理失败事件；
- 无法可靠恢复和观测；
- `published` 布尔值不足以表达状态。

处理要求：

- 设计 claim/lease 或事务锁定模型；
- 增加重试字段和失败状态；
- Publisher 使用幂等 event ID；
- Consumer 必须幂等；
- 增加多 Worker 和崩溃恢复测试。

优先级：P1，在启用消息发布前完成。

## 5. 中优先级问题

### M-01：Worker、Migration 和 Agent Adapter 会立即退出

这些进程当前只记录一条日志或甚至未初始化 tracing，随后返回成功。

结论：它们是项目占位符，不是可运行部署单元。部署配置、README 和健康检查不得把它们描述为已完成服务。

### M-02：Migration 应用未实现

总体和基础设施文档把 migration 视为首阶段能力，但程序尚未读取配置、连接数据库或执行状态/升级命令。

### M-03：配置模型与目标基础设施不一致

当前 `StorageConfig` 只有单 bucket，而基础设施方案定义 documents/temp/exports/backups 多 bucket；配置也要求所有应用加载 storage/auth，即使某个 Worker 不需要。

建议按应用进程拆分配置，并引入多 bucket 逻辑配置。

### M-04：对象存储默认使用 `Vec<u8>`

这会把完整文档加载到内存，不符合大文件和高并发目标。接口应支持流式读取、写入、长度和 checksum。

### M-05：HTTP Timeout 使用 408

服务端内部处理超时通常应映射为网关/服务超时语义，而不是客户端请求超时。需要统一错误码和 HTTP 映射策略。

### M-06：健康接口与文档路径不一致

当前 liveness 为 `/health`，文档基线为 `/health/live`。应统一协议，避免部署探针和文档分离。

### M-07：缺少可验证的工程门禁

未发现：

- GitHub Actions；
- `rust-toolchain.toml`；
- Workspace lint；
- `rustfmt.toml`；
- 测试和覆盖策略；
- 构建、Clippy 或测试证据。

## 6. 结论

### 6.1 接受范围

可以接受：

- Workspace 目录和 crate 初始划分；
- 技术栈方向；
- `business-api` 基础启动骨架；
- Outbox、对象存储、可观测性等概念接口；
- 总体架构和基础设施规划。

### 6.2 不接受为已完成能力

不能视为已完成：

- MinIO/S3；
- 预签名 URL；
- 安全本地对象存储；
- 多 Worker Outbox；
- 认证和租户；
- Migration；
- Worker；
- Agent Adapter；
- 领域业务；
- 生产健康检查；
- CI 和质量门禁。

## 7. 建议修复顺序

```text
P0
1. 对象存储安全边界和真实 S3 SDK
2. LocalStorage 路径穿越

P1
3. shared-kernel 去框架化
4. 应用服务/Repository/API 依赖边界
5. Secret 脱敏
6. CORS、认证、租户和安全错误响应
7. Migration 基座
8. Outbox claim、重试和多 Worker
9. Tokio 阻塞 I/O 修复

P2
10. Worker 生命周期和优雅关闭
11. 真实集成测试与 Mock AI Server
12. CI、工具链和 Workspace lint
13. 首个端到端业务垂直切片
```

## 8. 下一阶段准入标准

在开始批量实现领域模块前，至少满足：

- [ ] `cargo fmt`、严格 Clippy、Workspace test 有 CI；
- [ ] `shared-kernel` 不依赖 Axum 和 SQLx；
- [ ] Handler 不直接持有 `PgPool`；
- [ ] Secret 日志安全；
- [ ] 认证和租户默认保护业务路由；
- [ ] Migration 可从空库执行；
- [ ] S3 Adapter 对真实 MinIO 通过契约测试；
- [ ] Local Adapter 路径安全；
- [ ] Outbox 多 Worker 设计已通过测试；
- [ ] 至少完成一个业务垂直切片证明分层可用。
