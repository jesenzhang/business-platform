# Data Integrity Rule Standard

An integrity rule is a deterministic, versioned, bounded-context-owned
detector. Its descriptor declares severity and whether an explicit automatic
repair allow-list entry exists. Rules return bounded fingerprints and safe
expected/detected summaries, never raw documents, secrets, signed URLs, or
SQL. A dependency timeout or object-store outage is an unknown scan result,
not a data-corruption finding.

Findings are deduplicated by tenant/rule/version/resource/fingerprint and use
optimistic lifecycle transitions. A scan run is durable, cancellable, and
auditable. Rule verification is required before and after a repair.
