# n16-checkout-added-an-uncommitted-source

case       : a checkout with a source file the commit does not contain must be refused, with no artifact created
expect     : exit 1
, refused by `source-member-set-matches-revision`, and no artifact directory
observed   : exit 1, check `source-member-set-matches-revision`, artifact directory created: False
verdict    : OK

the checkout is `F:\Workspace\business-platform\target\wave5a-repair2\pilot\run-20260930T032942Z\isolated-checkout` — a directory whose `.git` file names this repository's
object store, holding only the files a candidate is built from.

what this case did to it : restored 'crates/identity/src/application/access.rs' from the repository and added the never-committed 'plan0014_repair_case_extra.rs'

Raw log: `10-generate.log` (archived beside this file).

