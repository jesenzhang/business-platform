# PLAN-0009 Stage 3 — real 120-contract dual replay

Status: **candidate — independent Reviewer pending**.

Stage 3 is a rehearsal-only execution and comparison stage. It does not
authorize production migration and it does not activate PLAN-0006.

## Boundary and composition

Stage 3 consumes the accepted Stage 1 v9 manifest and composes the accepted
Stage 2 mapping engine through its bounded target-directory seam. It does not
duplicate or weaken the Stage 2 Exact/Probable/quarantine policy.

The source tuple is the user-provided C local-test env, database, and physical
roots. Source access remains read-only and all writes are inside the isolated
Stage 3 target:

`F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810\stage-3-rehearsal-v1`

The two replay targets are `replay-a` and `replay-b`. Each target is created
fresh, run once to freeze, and run a second time against its own frozen output
to exercise the replay path. No manual edit was made to any target or source
artifact.

## Input and real sample

- input schema: `plan-0009.stage-1.inventory.v9`
- input manifest SHA-256: `8376eac8c5aa2447077048f3a50d68c3584e3df929d3473a865f995f5319cb43`
- selected contracts: `120`
- source census: `Exact=0, Probable=6, Ambiguous=644, Conflict=0, Orphan=208, Missing=0, Rejected=634`
- sample classification: `Exact=0, Probable=1, Ambiguous=89, Conflict=0, Orphan=29, Missing=0, Rejected=1`

The coverage matrix is derived from the frozen manifest, not hand-entered
test data:

| Scenario | Count | Derivation / result |
| --- | ---: | --- |
| ordinary single file | 0 | no selected record has only an ordinary unique file lineage |
| multi-version | 2 | `versions > 1` |
| scanned / OCR | 1 | `ocr_artifacts > 0` |
| attachments | 10 | `attachments > 0` |
| LLM / structured | 5 | structured JSON artifact kind present |
| known bad relationship | 1 | rejected source relationship |
| duplicate / multiple physical matches | 89 | `Ambiguous` classification |
| missing or non-usable evidence | 90 | `Ambiguous` plus `Rejected`; source `Missing` classification itself is 0 |
| orphan | 29 | `Orphan` classification |
| ambiguous | 89 | `Ambiguous` classification |
| conflict | 0 | source tuple contains no conflict case |

Unavailable ordinary-single-file and conflict cases are recorded as zero; the
rehearsal does not fabricate coverage. The non-usable-evidence count is
explicitly distinguished from the source classification `Missing` count.

The selected lineage totals are: 22,121 physical evidence entries, 21,570
multi-source evidence entries, 92 versions, 13 attachments, 217 artifacts, 91
ingestions, 6 ingestion tasks, 36 task files, 6 parse jobs, 6 extraction
results, 1 OCR artifact lineage, 22 structured-artifact lineages, and 785
legacy fingerprint observations.

## Real dual replay evidence

The CLI result was:

```text
stage=3 status=replayed selected=120 replay_equal=true quarantine=119 object_files=0 object_bytes=0 input_manifest_sha256=8376eac8c5aa2447077048f3a50d68c3584e3df929d3473a865f995f5319cb43
```

Both `replay-a` and `replay-b` had the following sequence:

```text
first:  status=frozen   selected=120 exact_eligible=0 exact_materialized=0 review=1 quarantine=119
second: status=replayed selected=120 exact_eligible=0 exact_materialized=0 review=1 quarantine=119
```

Both replay targets produced the same Stage 2 mapping SHA-256
`f5aae49415500877e0e9db753cbfea9567493b611c3d7eac62ba317f05f021e0` and
mapping file-bytes SHA-256
`e00a2769b53e68212a98cdeb4939a0ca668f737be909844568b73c9f7b220e59`.
Each Stage 2 replay audit records `replay_count=1` and
`last_status=replayed`.

Semantic target comparison passed for both targets:

- mapping ledger rows: `120`
- materialized mapping rows: `0`
- duplicate mapping keys: `0`
- formal documents, revisions, links, processing runs, processing artifacts,
  processing evidence, audit events, and outbox events: all `0`
- SQLite `integrity_check=ok`, `quick_check=ok`, foreign-key violations: `0`
- object files / bytes: `0 / 0`
- empty object-root digest: `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`

The zero formal-fact result is caused by the real sample having no Exact
record; candidate IDs in the mapping plan remain proposals and are not
business facts. The Stage 3 audit records `failures=[]` and
`replay_equal=true`.

Authoritative Stage 3 audit artifact:

`stage3-replay-audit-v1.json`

Audit artifact SHA-256:
`fb312b732b9879df1471dab22fda9b22848fdeb44b8cd269102c14f07f16fd6c`

It contains only manifest/classification/lineage counts, safe SHA-256 values,
target semantic snapshots, replay status, and failure codes. It contains no
raw source names, text, absolute source paths, URLs, credentials, or signed
URLs.

## Verification gates

Executed:

```text
rtk cargo fmt --all -- --check
rtk cargo check --workspace --all-targets --all-features
rtk cargo clippy --workspace --all-targets --all-features -- -D warnings
rtk cargo test --workspace --all-features
rtk cargo test -p plan-0009-rehearsal --all-features   # 10 passed
```

Results:

- format check: pass;
- workspace check: pass (`377` crates compiled);
- workspace Clippy: pass (`No issues found`);
- workspace tests: `147 passed, 34 ignored` across `113` suites;
- focused Stage 3 tests: `10 passed` across `3` suites.

The workspace check, Clippy, and test commands used the temporary
`G:\codex-build\business-platform-target` Cargo target directory with
incremental compilation disabled because the repository volume had exhausted
its free space during the first full test attempt. This changes only generated
build output, not source or rehearsal data.

## Review ledger

- A new Stage 3 Luna implementation worker was launched with this ownership;
  it stalled before producing a change, so the coordinator completed the
  scoped implementation and verification without changing the accepted
  Stage 0–2 semantics.
- A follow-up Stage 3 Luna verification worker independently checked the real
  audit and target snapshots and returned coordinator evidence; it did not
  replace the required independent Reviewer verdict.
- Coordinator real run: complete; first/replay output and both target snapshots
  match.
- Independent Reviewer: pending. Stage 3 remains open until the Reviewer
  directly verifies this report, the real audit artifact, both target trees,
  coverage derivation, and source-isolation claims and returns `PASS`.
