# Changelog — `identity` module releases

All notable changes to the **release layer** of the `identity` module are
documented here. The packed crate sources inside this artifact are the module's
sources at the pinned `source_revision`; this file describes what a release of
this module means, not what the module's code does.

## 0.1.0-rc.1 — local release candidate (PLAN-0014 Wave 5A), NOT PUBLISHED

First independent release candidate of the `identity` module. It introduces **no
change to module behaviour**: it is the first time the existing module is
declared as a `BusinessModuleManifest`, versioned independently, packed and
digested as a candidate.

### Added (release layer only)

- Independent module release version `0.1.0-rc.1`, deliberately separate from the
  workspace product version `0.1.0`
  (`docs/standards/SAAS_MODULE_STANDARD.md` §8). The pre-release tag states that
  this is a candidate and not a general-availability release.
- First `BusinessModuleManifest` declaration for this module
  (schema `business-module.manifest.v1`): owned bounded context
  `identity-management`, 5 published commands, 4 published queries, no published
  events, no published resource kinds, `confidential` data classification,
  migration namespace `identity-management`, no module dependencies, platform
  compatibility minimum `0.1.0` and no maximum. Every field and its authority is
  recorded in `tools/module-release-tool/specs/identity.json`.
- Pinned release identity (`module-release.identity.v1`): module id and version,
  manifest schema version, source revision, contract digest, compiled-declaration
  package digest, migration set digest and toolchain pin, plus the module-version
  to crate-version bundle mapping.
- Read-only migration mapping for the two catalogs that carry this module's
  schema: the module-owned SQLite catalog and the shared workspace-level
  PostgreSQL catalog, each entry pinned to its formal `MANIFEST.sha256` checksum.

### Changed

- Nothing in the module's public Rust API, domain rules, application use cases or
  ports. The packed sources are the files under `crates/identity/src` unchanged;
  only `Cargo.toml` is rewritten for independent use (workspace inheritance
  materialised, third-party dependencies pinned from `Cargo.lock`,
  `[dev-dependencies]` and `[lints]` removed, empty `[workspace]` table appended).
  Every rewrite is listed in `artifact-manifest.json`.

### Compatibility

- No released contract artifact exists before this candidate, so there is no
  upgrade path from an earlier release version. `0.1.0-rc.1` is the first
  candidate consumers may build against.
- SemVer note, verified by test: `VersionReq` `^0.1.0` does **not** match
  `0.1.0-rc.1`. A consumer of a pre-release candidate must pin it explicitly.

### Known limits carried by this candidate (why this is not R2)

- `PLAN-0014` Wave 1 G-01 (no independent IAM module release contract accepted)
  and G-02 (no fresh real-PostgreSQL evidence) remain open.
- `PLAN-0014` Wave 2 G-201, G-202 and G-203 (C1) remain open; this candidate is
  evidence for them, not their closure.
- PostgreSQL: this module's tables live in the shared
  `migrations/019_identity_authorization_foundation.sql` together with
  organization and policy in one `public` schema and are embedded wholesale by
  `runtime-migration`, so installing or upgrading this module alone on
  PostgreSQL is not possible and is recorded as `BLOCKED` in
  `package/migrations/migration-mapping.json`.
- No migration was executed while producing this artifact, and nothing here was
  published, registered, installed or deployed.
