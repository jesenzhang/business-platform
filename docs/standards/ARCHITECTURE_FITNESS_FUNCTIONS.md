# 架构适配性函数与自动门禁

> 文档 ID：STD-ARCH-FITNESS-001
> 版本：1.0
> 状态：Baseline
> 生效日期：2026-07-30
> 适用范围：CI、代码审查、依赖治理、契约测试与发布验收

## 1. 目的

架构规则必须尽可能转化为自动化检查。本文定义持续证明系统符合架构 Baseline 的 Fitness Functions。

Fitness Function 可以是：

- 静态依赖检查；
- 编译和 lint；
- 单元、契约、集成或 E2E 测试；
- Schema 兼容检查；
- 安全扫描；
- 性能和恢复演练；
- 文档和迁移一致性检查。

## 2. 门禁等级

### Required

失败时禁止合并。

### Conditional Required

触及对应范围时必须执行并通过。

### Evidence Required

无法在普通 CI 自动运行时，必须提供预生产或人工演练证据。

## 3. 基础 Rust 门禁

Required：

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

禁止：

- `continue-on-error` 隐藏失败；
- 无理由全局 `allow`；
- 删除或永久 ignore 失败测试；
- 只检查单个 crate 而遗漏 Workspace。

## 4. 依赖方向检查

`cargo run -p architecture-check -- check` 读取正式 `cargo metadata`，并由
`scripts/check-architecture.ps1` 组合源代码、迁移和文档边界检查。合法与非法
依赖图 fixtures 由 `cargo test -p architecture-check` 验证。

Required 规则：

- Domain 模块/crate 不依赖 Delivery、Infrastructure、`apps/*`；
- Application 不依赖具体 Adapter；
- 核心不依赖 Axum、SQLx、Reqwest、Broker、S3 或供应商 SDK；
- `apps/*` 不被核心 crate 反向依赖；
- 领域 A 不依赖领域 B 的 Infrastructure；
- `shared-kernel` 不依赖交付框架和基础设施 SDK；
- 不存在未批准的循环依赖。

具体禁止包列表是实现检查手段，根本目标是防止交付和基础设施实现向核心泄漏。

## 5. 源码边界检查

Required：

- Domain 中不得出现 SQL、HTTP Response、Broker Client、SDK Client；
- Handler 不直接使用数据库连接；
- Worker Entry 不直接实现业务状态转换；
- Adapter 不直接写入其他 Bounded Context 的私有表；
- 生产代码中不得存在 `X-Amz-Signature=TODO` 等可被误认为完成的 Stub；
- TODO 必须关联 Plan/Issue 或明确阶段。

源码扫描只作辅助，最终以依赖图和测试为准。

## 6. Bounded Context 和数据所有权检查

Conditional Required：

当新增表、Repository、业务命令或事件时：

- Migration 标明所属上下文；
- Repository 位于数据拥有者上下文；
- 其他上下文没有直接写入；
- 跨上下文引用使用稳定 ID/API/Event；
- 新增上下文或改变所有权存在 ADR；
- 计划和 PR 描述包含数据所有者。

可以通过 migration 命名、目录、manifest 或 lint metadata 自动辅助验证。

## 7. Domain 测试

Required：

- 复杂不变量使用纯单元测试；
- 不启动网络、数据库、容器或 Tokio Runtime，除非领域本身确实需要 async 抽象；
- 状态机覆盖允许和拒绝转换；
- Value Object 覆盖边界值；
- 领域事件和版本行为稳定。

## 8. Application 测试

Required：

使用 Fake/In-Memory Ports 验证：

- 权限和调用上下文；
- 用例编排；
- 事务意图；
- 幂等；
- 乐观锁冲突；
- 业务事件和审计意图；
- 不依赖具体 Adapter。

核心用例无法在 Fake Ports 下测试，视为边界设计警告。

## 9. Adapter 契约测试

Conditional Required：

### Persistence

真实目标数据库验证：

