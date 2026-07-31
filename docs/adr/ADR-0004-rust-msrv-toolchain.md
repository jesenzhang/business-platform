# ADR-0004: Rust MSRV and pinned toolchain

## Status

Accepted

## Context

The workspace declared Rust 1.85 while `rust-toolchain.toml` and CI followed a
moving stable channel. On 2026-07-31, `cargo +1.85.0 check --workspace
--all-targets --all-features` was rejected before compilation. The first
blocking family was the actively maintained AWS SDK:

- `aws-sdk-s3 1.140.0` requires Rust 1.94.1;
- its Smithy runtime dependencies require Rust 1.94.1;
- additional locked dependencies require Rust 1.86 through 1.89.

Downgrading the maintained S3 SDK solely to preserve an unverified declaration
would create unsupported dependency debt.

## Decision

Rust 1.94.1 is the verified minimum toolchain for the current `Cargo.lock`.
The workspace `rust-version`, `rust-toolchain.toml`, and CI all use exactly
1.94.1 with `rustfmt` and `clippy`.

## Consequences

- Builds fail fast when a developer or CI runner lacks Rust 1.94.1.
- Dependency upgrades that raise MSRV require an ADR/status update and exact
  toolchain verification.
- Rust 1.85.0 remains a recorded failed candidate, not a supported MSRV.
