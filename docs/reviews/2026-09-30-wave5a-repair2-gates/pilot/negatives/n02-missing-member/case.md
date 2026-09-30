# n02-missing-member

case       : a payload member removed: identity must fail the integrity digest
expect     : exit 1 refused by `artifact-integrity-digest`
observed   : exit 1 refused by `artifact-integrity-digest`
verdict    : OK

## what the injection actually changed (content hashes, pristine copy -> damaged copy)

removed   `package/CHANGELOG.md` (was `6672a0f73560…`)

## commands

1. `module-release verify --artifact <copy>` before the injection — exit 0
   (the copy has to be a candidate that verifies, or the case proves nothing)
2. `module-release verify --artifact <copy>` after the injection — exit 1

Raw logs: `00-baseline-verify.log`, `10-verify.log` (archived beside this file).

