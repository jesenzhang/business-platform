# PLAN-0007：Business Console and External Access Demo

文档 ID：PLAN-0007  
版本：0.1  
状态：Active / local solo fast-forward  
日期：2026-08-07  
责任边界：Document Management、Document Intelligence、Runtime Governance 的外部访问面  
前置计划：PLAN-0001～PLAN-0005 已 Integrated；PLAN-0006 保持 Proposed / NOT ACTIVE

## 目标

交付一个可运行的企业文档智能处理与运行治理控制台 0.1，并证明同一
Business API/Application 能力可以被 React、REST 客户端、CLI 和只读 MCP
Adapter 复用。前端是可替换的静态客户端；它不拥有业务规则、身份事实或持久化。

## 非目标

- 不实现 Workspace、Conversation、Thread、Skill Registry、Capability Grant、Generated App 或完整 Agent Runtime；
- 不启动或归档 PLAN-0006；
- 不增加任意 SQL、动态指标查询、通用 DAG、调度器、OCR 平台或 Model Gateway；
- MCP 只开放固定 read-only allow-list；Repair 页面不绕过 Prepare → Preview → Confirm → Execute。

## 架构预检

| 项 | 决定 |
|---|---|
| Bounded Context | Document Management 拥有内容元数据/版本；Document Intelligence 拥有 ProcessingJob/Candidate/Review；Runtime Governance 拥有 Audit/Finding/Repair。 |
| 统一语言 | Document、Content Revision、Processing Job、Candidate、Review、Finding、Repair、Audit Event。 |
| 数据所有者 | UI/CLI/MCP 均不拥有正式业务事实；Business API 通过 Application ports 调用 owner context。 |
| 状态归属 | 业务状态在 owner context；durable lease/checkpoint/retry 在 Document Intelligence 执行边界；UI 只读投影。 |
| 公开能力 | Document list/get/upload、Processing list/start/get/candidate/review、fixed Operations Overview、Finding/Audit read。 |
| 访问协议 | REST `/api/v1` 是权威外部协议；CLI 与 MCP 使用 `business-api-client`；React 使用相同的公共 DTO 语义。 |
| 一致性 | 写命令带 Idempotency-Key；Processing 使用版本/租约；Upload 在对象成功而数据库失败时删除对象补偿；查询 tenant-scoped、cursor opaque。 |
| 安全 | Bearer 身份由 Business API/MCP Adapter 建立；tenant 不能由请求参数或模型指定；敏感存储字段、raw text、prompt、credential 和 provider 原始错误不进入 DTO。 |

## 交付切片

1. `public-api-contracts`、`business-api-client`、版本化 `openapi.json`。
2. Business API upload、processing list、operations overview 和公共 DTO 映射。
3. 独立 `apps/business-console`：Dashboard、Documents、Document Detail、Processing、Candidate Review、Integrity、Repairs、Audit。
4. `business-cli`：status、documents、processing、candidate、findings、audit。
5. `agent-adapter` HTTP MCP protocol `2026-07-28`，固定 read-only tools。
6. Demo compose、确定性 seed/provider 边界、MCP client 配置和一键脚本。

## 质量属性与可验证验收

- 性能：列表默认上限 20/50/100，cursor 查询使用 owner adapter keyset；Overview 是固定 bounded DTO，不能提交用户查询。
- 可用性：React、CLI、MCP 停止不影响 Business API/Workers；MCP 上游不可用返回稳定 `upstream unavailable`。
- 恢复：上传对象/数据库失败执行补偿；Processing 继续复用既有 lease/fence/retry/recovery；写命令可安全重试。
- 多租户：所有业务读取从可信 `TenantContext` 过滤；跨租户 ID 表现为 not-found/denied；MCP 不接受 tenant 参数。
- 可维护性：契约 crate 无 Axum/SQLx；CLI/MCP 无 DB、Repository、object-storage 依赖；React 只依赖 REST base URL。
- 可观测性：请求 ID、trace、Audit transition 和稳定错误码贯穿 API/Client；公共输出不泄漏内部凭据。
- 兼容性：OpenAPI、契约 round-trip、CLI JSON、MCP tools/list 和错误映射作为自动测试门禁。

## Fitness Functions

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `scripts/check-architecture.ps1`
- `public-api-contracts` 无 Axum/SQLx；CLI/MCP 无 SQLx/PgPool/Repository/object storage；console 无数据库依赖
- console `npm run lint`, `npm run typecheck`, `npm test`, `npm run build`, Playwright smoke
- OpenAPI JSON parse and sensitive-field regression tests

## 文档、部署与回滚

本计划同步 `openapi.json`、MCP example、Demo runbook、架构状态和迁移 manifest（仅新文件）。
Demo 是可删除的静态客户端/适配器部署单元，不改变核心 Bounded Context。回滚顺序为停止
console/MCP/CLI，撤回 Business API 外部路由，保留现有 Document/Processing/Governance
内部能力；不回滚既有迁移或删除正式业务事实。

## 完成定义

React 可以通过真实 API 完成上传 → 创建 Processing → 查看候选 → Review；CLI 可以通过
远程 API 输出稳定 JSON/table；MCP 可以 discover 并调用固定 read-only tools；三者共享
tenant、权限和业务事实；Rust/前端/架构/契约门禁均有明确 PASS 或带原因的 NOT RUN。
在达到 Candidate 前，不合并 main、不归档 PLAN-0007、不启动 PLAN-0006。
