# PLAN-0010：Business Module Isolation 与 Semantic Contract Foundation

> 状态：Candidate 重建中（独立分支；不自动集成 main）
> Base：`origin/main` @ `654fe83d82107d899079d20e5fef8aaf4d5431b8`（真实 GitHub main）
> Candidate implementation：待 clean implementation commit
> Branch：`codex/plan-0010-clean-candidate`
> 目标：正式登记 Canner/WrenAI 参考输入，并建立平台核心与业务模块隔离、语义贡献和确定性编译的最小纯 Rust 基础

## 1. 目标 Bounded Context 与数据所有者

本计划不新增 Bounded Context，也不改变现有 Context Map。新增 seam 属于平台基础能力：

| 能力 | 归属 | 数据所有者 |
|---|---|---|
| Business Module Manifest contracts | Platform Contracts | Platform Foundation 只拥有 manifest schema/校验语义，不拥有业务事实 |
| Semantic Contract compiler/registry seam | Analytics / Visualization platform capability | Analytics 只拥有 compiled registry input、摘要和未来可重建投影；正式指标事实/业务状态仍由 Business Module 拥有 |
| C legacy boundary | Migration ACL（未来） | C 外部系统在受控迁移前仍是外部权威；本计划不写入 |

业务模块仍按既有 Bounded Context Map 负责 Contract、Finance、Document Management、Document Intelligence、Customer、Project、Approval 等正式状态。新 manifest 不替代 Domain/Application/Infrastructure 分层。

## 2. 架构影响

本计划落实 [`ADR-0020`](../../adr/ADR-0020-business-module-isolation-and-semantic-contract.md) 和专题 Baseline [`BUSINESS_MODULE_ISOLATION_AND_SEMANTIC_CONTRACT_ARCHITECTURE.md`](../../architecture/BUSINESS_MODULE_ISOLATION_AND_SEMANTIC_CONTRACT_ARCHITECTURE.md)。

### 2.1 Concept Inventory

正式术语：`Business Module`、`Business Module Manifest`、`Module Lifecycle`、`Semantic Contribution`、`Dataset`、`Projection`、`Field`、`Relationship`、`Measure`、`Metric`、`Metric Version`、`Dimension`、`Time Dimension`、`Filter Policy`、`Lineage`、`Semantic Reference`、`Compiled Semantic Manifest`、`Platform Capability`、`Resource Kind`。

复用 ADR-0017 的语义定义；不引入 Wren-specific 术语或第二语义权威。`Module Lifecycle` 不与业务状态、Durable Task Execution 状态或数据保留状态混用。

### 2.2 Gap Analysis

- 当前没有 `modules/`、`integrations/` 或 Analytics runtime；本轮不创建运行时目录；
- `crates/customer`、`contract`、`project`、`finance`、`document*` 继续作为 transitional business crates；不 mass move；
- `apps/plan-0009-rehearsal` 与 `crates/legacy-migration-rehearsal` 保持 test/rehearsal 例外；C-specific 名称不得进入通用 crate；
- 新增 `business-module-contracts` 与 `semantic-contract` 两个 crate，均纯 Rust、无数据库/网络/AI/业务 module 依赖；
- 通过 `architecture-check`、PowerShell source scan 和 Rust tests 固化依赖、语义、确定性和 C boundary 规则。

## 3. 范围

### 3.1 本轮交付

1. 登记 Canner/WrenAI 固定提交、路径许可证、MDL/Context/Compiler/MCP 事实及 Adopt/Adapt/Reject/Defer 矩阵；
2. 接受 ADR-0020，并同步后端 manifest、代码架构、企业业务领域、Analytics、Legacy Migration、Architecture Status、AGENTS 和 Fitness Functions；
3. `business-module-contracts`：Manifest、平台能力需求、公开契约、资源类型、数据分类、迁移命名空间、semantic descriptor、依赖、兼容性和生命周期类型；
4. `semantic-contract`：Dataset/Projection/Field/Relationship/Measure/Metric/Dimension/Time Dimension/Filter Policy/Lineage 类型和纯 Rust compiler；
5. 编译器冲突/拒绝测试：重复、所有权、版本、依赖、循环、关系端点、跨模块 private 引用、非法平台依赖和未声明 contribution；
6. 确定性编译输出：稳定命名空间、排序、canonical JSON 和 SHA-256 摘要；
7. 架构门禁：新 crate 不依赖 Wren/Python/DB/Provider/具体业务模块；Platform Core 不出现 C-specific 名称；Agent/API 不新增 SQL/Schema/凭证边界破坏。

### 3.2 明确不做

- 不引入 WrenAI runtime、Python、LanceDB、DataFusion、SQLGlot、ClickHouse、OLAP、Text-to-SQL、通用 Query Engine 或 Model Gateway；
- 不实现真实 Module Registry、安装/启用 API、热加载/热卸载、插件系统、Workflow Designer 或独立微服务；
- 不实现 Analytics Query Service、Projection/Metric runtime、Dashboard/Report、Agent Analytics Skill 或 MCP schema/SQL tool；
- 不创建数据库 migration、业务 API、事件、Worker、部署单元或 C ACL 代码；
- 不激活 PLAN-0006，不重新打开 PLAN-0009，不执行 C Project 生产迁移或写入。

## 4. 公开契约与一致性

