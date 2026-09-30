# g01-absolute-into-repo

case       : a resolved path dependency pointing at the repository crate with forward slashes
expect     : exit 1 refused by `consumer-graph-isolation`
observed   : exit 1 refused by `consumer-graph-isolation`
verdict    : OK

The leak is injected into a real `cargo metadata` document taken from the
passing consumer run, so the guard sees exactly the shape a real build would
produce. The doctored document itself stays in the run directory (it embeds
absolute paths of this machine); its sha256 pins what was judged:

document   : `metadata.json` sha256 `bc2e605e2b6cc69172e284c4ac9fbf3d2ed9c36e196e7ef4aab7e2b8696d0a71`
consumer   : `F:\Workspace\business-platform\target\wave5a-repair2\pilot\run-20260930T032942Z\consumers\identity`
repo root  : `F:\Workspace\business-platform`
artifact   : `F:\Workspace\business-platform\target\wave5a-repair2\pilot\run-20260930T032942Z\run-a\identity-0.1.0-rc.1\package\contract\packages\identity`

Raw log: `10-guard.log` (archived beside this file).

