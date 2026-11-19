# 2026-11-19 交付状态：主干 CI 恢复、合同垂直切片计划、PLAN-0014 S1 整改

> 状态基线：`main` @ `19672b0`，与 `origin/main` 一致，工作区干净。
> 失败基线：CI run `44310190041` @ `e3d7c30`（10 作业 8 绿 2 红）。

## 1. P0 — 主干 CI 已恢复，完整 CI 全绿（已验收）

验收证据：

| Commit | CI run | 结论 |
|---|---|---|
| `75f526a` | `44475634539` | **success**，10/10 作业通过 |
| `19672b0`（当前 main HEAD） | `44476988954` | **success**，10/10 作业通过 |

通过作业：Format / Check / Clippy / Architecture Fitness / Security scanning（含 gitleaks + trivy）/ Frontend checks / Frontend Playwright smoke / CLI and MCP contracts / Unit tests / PostgreSQL + MinIO + E2E contracts。

### 1.1 变更 1：镜像拉取修复（可独立回滚）

- 分支 `codex/ci-minio-image-restore`，提交 `392d8f8`、`9250fee`；已合入 `main`（merge `ccaae47`、`75f526a`）。
- 根因：Quay `minio/minio` 与 `minio/mc` 匿名 token 的 `access.actions=[]`，manifest 返回 401；Docker Hub `minio/*` 404，ghcr 403；上游维护者确认 MinIO 已改为 source-only 发布。见 [证据文档](2026-11-19-CI-MINIO-IMAGE-RESTORE-EVIDENCE.md)。
- 修复：服务端与客户端统一使用公开、固定 tag 的 `openebs/minio:RELEASE.2024-12-18T13-15-44Z`（真实上游二进制，含 `curl`、`mc`、`/usr/bin/sh`，保留 healthcheck 与 bucket 初始化）；同步修正 multiprocess E2E 脚本与 dev/demo/generic compose 中失效的镜像引用。
- 验证路径：run `44474318859` 证明服务端可拉取且全部真实 PostgreSQL + MinIO 契约通过；其失败点是脚本中残留的 quay `mc` 引用，修正后 run `44475634539`、`44476988954` 全部通过。
- 回滚：revert 该分支两个提交；不涉及 Cargo、Rust 源码、迁移或数据库。

### 1.2 变更 2：Rust 依赖 / RustSec 修复（可独立回滚）

- 分支 `codex/ci-rustsec-dependency-updates`，提交 `7f7dc0b`；已合入 `main`（merge `2393eb5`）。
- 17 项通过升级解决：quinn-proto/quinn-udp、rustls/rustls-webpki/webpki-roots、rustix、tokio、ctutils、crypto-bigint（经 p256 0.13→0.14 根约束，含两处测试辅助函数 `to_sec1_point` 机械改名）、pin-utils 从 lock 移除。
- 3 项逐条带理由与复审触发条件的 `ignore`（无 blanket ignore）：`RUSTSEC-2026-0459`（libsqlite3-sys，sqlx 0.8 锁定 0.30.x，sqlx 0.9 源码迁移超出依赖变更范围）、`RUSTSEC-2026-0543`（chrono 全版本 unmaintained，替换会改公共契约）、`RUSTSEC-2023-0071`（rsa 仅未编译的 sqlx mysql 可选依赖）。详见 [依赖修复说明](../operations/CI_44310190041_RUSTSEC_DEPENDENCY_REMEDIATION.md)。
- 门禁：`cargo audit --deny warnings`、fmt、check、clippy 本地通过；CI run `44476988954` 的 Security 作业（audit + gitleaks + trivy）全绿。
- 回滚：revert `7f7dc0b`；无运行时副作用。

**必须披露的风险接受**：libsqlite3-sys 的 bundled SQLite CVE 是真实本地攻击面，未修复，仅在 `.cargo/audit.toml` 中记录风险接受；应通过专门的 sqlx 0.9 迁移计划关闭。

## 2. P1 — 合同业务垂直切片计划：已编制、已独立评审、已合并（仍 NOT ACTIVE）

