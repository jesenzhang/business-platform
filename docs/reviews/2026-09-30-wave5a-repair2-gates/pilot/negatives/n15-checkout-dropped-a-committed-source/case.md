# n15-checkout-dropped-a-committed-source

case       : a checkout missing a committed source file must be refused, with no artifact created
expect     : exit 1
, refused by `source-member-set-matches-revision`, and no artifact directory
observed   : exit 1, check `source-member-set-matches-revision`, artifact directory created: False
verdict    : OK

the checkout is `F:\Workspace\business-platform\target\wave5a-repair2\pilot\run-20260930T032942Z\isolated-checkout` — a directory whose `.git` file names this repository's
object store, holding only the files a candidate is built from.

what this case did to it : removed 'crates/identity/src/application/access.rs', which the pinned commit contains

Raw log: `10-generate.log` (archived beside this file).

