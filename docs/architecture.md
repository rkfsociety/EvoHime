# EvoHime Architecture

Web-first AI-agent platform. Browser is the only client.

## Components

```text
Browser
   │
   ├── HTTP API
   └── WebSocket
          │
          ▼
EvoHime Server (crates/server)
├── Agent Runtime (crates/agent-runtime)
├── Task Engine (crates/task-engine)
├── Model Gateway (crates/model-gateway) — LiteRouter + mock
├── Tool Runtime (crates/tool-runtime)
├── Permission Engine (crates/permissions)
├── Project Index (crates/project-index) — phase 6
├── Protocol (crates/protocol)
└── Storage (crates/storage)
```

## End-to-end foundation

```text
User message
  → POST /api/sessions + WS /ws/:session_id
  → task-engine creates task
  → agent-runtime loads history and runs the agent loop
  → tool-runtime executes sandboxed tools
  → events persisted in PostgreSQL
  → browser renders chat + event timeline
```

## Protocol

Single JSON Schema drives Rust enums and generated TypeScript types:

- Schema: `crates/protocol/schema/evohime.protocol.schema.json`
- Rust: `crates/protocol/src/lib.rs`
- TypeScript: `frontend/web/src/protocol.generated.ts`

Regenerate TS types:

```bash
npm run generate:protocol
```

## Deployment

```bash
docker compose up --build
```

- Web UI: http://localhost:5173
- API: http://localhost:3000

## Roadmap

| Stage | Scope | Status |
| --- | --- | --- |
| 1 | Monorepo, server, web UI, PostgreSQL, WebSocket protocol | ✅ Done |
| 2 | Chat with model, streaming, sessions/history | ✅ Done |
| 3 | Filesystem/shell tools, terminal, permissions | 🟡 Backend base |
| 4 | Monaco editor, file tree, Git diff | 🟡 Git backend |
| 5 | Task planning, parallel tools, cancel/resume | 🟡 Lifecycle base |
| 6 | Project index, MCP, multi-model, Python workers | 📋 Planned |

- [development-plan.md](development-plan.md) — полный план
- [roadmap.md](roadmap.md) — дорожная карта с milestones
- [current-state.md](current-state.md) — текущий статус
