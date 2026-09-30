# n13-packed-manifest-not-from-the-commit

case       : a packed Cargo.toml the pinned commit cannot produce, with a self-consistent record, must be refused
expect     : exit 1 refused by `derived-members-match-revision`
observed   : exit 1 refused by `derived-members-match-revision`
verdict    : OK

## what the injection actually changed (content hashes, pristine copy -> damaged copy)

changed   `artifact-manifest.json` (`ec177088f8c3…` -> `113e87caad8b…`)
changed   `package/contract/packages/identity/Cargo.toml` (`65c01c4a2a3e…` -> `4b14cd85ce67…`)

## commands

1. `module-release verify --artifact <copy>` before the injection — exit 0
   (the copy has to be a candidate that verifies, or the case proves nothing)
2. `module-release verify --artifact <copy>` after the injection — exit 1

Raw logs: `00-baseline-verify.log`, `10-verify.log` (archived beside this file).

