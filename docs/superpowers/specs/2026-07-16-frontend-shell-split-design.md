# Frontend shell decomposition (6.13)

> Дата: 2026-07-16  
> Статус: approved (approach A)

## Цель

Распилить `frontend/web/src/app.tsx` (~3000 строк) на types / api / lib / hooks / panels без смены UX и без переноса бизнес-логики во frontend.

## Структура

```text
frontend/web/src/
  types.ts
  lib/format.ts
  lib/storage.ts
  lib/paths.ts
  api/client.ts
  api/sessions.ts
  api/files.ts
  api/git.ts
  api/models.ts
  api/permissions.ts
  api/projects.ts
  api/github.ts
  api/mcp.ts
  api/tools.ts
  hooks/useServerEventHandler.ts   # applyEvent logic (optional if time)
  panels/*.tsx                     # settings, tasks, actions, sites, plugins, prs, files, editor, git, scheduled
  components/                      # existing ApprovalModal, TerminalPanel
  app.tsx                          # shell + remaining chat wiring
```

## Typed API client

- `apiRequest<T>(path, init?)` — JSON parse, non-ok → Error with body text
- Domain helpers call `apiRequest`; `App` stops using raw `fetch` for known endpoints

## Constraints

- No behavior change
- Protocol types stay in `protocol.ts`
- Commit after each coherent layer when practical; at least one commit for the slice