- [PLAN-0017](../plans/current/PLAN-0017-contract-business-vertical-slice.md)：Proposed / NOT ACTIVE，首个闭环固定为合同列表与详情 → 文档修订 → 抽取与证据 → 人工审阅 → 应用候选 → 生成合同新版本；Contract Management 拥有正式字段与版本；五项硬验收为租户隔离、幂等、版本冲突、已签版本保护、审计追溯；首轮仅脱敏纯文本/Markdown/JSON。
- [独立评审](../reviews/2026-11-19-plan0017-independent-review.md)：**PASS WITH COMMENTS**，7 条发现（1×C2、6×C3）已全部吸收到计划正文；评审确认边界、所有权、Candidate/Job Completed 隔离、Astryx 约束、AGENTS.md §13 必备要素均通过。Gate-0（本文 §1）已满足，Gate-1 仍待 Owner 冻结字段集、签署规则、权限键与 SLO。
- [PR #19 重评估](../reviews/2026-11-19-pr19-contract-frontend-recheck.md)：PR #19 为文档提案，其 `PLAN-0016` 子计划编号与已归档 Astryx PLAN-0016 冲突；建议并入 PLAN-0017 受门禁前端工作包或另取编号，不得原样以 PLAN-0016 合并。Contract Apply 与文档内容预览 API 未落地，相关 UI 保持 BLOCKED。
- 计划索引已更新；PLAN-0014 仍 Active、PLAN-0006 仍 BLOCKED、PLAN-0015 仍 Proposed。

## 3. PLAN-0014 — S1 整改完成并通过独立复评；S2 未授权

- 整改分支 `codex/plan0014-wave5b-s1-cleanup-boundary`，核心提交 `b4ccbe8`（C-1..C-4），另有集成/兼容修复提交；draft [PR #20](https://github.com/jesenzhang/business-platform/pull/20) 仅用于完整 CI 与复评，**未合并**。
- C-1：每次动作前重证祖先链至卷根且仍要求经过 walk root；新增“首次枚举时替换 walk root 之上组件”回归，fail-first 外部 sentinel 被删/退出 1，修复后具名拒绝/退出 0。
- C-2：动作前重读子项身份；retention 一律使门禁非零退出；新增动态、static 与正控回归。
- C-3：更正 fail-first 来源、证据范围与 LF/CRLF 哈希口径；C-4：默认 `target/tmp` 可在干净工作区创建，显式 `-Scratch` 仍 fail-closed。
- 本地/独立证据：driver 113 例（112 matched / 1 BLOCKED / 0 mismatched）、repairs 33/33、4 条新场景 fail-first→PASS、module-release-tool 108 passed、fmt 与 architecture PASS。
- [独立复评](../reviews/2026-11-19-wave5b-s1-cleanup-boundary-rereview.md)：**PASS WITH COMMENTS**，解除上一轮 CHANGES REQUESTED；授权进入 S2 需先确认完整 CI 通过；**R2 admission 仍 BLOCKED**。
- **CI 未全绿（不得写成通过）**：PR #20 的 Security 与 Architecture 已转绿；剩余失败在 PLAN-0014 分支特有的对账/证据测试中：`verification_guards` 的 source-revision fixture 与刷新后依赖不兼容，以及 Wave 4 S3 provider lane 的 bucket 创建错误识别、multipart upload 发现场景（后者按测试自身要求应记为 provider limitation / 非绿）。这些不是 `main` 的门禁，`main` 全绿不受影响。S2 与 R2 准入在这些项关闭前不启动。

## 4. 后续条件

1. PLAN-0017 Gate-1：Owner 冻结字段集、签署语义与权限键、Process Manager 范围、SLO/RPO/RTO；需要改公开契约时先建 ADR。
2. PR #19 处置：并入 PLAN-0017 前端工作包或另取编号；等待 Contract Apply / Document Preview API Owner 承诺。
3. PLAN-0014：在分支修复剩余两项 CI 失败并重跑完整 CI，随后在干净态复跑 4 条新场景 + 113 driver；未通过前 S2 与 R2 admission 不启动。
4. 另行计划：sqlx 0.9 源码迁移以移除 libsqlite3-sys 风险接受。
