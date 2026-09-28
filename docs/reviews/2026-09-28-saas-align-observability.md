# PLAN-0014 Wave 4 — Observability reference conformance review

> Wave: PLAN-0014 Wave 4 · Review date: 2026-09-28 (actual date)
> Branch `codex/plan-0014-wave4-reference-conformance` · Base `a4c2f74b…` · Code commit `6fb9487d…`
> Evidence: `2026-09-28-wave4-gates/`（`01`–`06`），能力矩阵与 provider 事实见 `2026-09-28-saas-align-document-storage.md` §4
> Maturity claimed: **R1 (foundation conformance)**. Verdict left to the independent reviewer.
> Baseline and fixed references: `2026-09-28-wave4-baseline.md` §2.2 / §3.

## 1. Scope

Review question: does the observability substrate (tracing initialisation, log
level/format, provider noise, metrics, request→job→provider correlation, log
redaction, telemetry-vs-audit separation) conform to
`docs/architecture/OBSERVABILITY_ARCHITECTURE.md`, SECURITY_ARCHITECTURE and
AGENTS §10/§11, measured against the OpenTelemetry specification v1.61.0 and the
Collector v0.161.0 (baseline §3)?

Not in scope: a tracing backend deployment, sampling infrastructure, alerting or
SLO tooling; no ADR-0028 assumption (it remains `Proposed` / NOT ACCEPTED).

## 3. Current implementation facts

**Process coverage is uneven, and one process had no telemetry at all.**
`business-api`, `business-worker` and `ai-worker` call
`observability::init_tracing`; `governance-worker` did not — every `tracing::*`
on the governance scan/repair path was silently dropped. This wave fixed that
(`apps/governance-worker/src/main.rs` `init_observability`, log level
`GOVERNANCE_LOG_LEVEL`, format `GOVERNANCE_LOG_FORMAT` parsed fail-closed,
optional `GOVERNANCE_OTLP_ENDPOINT`).

**Provider noise.** `SUPPRESSED_TARGETS` (`crates/observability/src/lib.rs:69-77`)
pins `aws_sdk_s3`, `aws_smithy_runtime`, `aws_sigv4`, `aws_runtime`, `hyper`,
`h2`, `rustls`, `reqwest` to `warn`, and `default_filter_directives` appends
`<target>=warn` per entry; `RUST_LOG` still overrides. This is a
level-pinning control, not a sampler: OTel's sampling and exporter isolation are
explicitly **not** implemented here.

**Metrics.** `apps/business-api/src/metrics.rs` used to build its own
`PrometheusBuilder`; it now delegates to `observability::metrics`, and the new
test asserts durations render as `authorization_duration_seconds_bucket{le=…}`
and that no `{quantile=` (client-side summary) appears. Label sets are bounded:
`business-worker` carries a test that pins the outcome/disposition label
cardinality. No tenant id, user id or document id is used as a metric label
anywhere in the metric paths this wave touched — verified by reading
`crates/observability/src/metrics.rs` and the two `metrics.rs` modules; the only
labelled series are route/outcome/disposition/step enumerations.

**Correlation.** There is no server-owned correlation identity. Three routes echo
the client header verbatim:
`apps/business-api/src/routes/{documents.rs:166-174, processing.rs:68, upload.rs:31}`
read `x-request-id` and `trace_error` puts it into the public `trace_id`
(`apps/business-api/src/api_error.rs:67`); every other route returns
`trace_id: None`. `processing.rs:289-293` writes the same value (or a fresh
UUIDv7 when absent) into `document_processing_jobs.correlation_id
VARCHAR(64)` (`migrations/018_processing_correlation_propagation.sql:5,8`), while
`audit_events.correlation_id` is `UUID` on PostgreSQL
(`migrations/012_runtime_audit_integrity_repair.sql:7`) and `TEXT` on SQLite
(`crates/document-sqlite/migrations/003_unified_audit_columns.sql:5`). Workers
restore correlation on the async path
(`apps/business-worker/src/main.rs:287,409`, `apps/ai-worker/src/main.rs:360,454,473`)
but nothing propagates a trace identity across the process boundary: no
`traceparent` extraction/injection exists anywhere in the workspace.

**Redaction.** The audit side has it (`redact_details`,
`sanitize_details_for_read`); the log path has no equivalent — redaction today is
"do not log it in the first place", which this wave enforced for storage
references (ST-W4-02) but is not a general control.

**Telemetry vs audit.** The separation holds: audit events are durable,
per-tenant hash-chained facts (Wave 3); tracing spans are best-effort. But
document lifecycle audit rows carry no correlation/trace value, so a request and
its formal audit row cannot be joined except by time and ids.

## 4. Capability matrix

