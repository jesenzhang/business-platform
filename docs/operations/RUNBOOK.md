# 运维 Runbook（v0.1 初版）

> 文档类型：Operations Runbook
> 状态：Draft（PLAN-0012 T4.5）
> 最后更新：2026-09-02
> 适用范围：business-api、business-worker、ai-worker、governance-worker、agent-adapter、business-console、PostgreSQL、对象存储（S3/MinIO）

## 1. 部署拓扑与进程

| 进程 | 作用 | 关键配置 |
|---|---|---|
| `business-api` | 公开 REST（`/api/v1`）、健康检查（`/health/live`、`/health/ready`） | `config/{env}.toml`，`BUSINESS_API__*` 环境变量覆盖 |
| `business-worker` | 一体式文档处理（抽取 + AI） | `BUSINESS_WORKER__*` |
| `ai-worker` | 独立 AI 步骤 worker（PostgreSQL only） | `AI_WORKER__*`，`ai_provider.mode = deterministic\|real` |
| `governance-worker` | 完整性扫描与受控修复执行 | `GOVERNANCE_WORKER__*` |
| `agent-adapter` | 只读 MCP HTTP 适配器（无 DB 依赖） | `AGENT_ADAPTER__*` |
| `business-console` | React 控制台（静态产物） | `apps/business-console` |

数据库迁移统一走 `cargo run -p migration -- --backend postgres up`；SQLite 仅限本地单进程。

## 2. 启动与就绪判定

1. PostgreSQL 与对象存储先于应用启动；
2. `business-api` 启动即执行 fail-fast 配置校验，配置错误立即退出（exit 1），不允许带错误配置运行；
3. 就绪判定：`GET /health/ready` 返回 200 才可挂流量；`/health/live` 只反映进程活性；
4. 认证要求（PLAN-0012 M3 + 发布收口）：
   - 生产必须配置 HTTPS 的 `auth.issuer_url`；`auth.audience` 生产必填（缺失或纯空白直接启动失败），防止同 issuer 其他应用的 token 被接受；
   - `auth.jwks_url` 可选：不配置则走 OIDC discovery；一旦配置必须为 HTTPS 且不得为空白/纯空白（空白值会抑制 discovery 并使每次 JWKS 拉取失败，配置校验在启动时 fail-closed 拒绝）；
   - IdP 不可达时 API 拒绝业务请求但 `/health/*` 不受影响——这是 fail-closed 预期行为，不是故障。

## 3. 升级流程

1. 发布前置检查：CI（fmt/check/clippy/test/architecture/security 六类 job）全绿；
2. 备份：执行 `deploy/operations/drill-backup-restore.sh` 或至少 `pg_dump --format=custom`（见 §5）；
3. 滚动顺序：先停 worker（graceful shutdown 会释放 lease）→ 迁移数据库 → 部署 business-api → 启动 worker；
4. 迁移只前进（`up`）；迁移 status 命令用于确认版本一致性。

## 4. 回滚

- 应用回滚：回退镜像/二进制至上一版本；不回滚已执行的数据库迁移（迁移 forward-only）；
- 认证回退：仅限开发环境，`auth.dev_auth_enabled = true`；生产禁止（配置校验拒绝）；
- AI 提取回退：`ai_provider.mode = deterministic` 立即脱离外部模型依赖，无需重新部署；
- 供应商密钥轮换：更新 `ai_provider.api_key` 后重启 ai-worker；密钥通过 `runtime-config` secret 机制承载，不落日志。

## 5. 备份与恢复

- 脚本：`deploy/operations/drill-backup-restore.sh`（备份 → 破坏 → 恢复 → 校验一体化演练）；
- 数据库：`pg_dump --format=custom`，恢复用 `pg_restore --no-owner`；
- 对象存储：`mc mirror` 全量镜像；
- 频率基线：每日逻辑备份 + 每次升级前强制备份；
- **未演练恢复的备份不视为有效**（DEPLOYMENT_ARCHITECTURE §16）；演练结果记录到 `docs/reports/`。

## 6. 故障处置

| 症状 | 初判 | 处置 |
|---|---|---|
| `/health/ready` 503 | DB 不可达或迁移缺失 | 检查数据库连接与 `migration status` |
| 大量 401 | IdP/JWKS 不可达或密钥轮换 | 确认 issuer 可达；JWKS 缓存 10 分钟内自动恢复，轮换后观察 `kid` 命中 |
| 处理任务停滞 | worker 崩溃或 lease 未释放 | worker 停机即停 claim；lease 过期后由存活 worker 自动 reclaim，无需手工解锁 |
| AI 提取持续失败（429/5xx） | provider 过载 | 失败自动按 retry 分类重试/死信；持续过载时切 `deterministic` 模式保流程 |
| 修复执行卡住 | 审批门控或 lease 竞争 | 修复必须经 dry-run → approve → execute；检查 governance-worker 日志与 repair ledger 状态 |

日志与追踪：`observability.log_format = json`（生产强制，配置校验 fail-closed）输出单行 JSON；请求 ID 由 API 中间件贯穿并作为 `correlation_id` 进入 ProcessingJob/AI Task/审计/worker 日志；可选 `otlp_endpoint` 导出 trace。指标：`business-api` 在公共端口 `GET /metrics`（无认证，供 Prometheus scrape），含 `http_requests_total{method,status}`、`http_request_duration_seconds{method}`、`auth_failures_total{reason}`；`business-worker`/`ai-worker` 在 `observability.metrics_addr`（生产必填）暴露内部 `GET /metrics`：排队等待、吞吐（bounded outcome）、lease 丢失/回收、重试 disposition、AI 时延、429/5xx。抓取配置与最小 dashboard：`deploy/observability/`。

## 7. 安全基线

- 所有进程配置校验 fail-fast；生产禁止 dev 认证、本地存储、`*` CORS；
- 密钥/URL 经 `runtime-config` Secret/SecretUrl 承载，渲染时自动脱敏；
- 安全扫描（cargo-audit/gitleaks/trivy）在 CI `security` job 强制；trivy 与演练用 MinIO `mc` 均为固定版本 + 显式维护的 SHA-256 校验，校验失败在下载后、执行前立即中止（维护策略见 `.github/workflows/ci.yml` 注释）；
- 生产 AI provider endpoint 默认只允许 HTTPS 或 loopback；内网明文 HTTP 需显式 `ai_provider.allow_private_http = true` 并承担传输风险。

## 8. 已知缺口（v0.1）

- WAL/PITR 与自动备份调度尚未建立（当前为手动/演练脚本）；
- 备份/恢复演练已在 CI `integration` job 的真实 PostgreSQL/MinIO service containers 上执行（pinned `mc` + checksum 校验，2026-09-02 起）；真实预生产栈上的演练与 Prometheus/Grafana 抓取、dashboard、标签基数验证待预生产首跑（配置在 `deploy/observability/`）；
- 预生产验收（20 并发性能 smoke、真实 IdP/model-provider 全链路、v0.1 tag）见 `docs/reports/PLAN-0012-COMPLETION-AUDIT.md`：因当前环境无 staging/真实凭据，标记 BLOCKED/NOT RUN；
- IdP demo compose 与 console 登录流程（T3.3）后置；
- 性能/容量基线（M5 T5.1）尚未建立。
