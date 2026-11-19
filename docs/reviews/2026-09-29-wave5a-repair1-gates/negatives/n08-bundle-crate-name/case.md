# n08-bundle-crate-name

case       : release-bundle.json binds a crate the payload does not ship
expect     : exit 1 refused by `packed-crate-bindings`
observed   : exit 1 refused by `packed-crate-bindings`
verdict    : OK

## what the injection actually changed (content hashes, pristine copy -> damaged copy)

changed   `release-bundle.json` (`cb4387e4a56e…` -> `8c29c3402acc…`)

## commands

1. `module-release verify --artifact <copy>` before the injection — exit 0
   (the copy has to be a candidate that verifies, or the case proves nothing)
2. `module-release verify --artifact <copy>` after the injection — exit 1

Raw logs: `00-baseline-verify.log`, `10-verify.log` (archived beside this file).
