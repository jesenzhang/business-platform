# n03-unknown-algorithm

case       : artifact-manifest.json names an integrity algorithm nobody implements
expect     : exit 1 refused by `artifact-record-format`
observed   : exit 1 refused by `artifact-record-format`
verdict    : OK

## what the injection actually changed (content hashes, pristine copy -> damaged copy)

changed   `artifact-manifest.json` (`ec177088f8c3…` -> `4f26de735ac6…`)

## commands

1. `module-release verify --artifact <copy>` before the injection — exit 0
   (the copy has to be a candidate that verifies, or the case proves nothing)
2. `module-release verify --artifact <copy>` after the injection — exit 1

Raw logs: `00-baseline-verify.log`, `10-verify.log` (archived beside this file).

