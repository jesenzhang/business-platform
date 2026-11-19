# n01-payload-byte

case       : one packed source byte changed: identity must fail the integrity digest
expect     : exit 1 refused by `artifact-integrity-digest`
observed   : exit 1 refused by `artifact-integrity-digest`
verdict    : OK

## what the injection actually changed (content hashes, pristine copy -> damaged copy)

changed   `package/contract/packages/identity/src/lib.rs` (`d700172c8f91…` -> `036ed71748ce…`)

## commands

1. `module-release verify --artifact <copy>` before the injection — exit 0
   (the copy has to be a candidate that verifies, or the case proves nothing)
2. `module-release verify --artifact <copy>` after the injection — exit 1

Raw logs: `00-baseline-verify.log`, `10-verify.log` (archived beside this file).