| Capability | 核心声明? | 适配器实现? | 真实环境验证? | 说明 |
|---|---|---|---|---|
| structured logs with service name + level + format | yes | `observability::init_tracing` | not against a real collector | text/json both implemented, format parse fail-closed |
| uniform coverage of every long-running process | n/a | 4 of 5 processes after this wave | no | governance-worker fixed this wave |
| provider noise control | yes (`default_filter_directives`) | EnvFilter directives | yes (unit test with a recording layer) | level pinning, not sampling |
| metrics: counters + bucketed histograms | yes | `observability::metrics` | no collector | Prometheus text format asserted |
| bounded label cardinality | partially (convention) | convention + tests | no | no tenant/user/document ids as labels in touched paths |
| request↔job↔provider correlation | no contract | ad-hoc `x-request-id` echo | no | OBS-W4-05 |
| W3C trace context propagation | no | none | no | depends on ADR-0028 for the async envelope |
| log redaction as a control | no (audit only) | audit side only | audit lanes (Wave 3) | OBS-W4-07 |
| exporter failure isolation | yes (`OTLP` branch) | exporter install at startup | **BLOCKED** (no collector) | OBS-W4-06 |
| sampling | no | no | no | explicitly out of wave scope |

## 5. Findings

### C1 R2 REQUIRED — repaired

**OBS-W4-01 · C1 · 已修复**
* 代码位置：`apps/governance-worker/src/main.rs`、`apps/governance-worker/Cargo.toml`
* 违反的契约：OBSERVABILITY_ARCHITECTURE「每个长时运行进程必须可观测」；AGENTS §11 可观测性
* 复现方式：静态证据（该 bin 不引用 `observability`，无任何 subscriber 安装点）——`tracing::info!` 在无 subscriber 时是 no-op，因此扫描/修复路径完全没有日志
* 影响：治理扫描与批准修复（含不可逆操作）在事故复盘中不可见；fail-closed 配置错误也只体现为退出码
* 修复范围：新增 `init_observability()`（`GOVERNANCE_LOG_LEVEL`/`GOVERNANCE_LOG_FORMAT`/`GOVERNANCE_OTLP_ENDPOINT`），在读取任何配置之前安装；格式非法直接失败而非静默降级
* 测试：`cargo clippy/check` + `cargo test --workspace`（bin 编译与现有 governance lanes 不变）；无新单测（初始化本身是组合根职责）
* 证据：`2026-09-28-wave4-gates/01b-check-workspace.log`、`02-clippy.log`
* 剩余限制：`config/production.toml` 的 `[observability] otlp_endpoint` 仍是注释行；未新增该进程的 metrics 端点

**OBS-W4-03 · C1 · 已修复**
* 代码位置：`apps/business-api/src/metrics.rs`（原自建 `PrometheusBuilder`）
* 违反的契约：OBSERVABILITY_ARCHITECTURE 单一指标底座 + Prometheus 基数纪律（直方图应在服务端分桶，客户端 summary 会把基数与分位数算法带进应用）
* 复现方式：修复前 `render()` 输出含 `{quantile=`（客户端摘要），且与 `observability::metrics` 存在两套并行实现
* 影响：同一指标族在不同进程口径不一致，分位数不可跨实例聚合；两套实现必然漂移
* 修复范围：`install_metrics()`/`render()` 全部委托 `observability::metrics`
* 测试：`duration_metrics_render_as_bucketed_histograms`（断言存在 `authorization_duration_seconds_bucket{le="0.5"} 1` 且不存在 `{quantile=`）
* 证据：workspace 测试日志
* 剩余限制：`observability::metrics::install_metrics` 仍为进程内全局；多进程聚合依赖部署侧抓取，本波未验证

### C1 R2 REQUIRED — registered, NOT repaired

**OBS-W4-05 · C1**：无服务端关联身份；`x-request-id` 原样回显并写入持久列。
位置：`apps/business-api/src/routes/documents.rs:166-174`、`processing.rs:68,289-293`、
`upload.rs:31`、`api_error.rs:67`；列：`document_processing_jobs.correlation_id
VARCHAR(64)`、`audit_events.correlation_id UUID(PG)/TEXT(SQLite)`。
违反：API_AND_EVENT_CONTRACT_STANDARD（事件与错误必须携带服务端签发的关联标识）；
SECURITY_ARCHITECTURE（不得把未校验输入反射进公开响应）。
复现方式：静态检索（仅三处读 header，无中间件生成服务端 id，无长度/字符校验）；
行为推论（PG 侧 >64 字符将触发 22001，而 SQLite TEXT 列接受）由列定义给出，
**本波未执行该 HTTP 请求**，因此标注为 schema 级证据。
影响：(1) 攻击者可控制公开 `trace_id` 内容（响应反射 + 日志注入面）；
(2) 长 header 可能在生产后端造成持久化失败而在开发后端静默通过；
(3) 无 trace 连续性 → 请求↔任务↔provider 无法端到端串联（本轮 §五 的核心要求）。
修复范围：服务端生成关联 id（缺失时）、回显前长度/字符约束、显式跨进程传播契约。
传播载体属公开事件/协议面 → **依赖 ADR-0028（未接受）**，本轮不得自造事件信封，
故登记为 R2 阻塞而不是修复。
测试建议：中间件单元测试（有/无 header、超长 header）+ 双后端持久化断言。
证据：本文件 §3 引用的行号。
剩余限制：ADR-0028 未接受前只能收敛“回显与校验”，无法完成传播。

