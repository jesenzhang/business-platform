# PLAN-0009 Stage 4 — adversarial integrity and recovery validation

Status: **candidate — independent Reviewer pending**.

Stage 4 is a rehearsal-only integrity and recovery validation. It does not
authorize production migration and it does not activate PLAN-0006.

## Boundary and inputs

Stage 4 starts from the accepted Stage 3 evidence and the same frozen Stage 1
v9 manifest. The source tuple remains:

- legacy repository: F:\Workspace\git_repo\contract_management;
- local-test environment: F:\Workspace\git_repo\contract_management\backend\.env.local-test;
- source data root: D:\contract_data_test;
- isolated target root:
  F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810\stage-4-integrity-recovery-v1.

The Stage 4 runner uses the reviewed Stage 2 target-directory seam. It writes
only under the isolated target root. The source mutation cases that cannot be
safely performed against the read-only C source are represented as bounded
count/boolean fixtures; they do not mutate or rewrite source facts.

Input identity:

- Stage 1 schema: plan-0009.stage-1.inventory.v9;
- Stage 3 audit SHA-256:
  fb312b732b9879df1471dab22fda9b22848fdeb44b8cd269102c14f07f16fd6c;
- frozen manifest SHA-256:
  8376eac8c5aa2447077048f3a50d68c3584e3df929d3473a865f995f5319cb43;
- selected contracts: 120;
- selected classification:
  Exact=0, Probable=1, Ambiguous=89, Conflict=0, Orphan=29, Missing=0, Rejected=1;
- source census:
  Exact=0, Probable=6, Ambiguous=644, Conflict=0, Orphan=208, Missing=0, Rejected=634.

## Real target recovery evidence

The captured CLI result was:

~~~text
stage=4 status=replayed selected=120 cases=13 fail_closed=8 quarantine=3 manual_review=2 replayed=true manifest_sha256=8376eac8c5aa2447077048f3a50d68c3584e3df929d3473a865f995f5319cb43 mapping_plan_sha256=f5aae49415500877e0e9db753cbfea9567493b611c3d7eac62ba317f05f021e0 matrix_sha256=707e368accbcb6bf99fa1b25222f97515a4fa640875f026c122d3dac1e7e4686 formal_facts=0 object_files=0 duplicate_formal_facts=0 duplicate_objects=0 audit_file_sha256=619002f0b7050ad4d9c3c15bdcc3b063b76d0e79af62c3f03889caa3653e167f
~~~

Authoritative audit artifact:

F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810\stage-4-integrity-recovery-v1\stage4-integrity-recovery-audit-v1.json

Audit SHA-256:
619002f0b7050ad4d9c3c15bdcc3b063b76d0e79af62c3f03889caa3653e167f

### Clean duplicate replay

Under clean-baseline, Stage 2 ran once as frozen and a second time as
replayed. Both runs had 120 mapping rows, 119 quarantined records, one
manual-review record, zero Exact-eligible records, and zero materialized
records. The mapping plan SHA-256 was
f5aae49415500877e0e9db753cbfea9567493b611c3d7eac62ba317f05f021e0 and the
mapping file-bytes SHA-256 was
e00a2769b53e68212a98cdeb4939a0ca668f737be909844568b73c9f7b220e59.

Both snapshots returned integrity_check=ok, quick_check=ok, zero foreign key
violations, zero duplicate mapping keys, zero formal rows, zero formal
duplicates, zero object files, zero object bytes, zero duplicate objects, and
the empty object digest
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855.

### Interrupted rehearsal recovery

Under interrupted-recovery, the first run was frozen. Only the generated
Stage 2 rehearsal audit was removed from the isolated target to model an
interruption after durable mapping/target writes. The clean rerun returned
replayed=true with last_status=replayed_recovered and replay_count=1.
Its mapping hashes, 120 rows, 119 quarantine rows, one manual-review row,
formal-row counts, object counts, integrity checks, and duplicate checks were
identical to the first snapshot.

### Partially written target

Under partial-target, a deterministic incomplete mapping artifact was written
to the isolated target before invoking Stage 2. Its artifact SHA-256 was
88214a42ff84080fd5018ad6130d39514bf8779c454db8823facf71c1ba5dd5c.
Stage 2 rejected it with the safe code manifest_read_failed. No target
database was created, and mapping rows, formal facts, objects, and duplicates
were all zero.

## Adversarial integrity matrix

The matrix is produced by the tested validate_integrity_cases function, not
by report-only constants. Every result has invariant_proven=true and
auto_materialize=false.

| Case | Disposition | Safe result code | Evidence source |
| --- | --- | --- | --- |
| stale version | fail-closed | stale_version_fail_closed | adversarial mutation |
| conflicting relation | quarantine | conflicting_relation_quarantine | adversarial mutation |
| missing object | fail-closed | missing_object_fail_closed | adversarial mutation |
| wrong SHA-256 | fail-closed | wrong_sha256_fail_closed | adversarial mutation |
| corrupted object | fail-closed | corrupted_object_fail_closed | adversarial mutation |
| duplicate replay | fail-closed | duplicate_replay_fail_closed | real target invariant |
| interrupted rehearsal | fail-closed | interrupted_rehearsal_recovered | real target invariant |
| partially written target | fail-closed | partial_target_fail_closed | real target invariant |
| duplicate physical content | quarantine | duplicate_physical_content_quarantine | manifest-derived mutation |
| OCR revision mismatch | manual review | ocr_revision_mismatch_manual_review | adversarial mutation |
| LLM processing-lineage mismatch | manual review | llm_processing_lineage_mismatch_manual_review | adversarial mutation |
| ambiguous reference | quarantine | ambiguous_reference_quarantine | manifest-derived mutation |
| incorrect old file ID/path | fail-closed | incorrect_old_file_binding_fail_closed | adversarial mutation |

The matrix distribution is 8 fail-closed, 3 quarantine, and 2 manual review.
The duplicate-content and ambiguous-reference fixtures derive their positive
counts from the real selected Ambiguous=89 records. OCR/LLM mismatch,
stale-version, relation, checksum, object, and old-reference mutations are
synthetic invariant checks because mutating their C inputs would break the
mandatory read-only boundary. They are not claimed as additional source
classifications.

## Verification

Executed against the Stage 4 candidate:

~~~text
rtk cargo fmt --all -- --check
rtk cargo check -p plan-0009-rehearsal --all-targets --all-features
rtk cargo test -p plan-0009-rehearsal --all-features
rtk cargo clippy -p plan-0009-rehearsal --all-targets --all-features -- -D warnings
~~~

Results:

- format check: pass;
- package check: pass (67 crates compiled);
- focused tests: 13 passed across 3 suites;
- package Clippy: pass (No issues found);
- full workspace gates after Stage 4 changes: pending final Stage 5 gate run.

No command wrote, staged, committed, uploaded, or executed anything in the
C repository or D:\contract_data_test. The Stage 4 report and audit expose
only safe classifications, stable codes, counts, hashes, and target semantic
snapshots.

## Review ledger

- Stage 4 base: 3c09837 (Stage 3 accepted candidate).
- Stage 4 Luna implementation sessions were interrupted before producing a
  committed candidate; the coordinator completed and verified the bounded
  module and report from the preserved Stage 4 working-tree state.
- Coordinator candidate: 005024d.
- Independent Reviewer: pending. Stage 4 remains open until a new Reviewer
  directly verifies the audit artifact, clean replay, interrupted recovery,
  partial-target rejection, matrix derivation, source isolation, and focused
  tests, then returns PASS.