- Migration；
- SQL 和类型映射；
- 约束、索引和时区；
- 事务和回滚；
- 乐观锁；
- 租户隔离；
- 多 Worker claim/lease。

### Artifact Store

真实 S3 兼容服务验证：

- 私有认证；
- put/get/head/delete；
- stream；
- metadata/content-type；
- presign；
- 特殊 key；
- 错误映射。

### Messaging

真实 Broker 验证：

- 发布和消费；
- 重复；
- Ack/Nak；
- 重放；
- Dead Letter；
- context 传播。

### External Provider

Mock Server 验证所有错误分支；少量供应商兼容测试独立运行。

## 10. API 契约检查

Conditional Required：

- OpenAPI 生成；
- Schema 校验；
- 破坏性差异检测；
- 稳定错误码；
- 认证、租户和权限；
- 幂等和版本冲突；
- 分页；
- Body Limit、Timeout、CORS；
- 不泄漏底层错误。

## 11. 事件契约检查

Conditional Required：

- Event Envelope 完整；
- Schema version；
- 未知可选字段兼容；
- 重复和乱序；
- 历史消息重放；
- 大型或敏感内容不直接嵌入；
- Domain/Integration/Execution Event 类型明确。

## 12. 长时任务检查

Conditional Required：

- 任务状态持久化；
- 多 Worker 原子 claim；
- lease 过期恢复；
- fencing/claim token；
- 重试分类和退避；
- 取消；
- Worker 崩溃；
- 消息重复和丢失；
- 检查点恢复；
- 业务状态与执行状态分离；
- 正式业务写入通过所属上下文 Application。

## 13. 安全门禁

Required/Conditional Required：

- Secret 扫描；
- 依赖漏洞扫描；
- 许可证策略；
- 非 root 镜像；
- SBOM；
- 跨租户测试；
- 路径穿越；
- 未认证和越权；
- Production Config fail-closed；
- 高风险确认绑定版本和主体；
- Prompt Injection/Tool 白名单测试。

## 14. 数据迁移门禁

Conditional Required：

- Migration 文件不可修改历史；
- 空库执行成功；
- 重复执行安全；
- 上一版本升级成功；
- 滚动期间新旧版本兼容；
- 破坏性删除在兼容窗口后进行；
- readiness 验证 migration 兼容性。

## 15. 文档门禁

Required：

触及以下内容时必须更新对应文档：

- Bounded Context；
- 数据所有权；
- API/Event Schema；
- 事务、幂等和恢复；
- 安全边界；
- 部署单元；
- 基础设施选型；
- 质量属性目标；
- 运维和恢复流程。

计划必须包含架构符合性章节。PR 模板应列出文档同步清单。

## 16. 性能门禁

Evidence Required 或 Conditional Required：

- 普通查询和写入 P95/P99；
- 大文件流式内存；
- Worker 吞吐和积压；
- Provider 并发与限流；
- 数据库连接池和慢查询；
- 关键路径回归阈值。

性能下降超过批准阈值时阻止发布或记录接受风险。

## 17. 恢复门禁

Evidence Required：

Staging 定期证明：

- 数据库恢复；
- Artifact 恢复；
- 应用重新部署；
- Job 恢复；
- 一致性扫描；
- 备份可用性；
- 故障注入和告警。

演练记录进入 `docs/reviews/` 或 `docs/runbooks/` 关联证据。

## 18. 架构偏离

任何临时偏离必须记录：

- 偏离规则；
- 原因；
- 风险；
- 范围；
- 到期时间；
- 修复计划；
- 负责人；
- 验证方式。

长期偏离必须通过 ADR。禁止以 TODO 无限期保留架构违规。

## 19. CI 阶段建议

```text
architecture
→ fmt/check/clippy
→ unit
→ integration
→ contract
→ e2e
→ security/image
```

普通文档 PR 可跳过运行型集成测试，但必须执行文档链接、格式和架构元数据检查。

