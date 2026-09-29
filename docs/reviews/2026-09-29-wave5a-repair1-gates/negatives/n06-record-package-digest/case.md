# n06-record-package-digest

case       : artifact-manifest.json pins a package digest release.json does not claim
expect     : exit 1 refused by `record-consistency`
observed   : exit 1 refused by `record-consistency`
verdict    : OK

## what the injection actually changed (content hashes, pristine copy -> damaged copy)

changed   `artifact-manifest.json` (`ec177088f8c3…` -> `9ec6b1d13e6e…`)

## commands

1. `module-release verify --artifact <copy>` before the injection — exit 0
   (the copy has to be a candidate that verifies, or the case proves nothing)
2. `module-release verify --artifact <copy>` after the injection — exit 1

Raw logs: `00-baseline-verify.log`, `10-verify.log` (archived beside this file).
