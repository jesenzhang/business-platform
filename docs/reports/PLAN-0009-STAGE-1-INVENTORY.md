# PLAN-0009 Stage 1 — Read-only inventory and frozen manifest

状态：Candidate，等待独立 Stage 1 Reviewer

记录日期：2026-08-10

实现 candidate：`0f342faaf65e5230239f967465c2dce596e1d153`

本 report 随后作为同一 Stage 1 candidate 的审计索引提交；独立 Reviewer 以实现 commit 加本 report 的最终 commit 作为审查范围。

## Source boundary

本 Stage 只读取用户指定的 C 项目 local-test source tuple：

- env：`F:\Workspace\git_repo\contract_management\backend\.env.local-test`
- `DATA_ROOT`：`D:\contract_data_test`
- authoritative SQLite：`$DATA_ROOT/db/contract_management.db`
- physical roots：`$DATA_ROOT/datasets`、`$DATA_ROOT/2026年合同1`、`$DATA_ROOT/2026年合同`
- isolated output：`F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810\stage-1-inventory-v7`

SQLite 以 read-only、single-connection 方式打开；配置边界同时保护 C repository root 和 `DATA_ROOT`。Stage 1 没有 C 应用启动、上传、导入、迁移或 source DB write。所有输出仅进入上述 isolated target。

## Deterministic inventory

实现位于 `apps/plan-0009-rehearsal`，纯选择和 classification vocabulary 位于 `crates/legacy-migration-rehearsal`。SQLite adapter 只使用固定 SQL 查询，覆盖：

- `contracts`、`contract_versions`、`contract_attachments`；
- 三种 ingestion/legacy file lineage：`contract_ingestions`、`contract_ingestion_tasks`、`contract_ingestion_task_files`、`contract_ingestion_task_results`；
- `contract_artifacts`、`contract_parse_jobs`、`extraction_results`。

sample selection 固定为 120 条，排序规则为：classification 覆盖和 lineage 特征（多版本、多附件、parse/extraction、task files、多证据、OCR、EXTRACTED_JSON）按固定优先级加入，其次正向 source contract flag，最后 `contracts.id ASC` 作为 tie-break；manifest 按 selection rank 保留该 coverage-first 顺序。样本不足时 fail closed；同一 source manifest 不依赖数据库 cursor 顺序。

物理身份优先使用实际读取文件的 SHA-256。源库的 32 字符 legacy fingerprints 被保留为 `legacy_fingerprint_count`，不会冒充 SHA-256；因此缺少可验证 SHA-256 的安全路径只会得到 `Probable`，不会被升级成 `Exact`。

Manifest 不写 customer/name/raw text、绝对 source path、数据库 URL、secret、signed URL 或明文 source-relative path；证据只使用 root label、不可逆的 relative-path SHA-256、depth/extension/size 等安全 metadata，并记录 source table/record id、lineage counts、artifact kinds、expected/observed digest、classification reason。Stage 2 必须通过 source_contract_id 重新解析并校验该 path fingerprint。

## Real source evidence

权威 source fingerprint 记录在 v7 manifest 中：

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

v7 的 120-contract sample：

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

`F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810\stage-1-inventory-v7\manifest-v1.json`

- schema：`plan-0009.stage-1.inventory.v7`
- canonical manifest content SHA-256（去除 digest 字段后）：`f1048572cab3c7ff7f79a9cb46cf3fc75d1838bde484956ee7ad33b107f72cdf`
- written manifest file-bytes SHA-256：`c495a3da19d753db26af58fbdc73e4c8690cf88f3f67c55e0f6e1ac8be64e943`
- digest sidecar：`manifest-v1-digests.json`，同时校验 canonical digest 与 file-bytes digest
- replay audit：`replay-audit-v1.json`

首次真实运行：

```text
stage=1 status=frozen selected=120 replayed=false
canonical_manifest_sha256=f1048572cab3c7ff7f79a9cb46cf3fc75d1838bde484956ee7ad33b107f72cdf file_bytes_sha256=c495a3da19d753db26af58fbdc73e4c8690cf88f3f67c55e0f6e1ac8be64e943
classifications=Exact=0,Probable=1,Ambiguous=89,Conflict=0,Orphan=29,Missing=0,Rejected=1
```

第二次使用相同 source tuple 的 replay：

```text
stage=1 status=replayed selected=120 replayed=true
canonical_manifest_sha256=f1048572cab3c7ff7f79a9cb46cf3fc75d1838bde484956ee7ad33b107f72cdf file_bytes_sha256=c495a3da19d753db26af58fbdc73e4c8690cf88f3f67c55e0f6e1ac8be64e943
classifications=Exact=0,Probable=1,Ambiguous=89,Conflict=0,Orphan=29,Missing=0,Rejected=1
```

`replay-audit-v1.json` 持久化 first run `status=frozen` 和 `replay_count=0`，第二次后为 `last_status=replayed`、`replay_count=1`；manifest 内容和两个 digest 均未改变。已有 manifest、sidecar 或 audit 内容不一致时 adapter fail closed，不覆盖 frozen artifact。

## Focused verification

已运行：

```text
rtk cargo fmt --all
rtk cargo check -p plan-0009-rehearsal --all-targets --all-features
rtk cargo test -p plan-0009-rehearsal --all-features   # 6 passed
rtk cargo test -p legacy-migration-rehearsal --all-features   # 8 passed
```

其中回归测试覆盖了大于 1 MiB 输入的 SHA-256 读取，避免 Windows 栈溢出；Stage 1 不运行完整 workspace test suite。首次实现 draft 的 stack overflow 已修复并未被当作数据失败；v7 才是本 Stage 的唯一 authoritative candidate，v1-v6 仅保留在 isolated workspace 作为作废修复轨迹。

## Stage 1 exit decision

满足：真实 source read-only inventory、固定 120 selection、source fingerprint、SHA-256/legacy fingerprint 区分、classification census、OCR/parse lineage、isolated frozen manifest、digest replay 和 focused tests。

下一步：由独立 Reviewer 审查 `a8bf104..` 到包含实现 `0f342fa...` 与本 report 的最终 candidate，重点检查 SHA-256 物理身份、三种 legacy ingestion 路径、source/target boundary、classification fail-closed、lineage completeness、明文路径不泄漏、双 digest 与 durable replay audit。Reviewer `FAIL` 时旧 candidate 立即作废并启动新的 repair worker。
