| F1 | both identity records claim 0000…0000 as source_revision | `module-release verify (pre-fix)` | exit 0 | RED CONFIRMED (verified) |
| F2 | one packed source byte changed on the right side only | `module-release compare (pre-fix)` | exit 0 | RED CONFIRMED (declared identical) |
| F2 | the same damage, through verify | `module-release verify (pre-fix)` | exit 1 | control: verify caught it, compare did not |
| F3 | artifact_integrity_algorithm replaced by an algorithm nobody implements | `module-release verify (pre-fix)` | exit 0 | RED CONFIRMED (verified) |
| F3 | artifact-manifest.json claims a module_version release.json does not | `module-release verify (pre-fix)` | exit 0 | RED CONFIRMED (verified) |
| F3 | release-bundle.json binds crate version 9.9.9 while the packed Cargo.toml says 0.1.0-rc.1 | `module-release verify (pre-fix)` | exit 0 | RED CONFIRMED (verified) |