## 20. PLAN-0001 最低交付

- `rust-toolchain.toml` 和 Workspace lint；
- GitHub Actions；
- 初版 `check-architecture.ps1`；
- shared-kernel 去框架化检查；
- Handler 不访问 PgPool；
- Secret 测试；
- MinIO/S3 契约；
- Migration 测试；
- Outbox 并发恢复测试；
- document metadata 分层和租户 E2E。

## 21. 完成判定

```text
功能测试通过
但架构门禁失败
= 任务未完成
```

只有所有 Required 门禁通过、Conditional Required 有对应证据、无法自动化的 Evidence Required 有记录时，任务才可进入合并判断。

## 22. Runtime configuration checks

The architecture fitness script must reject process configuration and `config`
crate dependencies in `shared-kernel`, environment access in Document Domain
or Application, and `AppState` fields containing `AppConfig`,
`DatabaseConfig`, `SecretUrl`, or `PgPool`.

## 23. Persistence and query adapter gates

- Domain/Application cannot depend on SQLx, SQLite, PostgreSQL or an ORM.
- `document-postgres` and `document-sqlite` depend inward on `document`; shared
  contract support is dev-only.
- Query DTOs cannot derive `sqlx::FromRow`; SQL is prohibited in Application.
- Aggregate repositories cannot add dashboard/report/export query methods.
- `AppState` cannot contain a concrete PostgreSQL or SQLite pool.
- Production configuration rejects SQLite before infrastructure access.
- PostgreSQL and SQLite run common semantic persistence contracts; their
  database-specific concurrency and recovery contracts remain separate.
- `DocumentMetadata` fields are private; adapters use the validated
  `rehydrate` seam and no adapter may construct the aggregate directly.
- The Document Search port remains Deferred; no partial Search adapter may be
  exported. PostgreSQL uses the shared `runtime-migration` catalog while the
  SQLite adapter owns its local catalog.
- HTTP list cursors are opaque v1 tokens and storage-internal object locations
  are absent from response DTOs. Shared LIKE escaping and invalid-row
  fail-closed mapper tests run for both adapters.

## 24. PLAN-0004 durable processing gates

When the document-processing slice is present, Required evidence additionally
includes the metadata-driven core/adapter dependency check, fixed-pipeline
domain tests, SQLite `BEGIN IMMEDIATE` cross-adapter idempotency tests,
PostgreSQL `SKIP LOCKED` claim and stale-fence tests, candidate/review
optimistic concurrency, process restart recovery, and redacted processing API
contract tests. SQLite evidence must state single-process scope; it cannot be
used as evidence for PostgreSQL distributed concurrency.

Revision 1 adds fitness checks that worker binaries depend on
`ProcessingExecutionUnitOfWork` for Job/Step/AI Task/Candidate/Review writes,
that the fixed `current_step` pipeline is dispatched one step per claim, that
heartbeat tasks are owned and joined, and that migration 011/SQLite 002 are
manifested without historical edits. Contract tests cover atomic review
replay/rollback, AI retry/reclaim, stale fences, artifact metadata, and the
SQLite `BEGIN IMMEDIATE` single-writer boundary.

## 25. PLAN-0007 external access gates

PLAN-0007 additionally requires:

- `scripts/check-openapi.ps1` to parse `openapi.json`, assert the stable v1 paths
  and reject internal storage fields;
- `scripts/check-architecture.ps1` to reject database/object-storage dependencies
  from `business-cli`, `agent-adapter` and `public-api-contracts`, and database
  references from `business-console`;
- CLI parser/unit contracts and MCP allow-list, malformed-argument, auth and
  upstream-unavailable contracts;
- multipart upload idempotency, content-type, tenant isolation and object/database
  compensation tests;
- frontend lint, strict typecheck, unit, build and Playwright smoke checks;
- the local Demo compose topology and seed script to remain outside the production
  configuration path. Docker/PostgreSQL/MinIO execution is evidence-required when
  the local host does not provide those runtimes and must be reported as `NOT RUN`.

