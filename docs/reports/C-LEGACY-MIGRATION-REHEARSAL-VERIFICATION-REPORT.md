# PLAN-0009 Stage 5 — C legacy migration rehearsal verification report

Status: **Stage 5 evidence assembled — independent Reviewer pending**
Recorded: 2026-08-10 (Asia/Shanghai)
Scope: real C-project read-only migration rehearsal only
Base SHA: `5de6a995b874c93bdc97486391aed8c2d5920462` (`5de6a99`)
Candidate SHA: 97336d220d87cdbd730f35950ce60b723707aa2f
Independent Reviewer: **PENDING**

## 1. Decision

**Readiness decision: `REHEARSAL_PASS_WITH_MANUAL_REVIEW_REQUIRED`**

This is a rehearsal decision, not production authorization. Stages 0–4
provide real, isolated evidence for inventory, deterministic mapping, dual
replay, integrity handling, interrupted recovery, and partial-target rejection.
The real selected sample contains no `Exact` record, so no formal target fact
was materialized. One `Probable` record requires human review and 119 records
are quarantined. PLAN-0009 remains **Active / Rehearsal Only** and PLAN-0006
remains **Proposed / NOT ACTIVE**.

No command in the Stage 0–4 evidence wrote the C repository, the C source
database, `D:\contract_data_test`, or a production system. All generated target
state was under the isolated rehearsal root.

## 2. Scope, source boundary, and legacy import paths

The source boundary is the complete read-only tree
`F:\Workspace\git_repo\contract_management`, using the local-test tuple
recorded by Stage 0 and Stage 1:

- environment file:
  `F:\Workspace\git_repo\contract_management\backend\.env.local-test`;
- data root: `D:\contract_data_test`;
- source database: `$DATA_ROOT/db/contract_management.db`;
- physical roots: `$DATA_ROOT/datasets`, `$DATA_ROOT/2026年合同1`, and
  `$DATA_ROOT/2026年合同`;
- isolated rehearsal root:
  `F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810`.

The source database snapshot in the frozen manifest is 194,637,824 bytes,
Alembic revision `0057`, WAL mode, `integrity_check=ok`, and 32 retained
foreign-key violations. The violations and database/filesystem count
differences are source evidence; rehearsal code does not repair or infer from
them.

The analyzer covered the three observed legacy entry paths without invoking
any importer or application write path:

| Legacy path | Evidence covered | Target-lineage consequence |
| --- | --- | --- |
| Standalone `contract_importer` | Excel parsing, file discovery, `contracts.json`/`manifest.json`, then the backend importer or upload flow; both external relative-path (`--in-place`) and legacy uploaded-path forms | Contract, version, attachment, physical path and SHA-256 evidence remain source provenance; old ID/path is not physical identity |
| Backend directory import | `backend/utils/import_contracts_from_directory.py`, exposed by the import scripts, combining template parsing and directory discovery | Contract/version/attachment references are classified from database rows and physical bytes, not re-imported |
| Application ingestion | `contract_ingestions`, `contract_ingestion_tasks`, `contract_ingestion_task_files`, task results, versions and artifacts, including upload task kinds | Ingestion/task-file/task-result relationships feed parse/extraction and artifact lineage; uncertain relationships are quarantined |

The resulting lineage path is:

`Contract → Version/Attachment → physical SHA-256 object → Ingestion /
Task / Task File / Task Result → Parse Job / Extraction Result → OCR or
structured artifact → LLM-related processing lineage`.

The analyzer preserved safe table/row provenance, path digests, content
digests, sizes, extensions, lineage counters, and classification reasons. It
did not write source records, expose raw source text, or treat a legacy ID,
path, parser result, or model output as a target business fact by itself.

## 3. Frozen manifest identity and audit hashes

The authoritative frozen input is:

`F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810\stage-1-inventory-v9\manifest-v1.json`

Its identity is:

