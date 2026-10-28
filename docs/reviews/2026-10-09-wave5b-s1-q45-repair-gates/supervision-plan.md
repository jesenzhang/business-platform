# Q4/Q5 repair supervision plan

Start HEAD: 53ef8b289f2a1bf11b108cb5504d90f21153ebc8.
Human authorization: primary agent fixes Q4/Q5; GPT-6 Luna executes; primary agent owns scheduling, review and decisions.

Scope: offline Wave 5B-S1 test tooling only. No business bounded context, authority owner, Application API/Event, database schema, migration, worker or deployment change. Temporary run trees belong to this run; repository, callers' parents and evidence remain protected. No push/merge/deploy/S2.

Executor: collaboration agent /root/q45_luna, requested model gpt-6-luna, high reasoning. Ownership: scripts/test-plan0014-wave5b.ps1 and scripts/test-plan0014-wave5b-repairs.ps1; one native helper file only if needed and explicitly coordinated. Primary owns independent evidence and closeout.

Approved design:
- No-follow open each directory component from local volume root down, inspect handle attributes and retain the handles.
- Sharing mode must deny both writes and delete/rename, including in-place reparse changes.
- Leaf attributes and deletion operate on the same handle; no path fallback after releasing a pin.
- Only unlink a link leaf after proving/pinning its parent chain.
- Windows local NTFS and exFAT must both retain normal cleanup; unsupported remote/unknown platforms fail closed.
- All handles close on success, sharing conflict, permission error and injected failure.
- A direct file leaf must never bypass ancestor checks.
- Existing evidence, KeepScratch, protected paths and unique RunId isolation remain covered.

Evidence:
1. Save Q4/Q5 failures at original source.
2. Establish actual share-lock and handle-delete behavior on both local filesystems.
3. Add regression for static linked parent, last-check boundary, attribute-write/delete boundary, chain mutation, sharing conflict, read-only file, leaf link and exceptional handle release.
4. Real attempted replacements must be observed as blocked or refused; sentinels retain SHA256 and counts. Normal owned file/tree deletion remains a positive control.
5. Primary reviews final diff and independently probes it.
6. Serial final checks: repair full sweep; real offline 113 driver; fmt/check/clippy/workspace test/architecture. Bind source hashes or fixed local candidate SHA and preserve NOT RUN/BLOCKED accurately.
7. No global Git configuration; avoid previous deep-clone path failure by running final gates at main workspace or process-only longpaths.

Stop after local repair delivery and independent primary verification. R2 remains BLOCKED / NOT CLAIMED.
