# PLAN-0009：C Legacy Contract & Document Migration Rehearsal

文档 ID：PLAN-0009  
版本：0.1  
状态：Active / Rehearsal Only  
日期：2026-08-10  
Owner：Platform Foundation / Document Management / Document Intelligence  
前置集成：PLAN-0008 `Integrated`，执行基线为真实 main
`654fe83d82107d899079d20e5fef8aaf4d5431b8`（旧 SHA 仅作历史参考）

## 1. 目标

证明 PLAN-0008 的 Document、DocumentRevision、DocumentLink、ProcessingRun、
ProcessingArtifact、Evidence 模型能够承载 C 项目的真实历史数据。第一阶段
只做 read-only inventory/analyzer 驱动的隔离 rehearsal，不做生产迁移，不改变
C 项目任何状态。

## 2. 仅限第一阶段的范围

- frozen read-only source manifest；
- deterministic target UUID map；
- complete lineage classification；
- isolated SQLite target 与 LocalStorage objects；
- 覆盖代表性场景的 120-contract rehearsal；
- replay/idempotency；
- SHA-256/object validation；
- revision/run/artifact/evidence integrity；
- stale/conflict assertions；
- orphan/missing/ambiguous/conflict quarantine；
- migration verification report。

样本 manifest 必须覆盖普通单文件、多版本、扫描件、多附件、OCR/LLM 和已知
错误关联。规模扩大前先冻结样本选择、输入 hash、分类规则和 target UUID map。

## 3. Read-only source boundary

C 项目位于 `F:\Workspace\git_repo\contract_management`，是只读 source：

- 不修改 C 项目代码、原数据库、数据库文件、原始文件或 storage root；
- 不运行 destructive migration，不覆盖任何原始内容；
- 所有 target SQLite、LocalStorage object、manifest、quarantine 和 report
  只写入隔离 workspace，例如
  `F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810`；
- 物理文件身份必须由 SHA-256 建立，不得仅信任旧 `file_id` 或 path；
- metadata-only contract 不得虚构 DocumentRevision；orphan file 不得自动
  变成业务事实。

## 4. Canonical mapping and classification

Legacy → PLAN-0008 的 mapping 需要显式输出以下目标实体：

`Document` · `DocumentRevision` · `DocumentLink` · `ProcessingRun` ·
`ProcessingArtifact` · `Evidence`

每条 mapping 必须标注：

`Exact` · `Probable` · `Ambiguous` · `Conflict` · `Orphan` · `Missing` ·
`Rejected`

只有 `Exact` 才可进入自动 rehearsal 写入；`Probable` 必须保留证据并进入人工
复核策略。`Ambiguous`、`Conflict`、`Orphan`、`Missing` 和 `Rejected` 进入
quarantine 或 report，不得自动修复、合并或升级为正式事实。

Lineage 必须可回答：

`Contract → File → Version/Attachment → physical SHA-256 object → OCR → LLM`

OCR 无法解析 revision、LLM 无法解析 processing lineage、错误关联和多引用内容
都必须保留原始证据与分类原因。

## 5. Rehearsal invariants

- 相同 frozen manifest 重放产生相同 target UUID、object key、revision identity
  和 mapping classification；
- object bytes、SHA-256、SQLite metadata、revision checksum 和 artifact/evidence
  bindings 一致；
- 历史 revision 不可变，processing run 精确绑定 revision；
- artifact/evidence 精确绑定 revision/run，重放不产生重复事实；
- stale version、冲突 relation、缺失 object、orphan object 和 ambiguous mapping
  均 fail closed；
- rehearsal 只能证明隔离 target 的可重放性，不代表生产迁移已获授权。

## 6. 禁止事项

本计划 Proposed 阶段不授权任何实现或迁移。明确禁止：

- 修改 C 项目、C 原数据库或 C 原 storage；
- production migration；
- Ambiguous/Conflict auto repair；
- metadata-only contract 虚构 revision；
- orphan file 自动成为业务事实；
- 激活 PLAN-0006 或扩大 PLAN-0008 产品范围。

## 7. Activation gate

只有在单独接受本计划、冻结 source manifest 和目标隔离边界后，才可建立独立
feature branch。Activation 前必须复核 source connection/storage 位置、schema、
三种导入路径、OCR/LLM lineage、样本覆盖、quarantine 策略、回滚方案和完整
 verification report。当前不开始正式迁移。

## 8. Activation record

本计划已于 2026-08-10 激活，但仅授权隔离的 read-only rehearsal。激活证据、
source fingerprint、目标边界、清理方式和真实 C 项目盘点见：
[`docs/reports/PLAN-0009-STAGE-0-ACTIVATION.md`](../../reports/PLAN-0009-STAGE-0-ACTIVATION.md)。

激活不改变 C 项目、不授予 production migration 权限、不改变 PLAN-0008 的已集成
边界，也不激活 PLAN-0006。后续阶段仍必须以独立 candidate 和 reviewer PASS 为
进入条件。

## 9. 架构符合性

- Bounded Context：Document Management 与 Document Intelligence；本计划是
  read-only Anti-Corruption Layer/rehearsal，不新增业务上下文。
- 权威数据所有者：Document Management 拥有 Document/Revision/Link；Document
  Intelligence 拥有 ProcessingRun/Artifact/Evidence 及候选处理状态；C 项目只作为
  不可信历史 source，绝不成为 target 写入者。
- 一致性：source 只读；target 仅为隔离 SQLite/LocalStorage；相同 frozen manifest
  必须确定性重放；不确定映射 fail-closed 进入 quarantine。
- 安全：source/target 路径 guard、生产模式拒绝、敏感原文/内部 key/凭证不进入公开
  DTO 或日志；C 项目既有 dirty working tree 作为外部基线保留。
- 质量与门禁：focused domain/application tests、replay/integrity tests、真实
  120-contract rehearsal、source read-only proof、Architecture Fitness 和完整
  workspace gates 均为完成条件。
- 文档同步：Stage 0 activation evidence；后续实现、verification report、review
  ledger 与 closeout 必须在同一计划变更中更新。
