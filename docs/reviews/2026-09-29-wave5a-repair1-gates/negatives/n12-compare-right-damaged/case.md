# n12-compare-right-damaged

case       : a damaged payload on the right side must be refused, not reported as a build difference
expect     : exit 1 refused by `compare-payload-integrity`
observed   : exit 1 refused by `compare-payload-integrity`
verdict    : OK

## what the injection actually changed (content hashes, pristine copy -> damaged copy)

changed   `package/contract/packages/identity/src/lib.rs` (`d700172c8f91…` -> `05b885b866be…`)

## commands

1. `module-release compare --left <pristine> --right <copy>` before the injection — exit 0 expected
2. `module-release compare --left <pristine> --right <copy>` after the injection — exit 1

Raw logs: `00-compare-clean.log`, `10-compare-damaged.log` (archived beside this file).
