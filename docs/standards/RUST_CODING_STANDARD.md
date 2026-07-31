# Rust 代码编写规范

> 文档 ID：STD-RUST-001  
> 版本：1.0  
> 状态：Baseline  
> 生效日期：2026-07-30  
> 适用范围：整个 Rust Workspace

## 1. 目标

本规范约束 Rust 代码的可读性、正确性、安全性、异步行为、错误处理、测试和可维护性。架构依赖规则以 [`../architecture/CODE_ARCHITECTURE.md`](../architecture/CODE_ARCHITECTURE.md) 为准。

## 2. 工具链与门禁

仓库当前 MSRV 和构建工具链统一固定为 Rust 1.94.1。Rust 1.85.0
已在 2026-07-31 验证失败，原因是当前维护中的 `aws-sdk-s3`/Smithy
运行时要求 Rust 1.94.1；完整决策见
[`../adr/ADR-0004-rust-msrv-toolchain.md`](../adr/ADR-0004-rust-msrv-toolchain.md)。

仓库必须使用该精确工具链，并在 CI 执行：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo check --workspace --all-targets
```

建议在 Workspace 逐步启用：

```toml
[workspace.lints.rust]
unsafe_code = "forbid"
unused_must_use = "deny"

[workspace.lints.clippy]
all = "warn"
pedantic = "warn"
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
```

`pedantic` 中不适合项目的 lint 应集中说明后再调整，不在源码中大量散落 `allow`。

## 3. 格式与命名

- 使用 `rustfmt` 默认格式；
- crate、module、function、variable 使用 `snake_case`；
- type、trait、enum 使用 `UpperCamelCase`；
- constant 使用 `SCREAMING_SNAKE_CASE`；
- 缩写按普通单词处理，例如 `HttpClient`、`TenantId`；
- 布尔值使用明确前缀：`is_`、`has_`、`can_`、`should_`；
- 单位写入名称：`timeout_secs`、`size_bytes`、`latency_ms`；
- 避免 `data`、`info`、`manager`、`handler` 等无边界名称。

## 4. 文件与模块

- `lib.rs` 只声明模块、重导出稳定公开 API 和写 crate 文档；
- 单文件超过约 400 行时优先按职责拆分，但不机械拆分；
- 私有实现默认不 `pub`；
- 优先 `pub(crate)`，只有跨 crate API 才使用 `pub`；
- 不使用深层 `mod.rs` 树制造导航困难；
- 一个模块应有清晰单一职责。

## 5. 类型设计

### 5.1 强类型 ID

不要在领域接口中混用裸 `String`/`Uuid` 表示不同实体。

推荐：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContractId(Uuid);
```

要求：

- 构造和解析统一；
- `Display` 不输出敏感信息；
- 数据库和协议转换位于边界层；
- 不允许不同实体 ID 隐式互换。

### 5.2 Value Object

金额、日期区间、版本、状态和对象 key 应使用值对象表达不变量。

```rust
pub struct Version(u64);
pub struct ObjectKey(String);
pub struct Money { /* amount + currency */ }
```

构造时验证，避免非法状态在系统内部传播。

### 5.3 Enum 优于字符串状态

```rust
pub enum JobStatus {
    Pending,
    Running,
    RetryScheduled,
    Completed,
    Failed,
    Cancelled,
}
```

数据库/JSON 字符串转换集中管理。禁止在业务代码中比较魔法字符串。

## 6. 所有权与克隆

- 优先借用 `&T`、`&str`、`&[T]`；
- 只在需要所有权跨任务、持久化或缓存时 clone；
- 不用 `clone()` 暂时逃避借用设计问题；
- 大对象和文档内容不重复 clone；
- 共享不可变服务可使用 `Arc<T>`，但不得把所有对象默认包装为 `Arc<Mutex<_>>`；
- 领域对象默认不使用内部可变性。

## 7. 错误处理

### 7.1 错误分层

- Domain Error：业务规则和非法状态；
- Application Error：权限、冲突、幂等、用例失败；
- Infrastructure Error：数据库、网络、消息、对象存储；
- API Error：稳定错误码和协议响应。

