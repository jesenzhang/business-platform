# n14-isolated-checkout-control

case       : a complete isolated checkout of the pinned commit generates a candidate (control for n15 and n16)
expect     : exit 0
 and an artifact directory
observed   : exit 0, check `no check identifier`, artifact directory created: True
verdict    : OK

the checkout is `F:\Workspace\business-platform\target\wave5a-repair2\pilot\run-20260930T032942Z\isolated-checkout` — a directory whose `.git` file names this repository's
object store, holding only the files a candidate is built from.

what this case did to it : nothing — the checkout is exactly the release inputs of commit as it stands (26 files)

Raw log: `10-generate.log` (archived beside this file).