- schema: `plan-0009.stage-1.inventory.v9`;
- selection limit: `120` contracts;
- canonical manifest SHA-256:
  `8376eac8c5aa2447077048f3a50d68c3584e3df929d3473a865f995f5319cb43`;
- manifest file-bytes SHA-256:
  `759e8f96b9555b697a83798ace0d3a888fb8010bb816113bcddd05d517ab13aa`;
- source environment-file SHA-256:
  `0125359b073d9f22286d720a77a020e0554dc93cf6e2914de5aad41f696aa7da`;
- source database SHA-256:
  `08dd1a0458abc41aae330a66cf65c12caddb2319444ae8e0c16e288a3fa1aa6a`.

The following hashes were read directly from the real artifacts. “Canonical”
is the digest recorded by the artifact’s sidecar or audit; “file bytes” is a
direct SHA-256 of the artifact file.

| Artifact | Identity / audit hash |
| --- | --- |
| `stage-1-inventory-v9/manifest-v1-digests.json` | file bytes `aceba45911727f9fac846be16f6cf1a66ed05e03612f3d3767f9f4516d955666`; records canonical `8376eac8…f5319cb43` and file bytes `759e8f96…517ab13aa` |
| `stage-1-inventory-v9/replay-audit-v1.json` | file bytes `4f183e138d9b91cb1e89b00f7ff48b586e0127f510677671edfd555f0b2887e2`; `replay_count=1`, `last_status=replayed` |
| `stage-2-rehearsal-v2/mapping-plan-v1.json` | canonical mapping SHA-256 `f5aae49415500877e0e9db753cbfea9567493b611c3d7eac62ba317f05f021e0`; file bytes `e00a2769b53e68212a98cdeb4939a0ca668f737be909844568b73c9f7b220e59` |
| `stage-2-rehearsal-v2/mapping-plan-v1-digests.json` | file bytes `13e74a30b400a79d3589d18f2e08fb991ac8513198c9711a196ab40313a4a39b` |
| `stage-2-rehearsal-v2/rehearsal-audit-v1.json` | file bytes `8e5c7af48d93a787772cf82ef8dbdbf4c029b82234524481a7afd85c6ecf6c0b`; `replay_count=1`, `last_status=replayed` |
| `stage-3-rehearsal-v1/stage3-replay-audit-v1.json` | SHA-256 `fb312b732b9879df1471dab22fda9b22848fdeb44b8cd269102c14f07f16fd6c`; `replay_equal=true` |
| `stage-4-integrity-recovery-v1/stage4-integrity-recovery-audit-v1.json` | SHA-256 `619002f0b7050ad4d9c3c15bdcc3b063b76d0e79af62c3f03889caa3653e167f` |

The Stage 4 audit also records matrix SHA-256
`707e368accbcb6bf99fa1b25222f97515a4fa640875f026c122d3dac1e7e4686` and
the partial mapping artifact SHA-256
`88214a42ff84080fd5018ad6130d39514bf8779c454db8823facf71c1ba5dd5c`.

The abbreviated hashes in the table are only visual shortening; the full
values above and in the real artifacts are authoritative.

## 4. Sample methodology and representativeness

The source contains 1,492 contract rows. The selector used the frozen,
coverage-first rule from Stage 1:

1. cover classification and lineage features;
2. cover the positive source-contract flag;
3. use `contracts.id ASC` as the deterministic tie-break;
4. stop at the fixed limit of 120.

The 120 is representative of the rehearsal’s operational risk surface, not a
statistical estimate of the whole source population. It is reproducible from
the real source and deliberately includes the available multi-version,
attachment, OCR, structured-artifact, rejected-relationship, ambiguous-match,
and orphan cases. The manifest-derived coverage is:

