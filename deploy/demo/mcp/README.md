# Business Platform demo MCP

The demo exposes a stateless, read-only MCP adapter at
`http://localhost:3100/mcp` using protocol version `2026-07-28`.

The adapter has its own demo bearer token, then calls the Business API with
the server-configured development principal. Tenant and permissions are not
accepted as tool arguments or request-controlled headers.

Use `example-client-config.json` as a generic HTTP MCP client configuration.
After the stack is running, discover tools with `tools/list` and exercise:

- `document.processing.list`
- `document.processing.get`
- `operations.overview`

The MCP adapter has no database or object-storage dependency. Stop it and the
Business API and workers continue to operate independently.
