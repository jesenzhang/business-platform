# ADR-0025: Provider-Neutral Object Storage and RustFS Evaluation

## Status
Proposed

## Date
2026-09-17

## Context

The platform already isolates S3-compatible storage behind `crates/object-storage` and uses the official `aws-sdk-s3` client selected by ADR-0001. The current validated development/integration backend is MinIO, and existing contract coverage is still named around MinIO (`crates/object-storage/tests/minio_contract.rs`).

RustFS reached stable `1.0.0` on 2026-09-16 and is a viable S3-compatible object-storage candidate. Its own 1.0.0 compatibility matrix explicitly claims broad, not complete, S3 compatibility. Core operations required by this platform are covered upstream, including bucket/object operations, multipart upload, presigned GET/PUT, range reads, metadata, tagging, policies, checksums, selected versioning and object-lock behavior. RustFS also documents remaining gaps and intentional deviations, so product adoption must be proven against this repository's workload rather than inferred from the S3-compatible label.

The business platform stores durable document binaries and processing artifacts that are materially different from database state. This ADR therefore distinguishes the stable object-storage contract from any concrete storage product.

## Decision

1. Keep the business/domain contract provider-neutral. RustFS must not appear in domain models, application service APIs, document/revision identifiers, or persistence schemas.
2. Keep `aws-sdk-s3` as the S3 protocol client unless a separate ADR changes that decision.
3. Treat RustFS as a candidate S3-compatible backend behind the existing object-storage adapter, not as a new storage abstraction.
4. Do not replace MinIO in production, CI, deployment manifests, or default local configuration as part of this ADR. No RustFS implementation is authorized by this documentation-only change.
5. Before any implementation or production switch, generalize the existing backend contract tests so the same repository-owned behavior suite can run against MinIO and RustFS.
6. Backend promotion is based on repository contract evidence, not product branding. A RustFS version may become an approved backend only after the exact version passes the required compatibility and recovery gates below.
7. If Jarvis and business-platform later share one RustFS cluster, they must use separate buckets, service identities/credentials and policies. Prefixes alone are not an acceptable security boundary.

## Storage Boundary

Object storage is appropriate for durable binary/blob payloads such as:

- original contract/document files;
- revision binaries;
- processing artifacts and evidence payloads;
- previews and derived files;
- OCR/layout/model input-output files where the database retains authoritative metadata and lineage.

Object storage is not the authority for relational business state, workflow/job state, audit metadata, authorization data, or database files. Those remain in their existing persistence owners.

## Required RustFS Evaluation Gate

A future implementation task must run one backend-neutral contract suite against both the current MinIO baseline and the candidate RustFS version. At minimum, verify:

- bucket readiness and isolation;
- PUT, GET, HEAD and DELETE;
- ListObjects/ListObjectsV2 behavior actually used by the platform;
- multipart create/upload/complete/abort;
- range and conditional reads used by preview/download flows;
- presigned GET/PUT behavior and expiry semantics;
- content type and user metadata round-trips;
- checksum/ETag assumptions made by this codebase;
- idempotent retries and interrupted upload recovery;
- concurrent uploads/downloads for realistic document sizes;
- service restart with previously committed objects still readable;
- delete/retry behavior and absence reconciliation;
- IAM/policy isolation for tenant/service boundaries that rely on object-storage policy;
- backup/restore or replication procedures selected for production;
- versioning/Object Lock only if a product flow actually depends on those semantics.

Tests must use realistic PDF, DOCX, XLSX, image, OCR-artifact and processing-artifact payloads rather than only tiny synthetic objects.

## Known Upstream Compatibility Boundaries at RustFS 1.0.0

The RustFS 1.0.0 compatibility matrix states that compatibility is broad rather than complete. Not-yet-passing or excluded areas include bucket access logging tests, POST Object form-upload checksum handling, bucket ownership controls, some multipart listing/part-lookup edge cases, IAM-account or multi-storage-class dependent cases, and tenanted bucket-policy edge cases. RustFS also documents intentional object-key and versioned directory-marker deviations from AWS S3 behavior.

These are not current blockers unless the platform depends on them, but future implementation must fail closed if repository behavior relies on an uncovered semantic.

References:

- RustFS repository: https://github.com/rustfs/rustfs
- RustFS 1.0.0 release: https://github.com/rustfs/rustfs/releases/tag/1.0.0
- RustFS 1.0.0 S3 compatibility matrix: https://github.com/rustfs/rustfs/blob/1.0.0/docs/architecture/s3-compatibility-matrix.md
- Existing SDK decision: `docs/adr/ADR-0001-s3-sdk-selection.md`

## Migration Shape

If the evaluation passes, the preferred implementation is configuration-level backend substitution through the existing S3 adapter:

```text
Business domain/application
        |
        v
crates/object-storage
        |
        v
   S3 protocol client
      /       \
   MinIO     RustFS
```

No business-table migration should be required solely to change backend product. Stored object identifiers should remain stable and provider-neutral. Any data-copy procedure between products must be a separate operational migration with integrity verification and rollback evidence.

## Rollback

Rollback must remain possible by restoring the previous S3 endpoint/credentials and, when data migration has occurred, restoring or reconciling the corresponding object set. The object-storage adapter contract must not require RustFS-specific APIs that would prevent returning to MinIO or another conforming S3 backend.

## Consequences

Positive:

- preserves the existing architecture and avoids vendor coupling;
- makes RustFS evaluation low-risk and evidence-driven;
- turns the current MinIO-specific contract test into a reusable storage conformance asset when implementation is authorized;
- allows business-platform and Jarvis to share infrastructure without sharing application authority.

Costs/risks:

- MinIO remains the current validated baseline until an explicit implementation task is completed;
- RustFS 1.0.0 still has documented S3 compatibility boundaries that must be checked against actual workload;
- operating one shared object-storage cluster requires explicit identity, bucket, policy, backup and blast-radius design.

## Deferred Implementation

This ADR intentionally does not:

- add a RustFS dependency;
- change Cargo manifests or Rust code;
- rename or generalize `minio_contract.rs`;
- modify Docker/Compose/Kubernetes deployment;
- change default S3 endpoints or credentials;
- migrate existing objects;
- remove MinIO.

A later implementation plan must cite this ADR and provide exact test evidence before RustFS is enabled anywhere beyond an isolated evaluation environment.
