# Audit Retention and Tamper Evidence

> Status: Baseline profile for PLAN-0005

Hot PostgreSQL audit data is retained for the configured operational window
(default 90 days in non-production examples; production approval owns the
actual value). Older records are exported as newline-delimited canonical JSON
with checksum and tenant metadata to a private, tenant-scoped artifact prefix.
The export manifest, legal-hold marker, and restore verification are retained
with the archive. Deletion is only through an approved retention job and never
through business APIs; Legal Hold blocks deletion.

The per-tenant hash chain is tamper evidence. It detects row mutation, removal,
or reordering when verified from a trusted checkpoint, but is not presented as
absolute immutability. Production may add Object Lock/WORM and signed archive
manifests through a later ADR. API responses expose hashes and bounded
metadata, never secrets, storage keys, signed URLs, raw text, prompts, or
credentials.
