# Wave 5A Repair 2 — 70 个 ignored 仍然是那 70 个：本轮的分类与差异

分母**不能**写成「70 个 PostgreSQL lane」。本文件由候选 `c22939f0a51e3c7fa2ec424f3819c6e7113a9b66` 上
`cargo test --workspace --all-features --target-dir target/gate-target` 的输出
（`gates/38-test-workspace-isolated-target.log`，`864 passed / 0 failed / 70 ignored`，191 个测试套件）
逐行生成：每条 `test … ignored, <原因>`，套件名取自其上的 `Running …` 头，包名按
`(套件, 测试名)` 与 Repair 1 的清单逐项对齐继承（不重新猜测）。分类规则与 Repair 1 相同，按 ignore
原因文本依序匹配：同时提到 PostgreSQL 与 MinIO/S3 → PostgreSQL + object storage；只提 PostgreSQL →
PostgreSQL；只提 MinIO/S3 → object storage；其余 → external model provider。

| 类别 | 本轮数量 | Repair 1 数量 |
|---|---|---|
| PostgreSQL | 38 | 38 |
| object storage (S3 / MinIO) | 28 | 28 |
| PostgreSQL + object storage | 3 | 3 |
| external model provider | 1 | 1 |
| **ignored 合计** | **70** | **70** |

## 差异

与 `docs/reviews/2026-09-29-wave5a-repair1-gates/ignored-tests-inventory.json` 逐项比对：

- 新增 ignored：**0**；消失 ignored：**0**；类别漂移：**0**；包名无法继承（新套件）：**0**。
  即本轮的 ignored 集合与 Repair 1 **逐条同名、同套件、同原因**，四类分母不变。
- 本轮新增的 9 个测试全部在 `tools/module-release-tool/tests/source_derivation.rs`，都可离线运行、
  **没有一条被 `#[ignore]`**；workspace 通过数从 855 增至 864、套件从 190 增至 191，正好是这 9 例。

逐条清单：`ignored-tests-inventory.json`（同 Repair 1 的字段结构：`Package` / `Target` / `Test` /
`Category` / `Reason`）。本轮比对用的机读摘要在 `ignored-tests-inventory.summary.json`。

## 这一轮里它们为什么一条也没跑

这些用例要求的环境本轮都不存在，也没有被授权搭建：PostgreSQL 实例（含 PLAN-0008 共享迁移目录，
identity / policy 与 organization 共用 `019_identity_authorization_foundation.sql` 与 `public` schema）、
MinIO 或 S3 兼容对象存储（`WAVE4_S3_*`）、可访问的内网模型 provider。它们**不计入通过数**，
也不能被引用为「真实环境已验证」。逐项状态见 `F1-closure.md` §7。