| Scenario | Selected count | Evidence / derivation |
| --- | ---: | --- |
| Ordinary single file only | 0 | No selected record has only an ordinary unique-file lineage |
| Multi-version | 2 | `versions > 1` |
| Scanned / OCR | 1 | `ocr_artifacts > 0` |
| Attachments | 10 | `attachments > 0` |
| LLM / structured | 5 | Structured JSON artifact kind present |
| Known bad relationship | 1 | Rejected source relationship |
| Duplicate / multiple physical matches | 89 | `Ambiguous` classification |
| Missing or non-usable evidence | 90 | 89 ambiguous plus 1 rejected; this is not the `Missing` classification |
| Orphan | 29 | `Orphan` classification |
| Conflict | 0 | No real conflict case exists in the source tuple |

The source tuple has `Missing=0` and `Conflict=0`, and the selected sample has
the same zero values. No unavailable Exact, conflict, or missing case was
fabricated. The absence of an ordinary-single-file case is also recorded as
zero rather than simulated.

## 5. Classification, mapping, and target binding

The real source census and frozen selected distribution are:

| Classification | Full source census | Selected 120 |
| --- | ---: | ---: |
| Exact | 0 | 0 |
| Probable | 6 | 1 |
| Ambiguous | 644 | 89 |
| Conflict | 0 | 0 |
| Orphan | 208 | 29 |
| Missing | 0 | 0 |
| Rejected | 634 | 1 |
| **Total** | **1,492** | **120** |

Mapping policy is deterministic and fail-closed:

- only one observed SHA-256 `Exact` record may be auto-materialized;
- `Probable` is `manual_review`;
- `Ambiguous`, `Conflict`, `Orphan`, `Missing`, and `Rejected` are
  `quarantine`;
- a candidate UUID is a proposal identifier, not a formal fact.

The real mapping plan contains 120 immutable-by-digest ledger rows:

| Mapping result | Count | Real reason/disposition |
| --- | ---: | --- |
| Exact eligible / materialized | 0 / 0 | No selected `Exact` record |
| Manual review | 1 | `Probable`, `path_and_size_match` |
| Quarantine | 119 | 89 `multiple_physical_matches`, 29 `no_source_lineage`, 1 `has_contract_false` |

The target binding evidence is present in the mapping proposal, but formal
target rows are intentionally absent:

| Binding field / target observation | Evidence |
| --- | ---: |
| `candidate_document_id` proposals | 120 / 120 mapping rows |
| `candidate_revision_id` proposals | 120 / 120 |
| `candidate_link_id` proposals | 120 / 120 |
| `candidate_processing_run_id` proposals | 120 / 120 |
| `candidate_artifact_id` proposals | 120 / 120 |
| `candidate_evidence_id` proposals | 120 / 120 |
| deterministic `target_object_ref_sha256` proposals | 120 / 120 |
| rows with `evidence_path_sha256` | 91 |
| rows with observed physical SHA-256 | 1 |
| mapping ledger rows | 120 |
| materialized mapping rows | 0 |
| formal Documents / Revisions / Links / ProcessingRuns / Artifacts / Evidence | 0 each |
| target audit/outbox rows | 0 each |
| target object files / bytes | 0 / 0 |

