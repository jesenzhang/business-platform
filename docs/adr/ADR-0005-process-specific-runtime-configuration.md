# ADR-0005: Process-Specific Runtime Configuration

> Status: Accepted
> Date: 2026-07-31
> Related plan: PLAN-0002

## Decision

Runtime configuration belongs to each application composition root. Every
`apps/*` process owns a narrow configuration root and loads only the physical
configuration sections it consumes. `shared-kernel` contains no infrastructure
topology, configuration loader, or secret implementation.

`crates/runtime-config` is limited to stable runtime support: environment
selection, configuration loading, redacted `Secret<T>`, and `SecretUrl`. It
does not define a platform-wide configuration root and does not depend on
clients, SQLx, Axum, or domain types.

Connection URLs are represented as `SecretUrl`. Plaintext is available only
through `expose()` at the client-construction boundary. `Display`, `Debug`,
parse errors, and configuration validation errors must not reveal userinfo,
passwords, or sensitive query parameter values.

Environment-variable prefixes are process-specific: `BUSINESS_API__`,
`BUSINESS_WORKER__`, `AI_WORKER__`, `AGENT_ADAPTER__`, and `MIGRATION__`.

`DATABASE_URL` remains a deprecated Migration CLI compatibility input. A
simultaneous `MIGRATION__DATABASE__URL` and `DATABASE_URL` is rejected rather
than silently selecting one source.

## Consequences

The API no longer requires storage or messaging configuration. Placeholder
workers load only observability settings, and the Migration CLI only loads a
database URL. This reduces credential distribution and makes missing
unrelated configuration non-fatal for a process.

The existing physical configuration files remain compatibility sources. They
are deserialized into each process's independent root; a later layout change
requires an explicit migration and removal plan for `DATABASE_URL`.
