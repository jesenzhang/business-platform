# n11-stale-revision

case       : both identity records claim a real commit that does not contain the packed inputs
expect     : exit 1 refused by `source-inputs-match-revision`
observed   : exit 1 refused by `source-inputs-match-revision`
verdict    : OK

## what the injection actually changed (content hashes, pristine copy -> damaged copy)

changed   `release-bundle.json` (`6f3b82650714…` -> `1575e7069838…`)
changed   `release.json` (`0f676edcd898…` -> `a1ce2f135410…`)

## commands

1. `module-release verify --artifact <copy>` before the injection — exit 0
   (the copy has to be a candidate that verifies, or the case proves nothing)
2. `module-release verify --artifact <copy>` after the injection — exit 1

Raw logs: `00-baseline-verify.log`, `10-verify.log` (archived beside this file).

