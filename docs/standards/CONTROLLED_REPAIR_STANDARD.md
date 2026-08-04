# Controlled Repair Standard

Repair is a typed, allow-listed application operation, not a migration and
not arbitrary SQL. A `RepairHandler` owns a stable descriptor and implements
dry-run, execute, and verify with a typed command. The handler belongs to the
data-owning bounded context; Governance owns only orchestration state.

Low-risk repairs may be explicitly enabled for automation. Medium/high risk
repairs require a separate approver from the creator; critical and destructive
business operations are never automatic. Execution revalidates the Finding,
rule, target version, tenant, and approval, claims a lease/fence, and commits
owner state, Audit, Outbox, ledger, and Finding transition atomically. A
durable Repair Run supports retry, cancellation, checkpoint, resume, and
stale-worker rejection. APIs never expose SQL or generic JSON patch input.
