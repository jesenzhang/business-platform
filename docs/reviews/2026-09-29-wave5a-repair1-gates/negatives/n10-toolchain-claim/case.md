# n10-toolchain-claim

case       : the identity declares a toolchain rust-toolchain.toml at that commit does not
expect     : exit 1 refused by `toolchain-declaration`
observed   : exit 1 refused by `toolchain-declaration`
verdict    : OK

## what the injection actually changed (content hashes, pristine copy -> damaged copy)

changed   `release-bundle.json` (`cb4387e4a56e…` -> `f123038bc15c…`)
changed   `release.json` (`1bf1ff9690d7…` -> `0824f3e8c16b…`)

## commands

1. `module-release verify --artifact <copy>` before the injection — exit 0
   (the copy has to be a candidate that verifies, or the case proves nothing)
2. `module-release verify --artifact <copy>` after the injection — exit 1

Raw logs: `00-baseline-verify.log`, `10-verify.log` (archived beside this file).
