# План 117.3 — Architecture Snapshot & Drift Review: IPC, client projection и UI

Статус: этап 3 для [плана 117.0](./117-0-architecture-snapshot-drift-review.md); после [плана 117.2](./117-2-architecture-snapshot-drift-review.md).

## Цель

Дать Electron bounded projection для текущего snapshot, topology exploration, exact evidence и Before/Delta/After review без переноса authority из Core.

## Зависимости

### Блокирующие

- План 117.2 — snapshot/delta commands, events, recovery и stable result types.
- Authenticated desktop IPC, sequence replay/resync, generated TypeScript protocol, main/preload bridge и existing Project/Workbench navigation.

### Опциональные

- Typed Context References/Context Mentions для `@architecture:current`, component и delta refs.
- Diagnostics bundle для отображения расширенного warning summary.

## Реализация

1. После проверки highest tag зарезервировать additive commands/events/results, сохранив major compatibility, frame limits, request correlation, idempotency, sequence replay и resync.
2. Добавить Core commands: current/list snapshot, refresh/rebuild, get component/relationship, open evidence, upstream/downstream/route, compare snapshots, create/compare expected delta и review/acknowledge bounded outcome. Core заново проверяет paths, roots, hashes, permissions и versions.
3. Передавать только metadata projection: ids, names, kinds, bounded responsibility, counts, states, confidence, warnings, exact evidence refs и freshness. Не передавать raw source, prompts, outputs, secrets, hidden reasoning или unrestricted graph.
4. Связать `ipc_bridge.rs`, shared protocol, Electron main/preload adapters и reconnect/replay handling. Renderer не может изменить Verified/Candidate, accepted state, topology, delta или policy verdict.
5. Добавить Project/Workbench → Architecture: Current (revision/status/coverage/counts/warnings), bounded map/search/focus/boundaries, evidence badges/open exact source, filters, route view и Compare Before/Delta/After.
6. Добавить loading/refreshing/stale/failed/last-good states, explicit unsupported/root-denied warnings и accessible text fallback. Layout не должен превращать route в «подтверждённый impact».

## Acceptance-to-projection matrix

- `C01` Current → revision, hash prefix, status, freshness, coverage and bounded counts.
- `C02` Explore → stable component/relationship ids, boundaries, filters and route semantics.
- `C03` Evidence → exact file/range/symbol refs with Core authorization and stale status.
- `C04` Compare → added/removed/changed, uncertain matches, boundary/evidence/coverage changes.
- `C05` Review → expected/missing/unexpected/ambiguous and Core policy verdict.
- `C06` Security → replay/auth/bounds and no renderer-forged verified or accepted state.

## Критерии выхода

- [ ] IPC additive, authenticated, bounded and replay-safe.
- [ ] Every mutation is Core-checked, idempotent/versioned and returns typed stale/denied outcome.
- [ ] UI displays Core projection after reconnect and does not own graph/delta logic.
- [ ] Sensitive payloads, raw source and credentials absent from IPC and renderer state.

## Не входит

Direct filesystem/SQLite access, client-side extraction, unrestricted graph visualization, WYSIWYG editing и automatic review approval.
