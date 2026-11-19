# g02-relative-into-repo

case       : a resolved path dependency spelled relative to the consumer ('..\..\..\..\..\crates\identity\Cargo.toml')
expect     : exit 1 refused by `consumer-graph-isolation`
observed   : exit 1 refused by `consumer-graph-isolation`
verdict    : OK

The leak is injected into a real `cargo metadata` document taken from the
passing consumer run, so the guard sees exactly the shape a real build would
produce. The doctored document itself stays in the run directory (it embeds
absolute paths of this machine); its sha256 pins what was judged:

document   : `metadata.json` sha256 `6adb5a1b8aa6c313b4d97be97911e5d8c9cf12199679fd0ae36183d470a11aac`
consumer   : `F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\consumers\identity`
repo root  : `F:\Workspace\business-platform`
artifact   : `F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-a\identity-0.1.0-rc.1\package\contract\packages\identity`

Raw log: `10-guard.log` (archived beside this file).
