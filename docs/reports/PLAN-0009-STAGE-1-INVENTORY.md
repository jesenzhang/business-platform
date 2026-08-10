# PLAN-0009 Stage 1 — Read-only inventory and frozen manifest

状态：Candidate，等待独立 Stage 1 Reviewer

记录日期：2026-08-10

实现 candidate：`bd539869fa08db5d0a4c8fa703dd4549897b88bd`

## Source boundary

本 Stage 只读取用户指定的 C 项目 local-test source tuple：

- env：`F:\Workspace\git_repo\contract_management\backend\.env.local-test`
- `DATA_ROOT`：`D:\contract_data_test`
- authoritative SQLite：`$DATA_ROOT/db/contract_management.db`
- physical roots：`$DATA_ROOT/datasets`、`$DATA_ROOT/2026年合同1`、`$DATA_ROOT/2026年合同`
- isolated output：`F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810\stage-1-inventory-v6`

SQLite 以 read-only、single-connection 方式打开；配置边界同时保护 C repository root 和 `DATA_ROOT`。Stage 1 没有 C 应用启动、上传、导入、迁移或 source DB write。所有输出仅进入上述 isolated target。

## Deterministic inventory

实现位于 `apps/plan-0009-rehearsal`，纯选择和 classification vocabulary 位于 `crates/legacy-migration-rehearsal`。SQLite adapter 只使用固定 SQL 查询，覆盖：

- `contracts`、`contract_versions`、`contract_attachments`；
- 三种 ingestion/legacy file lineage：`contract_ingestions`、`contract_ingestion_tasks`、`contract_ingestion_task_files`、`contract_ingestion_task_results`；
- `contract_artifacts`、`contract_parse_jobs`、`extraction_results`。

sample selection 固定为 120 条，排序规则为：classification 覆盖和 lineage 特征（多版本、多附件、parse/extraction、task files、多证据、OCR、EXTRACTED_JSON）优先，其次正向 source contract flag，最后 `contracts.id ASC`。样本不足时 fail closed；同一 source manifest 不依赖数据库 cursor 顺序。

物理身份优先使用实际读取文件的 SHA-256。源库的 32 字符 legacy fingerprints 被保留为 `legacy_fingerprint_count`，不会冒充 SHA-256；因此缺少可验证 SHA-256 的安全路径只会得到 `Probable`，不会被升级成 `Exact`。

Manifest 不写 customer/name/raw text、绝对 source path、数据库 URL、secret 或 signed URL；证据使用 root label + source-relative path，并记录 source table/record id、lineage counts、artifact kinds、expected/observed digest、classification reason。

## Real source evidence

权威 source fingerprint 记录在 v6 manifest 中：

- DB bytes：`194637824`
- Alembic：`0057`
- journal mode：`wal`
- `integrity_check`：`ok`
- `foreign_key_check` violation count：`32`；该异常被保留，未自动修复
- physical root totals：`datasets=12407 files / 31557363061 bytes`、`external_contracts=3569 / 31717614058`、`repair_candidates=3241 / 31110378782`

全库 classification census：

| Classification | Count |
| --- | ---: |
| Exact | 0 |
| Probable | 6 |
| Ambiguous | 644 |
| Conflict | 0 |
| Orphan | 208 |
| Missing | 0 |
| Rejected | 634 |

v6 的 120-contract sample：

| Classification | Count |
| --- | ---: |
| Exact | 0 |
| Probable | 1 |
| Ambiguous | 89 |
| Conflict | 0 |
| Orphan | 29 |
| Missing | 0 |
| Rejected | 1 |

样本实际覆盖 2 个多版本合同、1 个多附件合同、5 个 parse/extraction lineage、1 个 OCR artifact record、5 个 RAW-result lineage record，以及 `SOURCE`、`PREVIEW_IMAGE`、`RAW_JSON`、`PARSED_JSON`、`EXTRACTED_JSON` artifact kinds。Source tuple 下 `Missing` 和 `Conflict` census 均为零，不能为了满足样本表而伪造这些分类；Ambiguous/Orphan/Rejected 均 fail closed。

## Frozen artifact and replay

权威 frozen artifact：

`F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810\stage-1-inventory-v6\manifest-v1.json`

- schema：`plan-0009.stage-1.inventory.v6`
- manifest SHA-256：`b31d742b6cc07a87a26bb0452fdd5d2751805a3723887e34e38080cb3db762a5`
- sidecar：`manifest-v1.sha256`

首次真实运行：

```text
stage=1 status=frozen selected=120 replayed=false
classifications=Exact=0,Probable=1,Ambiguous=89,Conflict=0,Orphan=29,Missing=0,Rejected=1
```

第二次使用相同 source tuple 的 replay：

```text
stage=1 status=replayed selected=120 replayed=true
classifications=Exact=0,Probable=1,Ambiguous=89,Conflict=0,Orphan=29,Missing=0,Rejected=1
```

两次 digest 相同；已有 manifest 内容不同时 adapter 返回 `manifest_conflict`，不会覆盖 frozen artifact。

## Focused verification

已运行：

```text
rtk cargo fmt --all
rtk cargo check -p plan-0009-rehearsal --all-targets --all-features
rtk cargo test -p plan-0009-rehearsal --all-features   # 4 passed
rtk cargo test -p legacy-migration-rehearsal --all-features   # 8 passed
```

其中回归测试覆盖了大于 1 MiB 输入的 SHA-256 读取，避免 Windows 栈溢出；Stage 1 不运行完整 workspace test suite。首次实现 draft 的 stack overflow 已修复并未被当作数据失败；v6 才是本 Stage 的唯一 authoritative candidate。

## Stage 1 exit decision

满足：真实 source read-only inventory、固定 120 selection、source fingerprint、SHA-256/legacy fingerprint 区分、classification census、OCR/parse lineage、isolated frozen manifest、digest replay 和 focused tests。

下一步：由独立 Reviewer 审查 candidate `bd539869...`，重点检查 SHA-256 物理身份、三种 legacy ingestion 路径、source/target boundary、classification fail-closed、lineage completeness 和 replay determinism。Reviewer `FAIL` 时旧 candidate 立即作废并启动新的 repair worker。
