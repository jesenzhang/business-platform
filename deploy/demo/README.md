# Business Platform 0.1 demo

From the repository root:

```powershell
.\scripts\demo-up.ps1
.\scripts\demo-seed.ps1
```

Open `http://localhost:4173` for the console. The REST API is at
`http://localhost:3000`, its public contract is `openapi.json`, and the
read-only MCP endpoint is `http://localhost:3100/mcp`.

The stack uses PostgreSQL and MinIO. The demo API has a fixed development
principal and token only; production configuration rejects development auth.
The deterministic extractor is the existing local provider used by the
workers and is not available through production configuration.

Stop services with `.\scripts\demo-down.ps1`. Reset only the demo data with
`.\scripts\demo-reset.ps1`.
