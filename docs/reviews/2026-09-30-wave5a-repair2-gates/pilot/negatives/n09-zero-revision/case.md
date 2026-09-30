# n09-zero-revision

case       : both identity records claim a revision that is not a commit
expect     : exit 1 refused by `source-revision-exists`
observed   : exit 1 refused by `source-revision-exists`
verdict    : OK

## what the injection actually changed (content hashes, pristine copy -> damaged copy)

changed   `release-bundle.json` (`6f3b82650714…` -> `807c5829efec…`)
changed   `release.json` (`0f676edcd898…` -> `5678019752b1…`)

## commands

1. `module-release verify --artifact <copy>` before the injection — exit 0
   (the copy has to be a candidate that verifies, or the case proves nothing)
2. `module-release verify --artifact <copy>` after the injection — exit 1

Raw logs: `00-baseline-verify.log`, `10-verify.log` (archived beside this file).