## 26. Business Module Isolation 与 Semantic Contract gates

PLAN-0010 additionally requires：

- `business-module-contracts` and `semantic-contract` remain pure Rust and do not
  depend on WrenAI, Python, LanceDB, DataFusion, SQLGlot, ClickHouse, SQLx,
  Axum, Reqwest, Messaging, object storage, AI providers, or concrete business
  modules;
- manifests contain the generic module contract fields and do not hard-code
  Contract, Finance, C-specific, Wren-specific, table, schema, SQL, credential,
  or provider DTO names in the platform compiler;
- semantic IDs are namespaced, compiled output is deterministic, and tests cover
  duplicate IDs, metric ownership conflicts, incompatible versions, unknown
  dependencies/endpoints, cycles, illegal platform dependencies, undeclared
  contributions, and cross-module private references;
- source scans reject C-specific names under Platform Core/generic crates except
  the explicit PLAN-0009 rehearsal, migration-report, reference and
  `integrations/legacy-c-contract-management` ACL exception paths;
- no public/API/Agent/MCP/logging path introduces arbitrary SQL, schema
  browsing, database URLs, credentials, storage keys or internal object paths;
- the C project remains read-only rehearsal evidence, PLAN-0006 remains Proposed
  / NOT ACTIVE, and PLAN-0009 remains Completed / Rehearsal Closed.

PLAN-0011 additionally makes the packaging foundation executable as a fitness
function. `scripts/check-architecture.ps1` scans production source in
`business-module-contracts`, `business-application-compiler`, and
`semantic-contract`, rejecting persistence/web/provider/runtime dependencies,
concrete business or synthetic fixture names, automatic business data purge
operation tokens, and duplicate semantic-authority type declarations. Inline
`#[cfg(test)]` sections and test/fixture paths are excluded from the production
scan; fixture names are permitted only in tests. The compiler remains responsible
for package/contribution/extension compilation and dry-plan evidence;
`semantic-contract` remains the sole semantic compiler/authority.

## 27. Architecture Foundation Convergence gates

Architecture Foundation Convergence 文档阶段新增以下 Required 设计门禁；PLAN-0011 激活前必须把它们转为 pure contract/compiler 或 fixture contract tests：

- Platform Core 不包含具体业务 module、fixture、表、schema、SQL 或 provider 名称；
- Business Module A/B 不得依赖对方 private Repository、Domain internals、private persistence、FK 或 SQL；
- 跨模块 Query/Command 有 timeout、tenant、authorization、version、idempotency 和稳定 error mapping；
- Integration Event 具有 owner→local transaction→Outbox→versioned event 链，消费者支持 duplicate、out-of-order、retry、replay 和 evolution；
- ResourceRef 不暴露 private storage identity，使用时重新执行 tenant/auth/lifecycle/version 校验；
- Reference + Snapshot 明确 immutable historical semantics，不能取代 Owner 当前状态；
- Published Projection 记录 owner、source/version、freshness、watermark、rebuildability 和 non-authority；
- Process Manager/Saga 是唯一跨模块业务流程模型，且与 Durable Task Job/Step/Lease/Fence/Retry/Recovery 状态分离；
- Published Extension Point 的 owner、consumer、stable ID、schema/version、classification、authorization、dependency 和 removal semantics 可验证；live consumer 存在时 removal 必须 Blocked；
- compiled package manifest 和 dry-plan 对输入顺序保持稳定排序、canonical bytes、SHA-256 digest 和诊断顺序；
- required platform capability 必须 against explicit evidence/catalog 解析并 fail closed；UI/Agent target 类型和 classification 必须由 typed contract 闭合校验；规划冲突必须输出结构化 `Conflict`；
- Module removal 不能暗示或自动执行 business data purge；
- UI、Agent、Semantic contribution 共享 module identity 但有独立 schema/authorization，Agent Tool 只能调用 Application API。
