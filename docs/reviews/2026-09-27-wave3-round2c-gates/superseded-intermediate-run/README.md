# Superseded intermediate run - not candidate evidence

These seven logs were produced before the final candidate tree existed. Two
things changed afterwards, so they cannot be read as evidence for the code
candidate:

1. `with_chain_metadata` gained `occurred_at` normalization, which changes the
   `audit` crate every lane compiles against.
2. One assertion in the new `chain_pg` precision test was itself wrong: it
   compared the stored `recorded_at` against the pre-append event value, but
   `append_postgres` assigns `recorded_at` from the adapter clock while signing
   the row. That test failed for a fixture reason, not a product reason, and was
   corrected. The three pre-existing `chain_pg` tests passed in this run, and the
   other four lanes passed.

They are retained unchanged because they record the database actually used, the
server facts observed on it, and the state of the tree at that moment. The
definitive run against the final tree is logs `03` to `09` in the parent
directory.