Both Stage 3 target snapshots also have
`integrity_check=ok`, `quick_check=ok`, zero foreign-key violations, zero
duplicate mapping keys, zero duplicate formal facts, and empty object-root
digest
`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.
The candidate IDs and object references demonstrate deterministic binding
proposal generation. They are not claimed as Document, DocumentRevision,
DocumentLink, ProcessingRun, ProcessingArtifact, or Evidence facts because
the eligibility gate correctly materialized none.

## 6. OCR, extraction, and LLM/structured lineage

The real Stage 1 manifest and Stage 3 audit preserve these selected lineage
totals:

| Lineage item | Selected count | What the evidence establishes |
| --- | ---: | --- |
| Physical evidence entries | 22,121 | Hashed physical evidence and source provenance entries |
| Multi-source evidence entries | 21,570 | Complete ordered contributing source-table/row provenance |
| Contract versions | 92 | Version lineage counts |
| Attachments | 13 | Attachment lineage counts |
| Artifacts | 217 | Source artifact lineage counts/kinds |
| Ingestions | 91 | Ingestion lineage |
| Ingestion tasks | 6 | Task lineage |
| Task files | 36 | Task-file lineage |
| Task results | Preserved in source inventory | Task-result table is included in the frozen source census and lineage path |
| Parse jobs | 6 | Parse lineage |
| Extraction results | 6 | Extraction-result lineage |
| Selected OCR artifact lineages | 1 | Real selected scanned/OCR lineage |
| Structured artifact lineages | 22 | Real structured-artifact lineage |
| Legacy fingerprint observations | 785 | Legacy fingerprints retained as evidence, never promoted to SHA-256 |

Verified from the real manifest/audit:

- the counts above and the selected coverage derivation;
- source table/row provenance and complete multi-source provenance lists;
- safe physical path digests, observed SHA-256 values where available, sizes,
  extensions, and artifact-kind/lineage counters;
- the policy that OCR must bind to one exact revision and LLM-derived output
  must bind to one exact processing run and revision before formal materialization.

Preserved only as evidence, and not verified as formal target facts:

- OCR text/content correctness and semantic correspondence to a revision;
- structured/LLM output correctness, prompt/model semantics, or business-field
  correctness;
- a formal target ProcessingRun, ProcessingArtifact, or Evidence binding,
  because no selected record was eligible for materialization;
- the adversarial OCR revision-mismatch and LLM processing-lineage-mismatch
  cases are invariant fixtures. They prove manual-review behavior, not that a
  real source mismatch was found or corrected.

## 7. Clean replay and interrupted recovery

### Clean replay

Stage 2 froze and replayed the same v9 input with:

```text
selected=120 exact_eligible=0 exact_materialized=0 review=1 quarantine=119
mapping_plan_sha256=f5aae49415500877e0e9db753cbfea9567493b611c3d7eac62ba317f05f021e0
replayed=true
```

Stage 3 created fresh `replay-a` and `replay-b` targets. Each target was run
once as `frozen` and once as `replayed`; both mapping hashes, mapping file
hashes, counts, target semantic snapshots, object counts, and integrity
results were equal. The Stage 3 audit reports `replay_equal=true` and
`failures=[]`.

### Interrupted recovery

The Stage 4 `interrupted-recovery` target was first frozen, then its generated
Stage 2 audit was removed to model interruption after durable target work. The
rerun returned `last_status=replayed_recovered` and `replay_count=1`. Before
and after recovery it retained:

- 120 mapping rows;
- 119 quarantine rows and 1 manual-review row;
- the same mapping canonical and file-bytes hashes;
- zero materialized rows and zero formal facts;
- zero object files/bytes and the empty object digest;
- `integrity_check=ok`, `quick_check=ok`, zero foreign-key violations, and zero
  duplicate keys/facts/objects.

This proves recovery of the isolated rehearsal state under the exercised
interruption, not recovery behavior for a production migration that has not
been designed or authorized.

## 8. Integrity matrix and partial-target evidence

Stage 4’s real audit contains 13 cases. Every result has
`invariant_proven=true` and `auto_materialize=false`.

| Case | Disposition | Safe result code | Evidence kind |
| --- | --- | --- | --- |
| Stale version | fail-closed | `stale_version_fail_closed` | Adversarial invariant fixture |
| Conflicting relation | quarantine | `conflicting_relation_quarantine` | Adversarial invariant fixture |
| Missing object | fail-closed | `missing_object_fail_closed` | Adversarial invariant fixture |
| Wrong SHA-256 | fail-closed | `wrong_sha256_fail_closed` | Adversarial invariant fixture |
| Corrupted object | fail-closed | `corrupted_object_fail_closed` | Adversarial invariant fixture |
| Duplicate replay | fail-closed | `duplicate_replay_fail_closed` | Real target invariant |
| Interrupted rehearsal | fail-closed | `interrupted_rehearsal_recovered` | Real target invariant |
| Partially written target | fail-closed | `partial_target_fail_closed` | Real target invariant |
| Duplicate physical content | quarantine | `duplicate_physical_content_quarantine` | Manifest-derived fixture |
| OCR revision mismatch | manual review | `ocr_revision_mismatch_manual_review` | Adversarial invariant fixture |
| LLM processing-lineage mismatch | manual review | `llm_processing_lineage_mismatch_manual_review` | Adversarial invariant fixture |
| Ambiguous reference | quarantine | `ambiguous_reference_quarantine` | Manifest-derived fixture |
| Incorrect old file ID/path | fail-closed | `incorrect_old_file_binding_fail_closed` | Adversarial invariant fixture |

Matrix distribution: **8 fail-closed, 3 quarantine, 2 manual review**. The
synthetic mutation rows are not additional source classifications. In
particular, the read-only boundary prevented mutating the real C source to
create false source facts.

For the real partial-target case, the incomplete mapping artifact had
SHA-256
`88214a42ff84080fd5018ad6130d39514bf8779c454db8823facf71c1ba5dd5c`.
Stage 2 rejected it with `manifest_read_failed`; no target database was
created, and mapping rows, formal facts, object files, and duplicate counts
were all zero.

## 9. Quarantine and human-review boundary

The selected 120 records resolve as follows:

- **119 quarantined:** 89 `Ambiguous` multiple-physical-match records, 29
  `Orphan` records with no source lineage, and 1 `Rejected` record with
  `has_contract_false`;
- **1 manual review:** 1 `Probable` record with `path_and_size_match`;
- **0 formal business facts:** no Document, DocumentRevision, DocumentLink,
  ProcessingRun, ProcessingArtifact, Evidence, audit, or outbox row was
  created;
- **0 Missing and 0 Conflict:** these are real source counts, not omitted
  cases that may be inferred.

The full source census still contains 6 Probable, 644 Ambiguous, 208 Orphan,
and 634 Rejected records. A production migration cannot treat the 120-row
rehearsal as approval to auto-resolve those records.

Human review must determine, with source evidence and an explicit decision
record:

1. whether each probable or ambiguous physical match is the intended object;
2. whether a version/attachment belongs to one exact DocumentRevision;
3. whether OCR is tied to that exact revision;
4. whether structured/LLM output is tied to one exact ProcessingRun and
   revision;
5. whether orphan and rejected records are excluded, repaired in the source
   under a separately authorized process, or retained permanently as
   quarantine evidence.

No metadata-only contract or orphan object may be upgraded to a formal fact by
this rehearsal.

## 10. Production migration risks, rollback, and next plan

### Risks remaining before any production design

- There are zero `Exact` records in both the selected sample and the full
  census, so automatic target materialization was not exercised on a real
  eligible record.
- The selected set is dominated by 89 ambiguous records and 29 orphans, and
  the full source has 644 ambiguous, 208 orphan, and 634 rejected records.
  Manual review volume and policy are unresolved.
- The source snapshot retains 32 foreign-key violations and source
  database/filesystem count differences. Rehearsal evidence does not repair
  or explain those anomalies.
- Stage 0/1 record a dirty source working-tree baseline but no independent
  cryptographic source pre/post snapshot. A controlled migration would need
  that evidence before and after every source-affecting operation.
- OCR and structured/LLM lineage counts are real and preserved, but formal
  target revision/run bindings and semantic output correctness were not
  established because no Exact input was materialized.
- Final workspace, architecture, security, secret, vulnerability, license,
  and image gates after Stage 4 have not been run; the Stage 5 independent
  Reviewer has not yet reviewed this candidate.

### Rollback strategy

This rehearsal has no source rollback because it permits no source write. Its
rollback/cleanup boundary is target-only:

1. stop the rehearsal and retain the frozen manifest, audit hashes, quarantine
   records, and review decisions;
2. archive the evidence outside the active target if required by the eventual
   runbook;
3. remove or move only the exact isolated target directory after evidence is
   secured; and
4. use a fresh, path-validated isolated target for another replay.

Any future production design must add an explicit prepare/preview/confirm
cutover, immutable source backup or restore point, pre/post source snapshot,
versioned target staging, idempotency key, and an auditable compensating or
restore procedure. It must not assume that a rehearsal target can roll back a
source mutation. Ambiguous, orphan, rejected, OCR-mismatch, and LLM-lineage
cases must remain quarantined until a human decision is bound to the source
record, target version, and evidence hash.

### Next-plan recommendation

Keep PLAN-0009 in rehearsal-only status. The next separately approved plan
should be a controlled migration design and manual-reconciliation phase. It
should first establish a reviewed real `Exact` fixture or an explicitly
approved path for handling the absence of Exact records, complete human
review/quarantine policy for the full census, source pre/post snapshot
controls, and a production rollback/runbook. Only after those decisions and
the final architecture/security/workspace gates pass should production
migration authorization be considered.

## 11. Gate status and review ledger

The following is the evidence status, with earlier reports reconciled to the
real artifacts:

| Stage / gate | Evidence status |
| --- | --- |
| Stage 0 activation and isolation | Focused/real boundary, source census, three import paths, lineage, and guard evidence recorded; rehearsal-only activation |
| Stage 1 inventory | Real v9 manifest/replay evidence; focused checks and independent Reviewer `PASS` recorded in the Stage 1 report |
| Stage 2 mapping | Real v9 mapping/replay evidence; focused checks and independent Reviewer `PASS` recorded in the Stage 2 report |
| Stage 3 dual replay | Real two-target replay and semantic comparison; focused checks, prior workspace checks, and independent Reviewer `PASS` recorded in the Stage 3 report |
| Stage 4 integrity/recovery | Real clean/recovery/partial-target evidence; format, package check/test/Clippy and independent Reviewer `PASS` recorded in the Stage 4 report |
| Stage 5 artifact/hash/report checks | **PASS for the read-only checks performed for this report** |
| Final workspace gates after Stage 4/Stage 5 candidate | **PASS** — fmt check; workspace check; workspace Clippy with -D warnings; workspace tests 150 passed, 34 ignored |
| Architecture fitness check | **PASS** — Cargo metadata, OpenAPI contract, and architecture fitness |
| Secret/vulnerability/license/image scans | **NOT RUN** — no repository-provided or installed cargo-audit/cargo-deny/gitleaks/trivy/syft/grype/osv-scanner entrypoint was available |
| Stage 5 independent Reviewer | **PENDING** |
| Production migration authorization | **NOT GRANTED** |

The final Rust gates were rerun against this Stage 5 candidate with the
temporary G:\codex-build\business-platform-target Cargo target directory and
incremental compilation disabled because the repository volume had exhausted
free space during the initial cold build. This changes only generated build
output. The architecture check was run with scripts/check-architecture.ps1 and
returned PASS. The prior Stage 3 report’s workspace checks were not used as a
substitute for these final gates.

Review ledger:

- Stage 5 base is `5de6a995b874c93bdc97486391aed8c2d5920462`.
- The candidate is 97336d220d87cdbd730f35950ce60b723707aa2f.
- Stage 5 independent Reviewer remains **PENDING** and must review the exact
  candidate range.
- No source C file, `D:\contract_data_test` file, production system, or
  unrelated repository area is owned by this report.

## 12. Completion limitations

This report is complete only as a Stage 5 rehearsal verification record. It
does not close the final workspace or architecture gates, does not replace
the independent Stage 5 Reviewer, does not prove source-side immutability by
cryptographic pre/post snapshot, does not prove production-scale throughput,
and does not authorize production migration. The zero-Exact/zero-conflict/
zero-missing source distribution is reported as observed; it is not a reason
to invent positive or negative cases beyond the manifest and the explicitly
identified invariant fixtures.