**OBS-W4-06 · BLOCKED（不是 PASS）**：exporter 不可用时业务结果不变，无法验证。
本环境无 OpenTelemetry Collector（无 docker/podman，`config/production.toml:41`
该行为注释）。按 AGENTS §12 记为 `NOT RUN / BLOCKED`，禁止用 Fake 声称通过。

**OBS-W4-07 · C1**：日志路径无脱敏控制。位置：`crates/observability/src/lib.rs`
（无 redaction layer），对照 `crates/audit/` 的 `redact_details` /
`sanitize_details_for_read`。违反：AGENTS §10（Secret/正文/签名 URL 不得进入日志）
+ QUALITY/SECURITY 对日志面的一致要求。复现方式：目前只有针对存储键的单点回归
（`log_hygiene.rs`）；任何新的 `tracing::*` 调用都无护栏，属结构性缺口。
影响：新代码可以在无门禁的情况下泄露正文/凭证；本轮修复是逐点式的。
修复范围：设计一个脱敏 layer（键名黑名单 + 值截断）并配契约测试 → 观测面公共
行为变更，建议先设计提案。
测试建议：注入 `authorization`/`secret`/`url` 字段断言被脱敏。
剩余限制：脱敏与排障成本的权衡需要 reviewer 判断。

### C2 / C3

* **OBS-W4-02 · C2 · 已修复**：provider crate 噪声压制（含测试
  `provider_debug_events_are_pinned_while_platform_debug_survives`）。剩余限制：
  第三方仍可能在 `warn` 级别输出桶/键，运维侧需要 Collector 过滤。
* **OBS-W4-04 · C3**：适配器诊断信息过窄。位置：
  `crates/document-sqlite/src/unit_of_work.rs:428`（注释明确登记）。为了让
  `document idempotency read failed` 不再携带驱动文本，诊断能力下降为固定消息；
  后续应在不携带底层文本的前提下补结构化诊断（错误 kind、SQLSTATE）。

## 6. Test matrix

| # | 测试路径 | 用例 | 命令 | 环境 | Verdict |
|---|---|---|---|---|---|
| O1 | `crates/observability/src/lib.rs` | `provider_debug_events_are_pinned_while_platform_debug_survives`、`default_directives_keep_the_requested_service_level`、`log_format_parse_is_case_insensitive_and_rejects_unknown` | `cargo test -p observability --all-features` | 进程内 recording layer | pass |
| O2 | `crates/observability/src/metrics.rs` | `install_is_idempotent_and_render_exposes_prometheus_text`、`duration_histograms_render_as_bucketed_prometheus_histograms` | 同上 | 进程内 | pass |
| O3 | `apps/business-api/src/metrics.rs` | `duration_metrics_render_as_bucketed_histograms` | `cargo test -p business-api --all-features` | 进程内 | pass（修复前 red：输出 `{quantile=`） |
| O4 | `apps/business-worker/src/main.rs` `metrics::tests` | 标签基数有界 | `cargo test -p business-worker --all-features` | 进程内 | pass |
| O5 | `crates/object-storage/tests/log_hygiene.rs` | 存储引用不进入日志 | `cargo test -p object-storage --test log_hygiene` | 自建 subscriber | pass |
| O6 | governance-worker 初始化 | 无专用测试（组合根初始化） | `cargo check --workspace --all-targets --all-features` | — | 编译级验证；运行期由 governance lanes 覆盖 |
| O7 | exporter 故障隔离 | — | — | 无 Collector | **NOT RUN / BLOCKED**（OBS-W4-06） |

## 7. R2 readiness

关闭条件：OBS-W4-05（关联契约，依赖 ADR-0028）、OBS-W4-06（真实 Collector 通道）、
OBS-W4-07（日志脱敏控制）。三者都在本波之前不存在任何证据；本波让它们在文档与
门禁意义上变成可执行、可判定的条目。

## 8. Reviewer questions

1. 在 ADR-0028 之前，是否可先只做「服务端生成 + 回显长度约束 + 结构化日志字段」而不引入任何跨进程信封？
2. `SUPPRESSED_TARGETS` 固定在 `warn` 是否会掩盖 provider 侧真实的可重试错误（例如 5xx 只在 debug 出现）？
3. 日志脱敏 layer 的键名黑名单是否与审计侧 `redact_details` 统一口径？

## 9. Verdict

**Self-verification only: READY FOR INDEPENDENT REVIEW / NOT INTEGRATED.**

### Blocking actions
OBS-W4-05（需 ADR-0028）、OBS-W4-06（需真实 Collector）、OBS-W4-07（需设计提案）。

### Deferred actions
OBS-W4-04；sampling 与 exporter 侧治理（明确留在观测后端，不在应用内实现）。

### Accepted differences from references
* 不引入 OTel SDK 的 span/tracer API，只用 `tracing` + 结构化字段：本仓当前没有跨进程
  trace 契约（ADR-0028 未接受），提前接入 SDK 会制造无法验收的承诺。
* 不在应用内做采样：OTel 规范允许在 Collector 侧处理，应用侧只保证级别与基数纪律。
* 指标口径以 Prometheus 文本为准，不承诺 OTLP metrics 通路（exporter 通道 BLOCKED）。
