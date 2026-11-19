# g03-transitive-into-repo

case       : a transitive local dependency on a repository crate the consumer never names
expect     : exit 1 refused by `consumer-graph-isolation`
observed   : exit 1 refused by `consumer-graph-isolation`
verdict    : OK

The leak is injected into a real `cargo metadata` document taken from the
passing consumer run, so the guard sees exactly the shape a real build would
produce. The doctored document itself stays in the run directory (it embeds
absolute paths of this machine); its sha256 pins what was judged:

document   : `metadata.json` sha256 `43fde8eaf4d9deacb5419c7756ed3661e248e1260ceccb80e4858a72bc432e86`
consumer   : `F:\Workspace\business-platform\target\wave5a-repair2\pilot\run-20260930T032942Z\consumers\identity`
repo root  : `F:\Workspace\business-platform`
artifact   : `F:\Workspace\business-platform\target\wave5a-repair2\pilot\run-20260930T032942Z\run-a\identity-0.1.0-rc.1\package\contract\packages\identity`

Raw log: `10-guard.log` (archived beside this file).

