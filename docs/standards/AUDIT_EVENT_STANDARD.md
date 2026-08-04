# Audit Event Standard

Audit events are append-only business evidence, distinct from logs, domain
events, execution events, findings, and repair ledger entries. Every event is
tenant scoped, validated at construction, carries actor/action/resource,
operation and trace correlation, a stable result, schema version, and
redacted details. Failure codes are allowed only for Failed or Denied.

Canonical hashing uses a deterministic field order and JSON representation;
the hash input includes the previous tenant-stream hash and excludes mutable
database metadata. The public query contract uses an opaque versioned keyset
cursor ordered by occurred time descending and id descending. Queries apply
tenant and management authorization before filtering.

Owner adapters append Audit and Outbox in the same local transaction as the
business write. No API, worker, or repair path may issue an arbitrary audit
UPDATE/DELETE or construct a SQL fragment from user input.