下层错误转换为上层稳定分类时保留 `source`，但不向客户端泄漏底层细节。

### 7.2 `thiserror` 与 `anyhow`

- library crate 使用 `thiserror` 定义稳定错误；
- app 的启动、组合和顶层退出可以使用 `anyhow`；
- Domain/Application 公开接口不得返回 `anyhow::Error`；
- 不把错误仅转换成 `String` 后丢失来源和分类。

### 7.3 禁止 panic 路径

生产代码禁止：

```rust
unwrap()
expect()
panic!()
unreachable!()
todo!()
```

允许范围：

- 测试；
- 编译期能够证明的静态初始化，并带明确说明；
- 尚未实现的初始骨架可暂时存在 `TODO` 注释，但不可在运行路径使用 `todo!()`。

### 7.4 客户端错误

- 返回稳定错误码；
- 内部错误统一为安全消息；
- 通过 request/trace ID 支持排查；
- readiness 等管理接口也不得返回原始数据库错误。

## 8. 异步与并发

### 8.1 不阻塞 Tokio

异步函数中禁止直接执行：

- `std::fs` 大文件 I/O；
- 长时间 CPU 计算；
- 阻塞式 SDK；
- `std::thread::sleep`。

替代：

- `tokio::fs`；
- `spawn_blocking`；
- 独立 Worker；
- 异步 SDK。

当前 LocalStorage 实现使用 `std::fs`，后续实现必须修正。

### 8.2 锁

- 不跨 `.await` 持有同步锁；
- 优先消息传递、数据库原子操作和不可变数据；
- 使用锁时记录保护对象和锁顺序；
- 不使用全局大锁实现业务一致性；
- 分布式一致性不能依赖进程内 Mutex。

### 8.3 取消、超时和背压

所有外部调用和长任务必须定义：

- 总超时；
- 单次尝试超时；
- 取消语义；
- 最大并发；
- 队列容量；
- 重试上限。

禁止无界 `spawn`、无界 channel 和无限重试。

### 8.4 Task 生命周期

后台 task 必须由拥有者管理：

- 保存 JoinHandle 或加入 JoinSet；
- 优雅关闭时停止接收新任务；
- 等待正在执行的任务或明确取消；
- 记录 panic/退出结果；
- 不产生无法追踪的 detached task。

## 9. HTTP 代码

- Handler 输入使用专用 Request DTO；
- 返回专用 Response DTO；
- 不直接序列化数据库 Row；
- 请求大小必须有限制；
- CORS 默认拒绝，按环境配置白名单；
- 外部 ID、分页和排序字段必须校验；
- 查询接口必须设定最大 page size；
- Header、认证和租户由中间件提取为明确上下文。

Request DTO 与 Domain 类型分离，避免协议变化污染领域模型。

## 10. 数据库代码

- SQL 集中在 Repository/Infrastructure；
- 绑定参数，禁止字符串拼接用户输入；
- 重要查询显式列名，避免长期使用 `SELECT *`；
- 事务范围尽量短；
- 不在事务内调用慢外部 API；
- 更新正式数据使用版本条件；
- 分页必须有确定排序；
- migration 一经发布不得修改；
- 数据库错误分类为冲突、未找到、暂时不可用或内部错误。

Outbox 多 Worker 获取记录时必须使用 claim/lease 或锁定策略，不能只查询 `published = false`。

## 11. 对象存储与文件

- 使用成熟 S3 SDK，不手写 Signature V4；
- Access Key 和 Secret 必须实际参与签名；
- 预签名 URL 必须真实签名，禁止占位字符串；
- key 通过 `ObjectKey` 构造和验证；
- Local Adapter 必须 canonicalize/校验目标仍位于根目录；
- 禁止 `..`、绝对路径和平台前缀逃逸；
- 大文件采用流式传输；
- 不把完整文件默认存入 `Vec<u8>`；
- 日志只记录安全 ID、大小和摘要，不记录文档正文。

## 12. 配置与 Secret

- 配置结构按应用进程拆分；
- Secret 使用专用包装类型，`Debug` 输出必须脱敏；
- 不记录完整 `AppConfig`；
- 启动时验证 URL、端口、连接池、bucket 和超时范围；
- 开发默认值不得静默进入生产；
- 生产缺失必要配置时 fail fast。