本轮没有 HTTP API、集成事件或数据库写命令。两个 crate 的 Rust 类型是内部平台契约，不向 Public API、Agent、MCP 或日志直接暴露内部字段。

未来若实现 Registry/API，必须遵循：命令版本化、租户/主体/幂等、manifest optimistic version、Prepare → Preview → Confirm → Execute（涉及高风险卸载/清除）、Outbox/审计、可重建 compiled artifact 和公开 DTO 不含 SQL/Schema/凭证。跨模块只通过 published application API、事件、ResourceRef、Public Projection 或 Reference + Snapshot。

## 5. 编译不变量

```text
module_id/version 合法且唯一
required capability/module version 可满足
platform capability 不伪装成 module dependency
module dependency 无环
semantic id = <module-id>.<semantic-id>
descriptor 与 contribution 一致
semantic id / metric ownership 不冲突
所有 semantic reference/relationship endpoint 可解析
跨模块引用不是 private
canonical JSON 与 digest 对输入顺序不敏感
```

编译器拒绝未知字段/非法值；不通过隐式 fallback、数据库探测、任意 SQL 或 Agent 推理修复。

## 6. 安全与租户

- Manifest/semantic type 允许声明 `Public/Internal/Confidential/Restricted` 分类；细粒度业务授权仍由 owner Context 和 Analytics Policy 执行；
- 语义定义不承载 raw text、storage key、signed URL、数据库 URL、credential、prompt 或 provider response；
- Agent 只获得未来的 typed semantic query capability，不能直接读取 manifest private fields 或执行 compiler side effect；
- 跨模块引用必须经过公开语义对象/ResourceRef/Projection/Snapshot；不能通过私有表 FK/JOIN 绕过授权；
- C source 只读、ACL 限界；本计划不创建外部连接或复制生产数据。

## 7. 质量属性

| 属性 | 验收证据 |
|---|---|
| 性能/容量 | 编译器为纯内存、确定性排序和摘要；运行时查询 P95/P99 不在本计划承诺范围 |
| 可用性/隔离 | 新 crate 没有外部运行时依赖；编译失败 fail closed，不影响现有 apps |
| 幂等/恢复 | canonical output 可重复生成；无持久化副作用；未来 Registry 必须可重建 |
| 安全/多租户 | 分类、owner、公开引用和禁止 SQL/Schema/credential Fitness Functions；真实租户策略留在 Query Service |
| 可维护性 | 两个小而深的 seam：module contract 与 semantic compiler；不把业务名硬编码进 generic compiler |
| 可替换性 | WrenAI 只作为参考；未来 Wren adapter 必须在边界外，不得进入核心类型 |
| 可观测性 | 本轮只输出稳定错误/摘要测试；不输出语义正文、数据库或凭证 |
| 契约兼容 | manifest schema、module version、semantic version 和 descriptor 都显式建模并测试拒绝不兼容版本 |

## 8. 实施步骤

1. 文档/ADR/参考登记与本计划（本步骤）；
2. 加入两个 crate 到 workspace，定义纯 contract 和 metadata；
3. 实现 semantic compiler、冲突错误和 canonical digest；
4. 加入单元测试、架构元数据验证和源代码 Fitness Functions；
5. 运行全量 gates，记录 PASS/NOT RUN，并审查 diff 中是否误引入 Wren/SQL/C 名称；
6. 输出分支候选报告；不自动 commit、push、PR 或 main merge。

## 9. 回滚与完成定义

回滚只删除本计划新增的 docs、ADR、两个 crate、workspace entries 和 fitness checks；没有 migration、event、API 或外部副作用。完成定义：

- [x] 文档、ADR、reference、plan、manifest/architecture index 全部登记；
- [x] `cargo fmt --all -- --check` 通过；
- [x] `cargo check --workspace --all-targets --all-features` 通过；
- [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings` 通过；
- [x] `cargo test --workspace --all-features` 通过；
- [x] `pwsh ./scripts/check-architecture.ps1`、`pwsh ./scripts/check-openapi.ps1`、`git diff --check` 通过；
- [x] Wren/Python/DB/SQL/C-specific Platform Core scans PASS；
- [x] 本地 `cargo test --workspace --all-features -- --ignored` 已执行；因缺少 `DATABASE_URL`，结果为 `NOT RUN / environment unavailable`；本计划无 persistence/runtime storage change，真实 PostgreSQL/MinIO 证据由 workspace CI 提供；
- [ ] 独立分支 Candidate review 与精确 Candidate SHA 的 GitHub CI 待 clean branch 完成后记录；不自动进入 main；
- [x] PLAN-0006 仍为 Proposed/NOT ACTIVE，PLAN-0009 仍为 Proposed/NOT ACTIVE。

## 10. Accepted Candidate 记录

Candidate implementation SHA、完整门禁、模块删除验证、Platform Core 隔离验证、C-specific scan 和远程 CI 将记录在 [`docs/reviews/2026-08-11-plan-0010-accepted-candidate.md`](../../reviews/2026-08-11-plan-0010-accepted-candidate.md)。当前仍处于 clean candidate reconstruction；完成精确 Base..HEAD 审查和最终 GitHub CI 后，才标记为 `Accepted Candidate`。不 archive、不 merge main、不启动 PLAN-0006/PLAN-0009。
