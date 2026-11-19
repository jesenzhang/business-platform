# g01-absolute-into-repo

case       : a resolved path dependency pointing at the repository crate with forward slashes
expect     : exit 1 refused by `consumer-graph-isolation`
observed   : exit 1 refused by `consumer-graph-isolation`
verdict    : OK

The leak is injected into a real `cargo metadata` document taken from the
passing consumer run, so the guard sees exactly the shape a real build would
produce. The doctored document itself stays in the run directory (it embeds
absolute paths of this machine); its sha256 pins what was judged:

document   : `metadata.json` sha256 `4930ef0099b3aad274be3a24c7f100d0ddab2424a881ff96c99834c88e41db69`
consumer   : `F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\consumers\identity`
repo root  : `F:\Workspace\business-platform`
artifact   : `F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-a\identity-0.1.0-rc.1\package\contract\packages\identity`

Raw log: `10-guard.log` (archived beside this file).