当前 `StorageConfig` 和 `AuthConfig` 的 Secret 可通过派生 `Debug` 暴露，后续应采用脱敏类型。

## 13. 日志与可观测性

使用结构化字段：

```rust
tracing::info!(
    tenant_id = %tenant_id,
    contract_id = %contract_id,
    "contract submitted"
);
```

规则：

- 日志消息使用稳定事件描述；
- ID 放字段，不拼接长字符串；
- 错误使用 `error = %error` 或 `error = ?error`，按敏感性选择；
- 不记录密码、Token、Cookie、连接字符串和文件正文；
- 每个外部调用记录 provider、operation、latency、result；
- trace 跨 HTTP、消息和 Worker 传播。

## 14. 安全编码

- 外部输入全部校验；
- 默认拒绝未知枚举、未知字段和越权资源；
- 多租户查询必须显式带 tenant 条件；
- 文件名不作为对象 key 或本地路径；
- URL、Header 和错误正文不进入日志前必须脱敏；
- 禁止把供应商返回 HTML/错误正文直接返回客户端；
- AI、OCR、文档和 Agent Tool 结果视为不可信输入；
- 任何写操作必须在服务端重新检查权限和版本。

## 15. API 与事件兼容

- 外部 API 使用版本化路径或兼容演进策略；
- 字段新增优先保持向后兼容；
- 删除/重命名需要弃用周期；
- 事件包含 `event_id`、`event_type`、`schema_version`、`occurred_at`、`tenant_id` 和 trace；
- 消费者必须容忍重复；
- 不兼容事件变更必须新版本或新事件类型。

## 16. 测试规范

### 16.1 单元测试

- 使用 Arrange/Act/Assert；
- 测试名称描述行为和条件；
- 覆盖正常、边界、拒绝和冲突路径；
- 不依赖执行顺序和真实时间；
- 时间、ID 和 Provider 可注入。

### 16.2 集成测试

- PostgreSQL 行为使用真实 PostgreSQL；
- S3 行为使用 MinIO/S3 契约测试；
- 消息行为验证重复、重放和 Worker 崩溃；
- 测试之间隔离数据；
- 不共享长期远程测试库。

### 16.3 Mock

只 Mock 系统边界，不 Mock 被测对象内部实现细节。Mock AI 响应必须包括非法 JSON、429、500、超时和中断。

## 17. 文档与注释

- 公开 crate、trait、struct 和重要方法写 `///` 文档；
- 注释解释“为什么”和约束，不复述代码；
- 安全、幂等、重试、锁和事务语义必须记录；
- TODO 格式：

```rust
// TODO(PLAN-0001): replace stub S3 client with signed SDK adapter.
```

禁止无归属的长期 TODO。

## 18. 依赖管理

- Workspace 统一依赖版本；
- 新增依赖前确认维护状态、许可证、安全和必要性；
- 默认关闭不需要的 features；
- 外部 SDK 封装在 adapter 内；
- 不让 SDK 类型进入领域 API；
- 定期运行 `cargo audit`、许可证和未使用依赖检查；
- `Cargo.lock` 对应用 Workspace 必须提交。

## 19. Code Review 清单

- [ ] 依赖方向符合架构；
- [ ] 没有 Handler SQL 或 Domain 基础设施依赖；
- [ ] 没有未处理 `unwrap`/panic；
- [ ] 没有阻塞 Tokio；
- [ ] 外部调用有超时、并发和错误分类；
- [ ] Secret 和敏感数据未泄漏；
- [ ] 多租户和权限在服务端校验；
- [ ] 数据写入有事务、版本和幂等策略；
- [ ] 测试覆盖失败路径；
- [ ] 文档与公开协议同步；
- [ ] 格式、Clippy 和测试通过。

## 20. Runtime configuration

Configuration loading is permitted only in runtime support and application
composition roots. Connection URLs and credentials use redacted value types;
call `expose()` only while constructing an infrastructure client and never
store its plaintext result in application state or log it.
