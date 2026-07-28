---
name: architecture
description: "Архитектура EvoHime, структура кода, компоненты"
metadata: 
  node_type: memory
  type: reference
  originSessionId: adc991a0-0ba6-4d74-ace4-d6cf01d7403e
  modified: 2026-07-24T16:13:54.744Z
---

## Структура Проекта

```
EvoHime/
├── crates/                    # Rust монореп
│   ├── protocol/              # JSON Schema → Rust enums, shared types
│   ├── permissions/           # Permission system
│   ├── storage/               # PostgreSQL CRUD (sessions, tasks, memory_items)
│   ├── memory/                # Redact, dedupe, conflict resolution, extraction
│   ├── tool-runtime/          # fs, shell, git, browser, mcp sandboxed tools
│   ├── agent-runtime/         # ReAct loop orchestration
│   ├── task-engine/           # Task lifecycle, checkpoints
│   ├── model-gateway/         # LiteRouter + OpenAI-compatible routing
│   ├── evals/                 # Golden-task eval harness
│   └── server/                # HTTP + WebSocket entrypoint (45+ endpoints)
│
├── frontend/web/              # React + TypeScript
│   ├── src/
│   │   ├── app.tsx            # Root component (grid: sidebar + main + trace)
│   │   ├── api/               # API clients by domain
│   │   ├── hooks/             # React hooks (useChat, useWebSocket, useWorkspace)
│   │   ├── panels/            # Lazy-loaded UI panels (Chat, Tasks, Files, Editor, etc.)
│   │   ├── components/        # Reusable components (modals, errors, search)
│   │   ├── lib/               # Utilities (format, paths, storage)
│   │   └── styles/            # CSS (core, chat, panels, settings, workspace)
│   └── vite.config.ts
│
├── migrations/                # PostgreSQL schema versions (0001–0030+)
├── docs/                      # Documentation
│   ├── current-state.md       # Source of truth (стадия, API endpoints)
│   ├── development-plan.md    # Детальный план
│   ├── roadmap.md             # Milestones
│   └── superpowers/           # Specs и plans по фичам
├── workers/python/            # Python worker (summarize, chunk, classify, etc.)
├── scripts/                   # Build scripts
└── .github/workflows/         # CI/CD (GitHub Actions)
```

## Frontend Панели (Lazy-loaded)

- **ChatPanel** → чат + markdown
- **ActionsPanel** → действия агента
- **TasksPanel** → таски с deep-dive
- **TerminalPanel** → xterm.js shell output
- **FilesPanel** → файловое дерево
- **EditorPanel** → Monaco Editor
- **GitPanel** → git status + diff
- **PluginsPanel** → plugin management
- **MemoryPanel** → structured memory items
- **PullRequestsPanel** → GitHub PRs
- **SitesPanel** → workspace HTML sites
- **ScheduledPanel** → cron-scheduled tasks
- **SettingsPanel** → конфиг

## ReAct Loop (Agent Runtime)

```
user.message
  ↓
POST /api/sessions (создаёшь сессию)
  ↓
load task + history + memory_items
  ↓
[tool.started → tool.output → tool.completed] × N (bounded ReAct)
  ↓
approval.required (if protected operation)
  ↓
agent.message.delta (streamed response)
  ↓
extract candidates (on success)
  ↓
memory.ask (admission gate)
  ↓
task.completed | task.failed
  ↓
persist all events + memory updates in PostgreSQL
```

## Tool Registry (Sandboxed)

Инструменты с sandboxing, permissions, timeout, cancellation:
- `filesystem.{read, write, patch, search}`
- `shell.execute`
- `git.{status, diff, commit, pull, push}`
- `browser.{open, extract, session.*}` (persistent CDP tab, 7.100+)
- `mcp.call` (MCP tool invocation)
- `memory.search` (structured memory retrieval)

## API Endpoints (45+)

Main routes:
- `GET /health` — health check
- `POST /api/sessions` — create session
- `WS /ws/:session_id` — real-time events
- `GET/PUT /api/files/content` — file operations
- `GET/POST /api/git/*` — git operations
- `GET/POST /api/github/pull-requests` — GitHub integration
- `GET/PUT /api/models/config` — LLM routing
- `GET/POST /api/memory` — memory list/search/import/export
- `GET/POST /api/plugins` — plugin management
- `GET/PUT /api/worker/jobs` — job queue
- `GET /api/scheduled` — cron tasks
- `GET /api/sync/*` — cloud sync (owner-only)

Full list in `docs/current-state.md`.

## Core Types

| Type | Crate | Назначение |
|------|-------|-----------|
| `ServerEvent` | protocol | Events from server to client |
| `ClientCommand` | protocol | Commands from client to server |
| `StorageClient` | storage | PostgreSQL CRUD |
| `AgentRunner` | agent-runtime | ReAct orchestration |
| `ToolRegistry` | tool-runtime | Tool execution |
| `MemoryService` | memory | Memory pipeline |
| `ModelGateway` | model-gateway | LLM provider routing |

## Key Design Decisions

1. **JSON Schema as SSOT** → generates Rust enums + TypeScript types
2. **WebSocket for real-time** → typed ServerEvent/ClientCommand
3. **Sandboxed tools** → filesystem/shell/git isolated with permissions
4. **Bounded ReAct** → step limit, timeout, cancellation
5. **Structured memory** → extract candidates, admission gate, dupe-check
6. **PostgreSQL for persistence** → all state (sessions, tasks, events, approvals)
7. **Lazy-loaded panels** → code splitting for faster frontend
8. **LiteRouter for LLM routing** → OpenAI-compatible, model selection by task
