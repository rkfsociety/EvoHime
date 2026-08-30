# План 77.3 — Headless Core CLI: non-interactive agent/workflow runs для CI, scripts и NDJSON automation: IPC, client projection и UI

Статус: этап 3 для [плана 77.0](./77-0-headless-core-cli.md); после [плана 77.2](./77-2-headless-core-cli.md).

## Цель

Добавить typed client surface для чтения Core state и явных user actions без переноса authority.

## Зависимости

### Блокирующие

- План 77.2 — runtime vertical slice, recovery и стабильные commands/events.
- Authenticated named-pipe IPC, sequence replay/resync и generated TypeScript protocol.

### Опциональные

- План 30.0 — зависимость из обзора.
- План 31.0 — зависимость из обзора.
- Реализованный Conversational Workflow Composer v1 — зависимость из обзора.
- План 59.0 — зависимость из обзора.

## Реализация

0. Проверить proto и зарезервировать additive names/tags, не меняя semantics старых clients.
1. Добавить bounded request/result/event messages с correlation, idempotency, optimistic version и typed errors; исключить secrets, raw prompt и hidden reasoning.
2. Реализовать Electron main/preload и предусмотренный client adapter; он только сериализует и маршрутизирует Core commands.
3. Добавить renderer/CLI projection для status, progress, blockers, refs, warnings и actions; renderer не вычисляет state machine и не запускает effect.
4. Проверить reconnect, replay gap, duplicate event, stale/denied action и unavailable optional backend.
5. Привязать UI/CLI trace к Core event/provenance IDs без запрещённых payload.

## Предметная декомпозиция

### Protocol and client surfaces

- Proto: добавить additive `HeadlessCoreCliRequest`, `HeadlessCoreCliResponse`, `HeadlessCoreCliEvent` и command/event oneof в `crates/desktop-ipc/proto/evohime.desktop.proto` после проверки свободных tags; сохранить major, replay/resync и bounded frame limits.
- Bridge: связать `crates/evohime-core/src/ipc_bridge.rs`, `desktop/evohime-electron/src/shared/api.ts`, `desktop/evohime-electron/src/preload/index.ts` и `desktop/evohime-electron/src/main/shell-bridge.ts`; renderer не получает Core/storage authority.
- UI: создать `desktop/evohime-electron/src/renderer/src/HeadlessCoreCliPanel.tsx` только как projection/action surface; тесты — `desktop/evohime-electron/tests/headless_core_cli.test.tsx` и protocol/typecheck gates.

### Acceptance-to-projection matrix

- `C01` — CLI является клиентом существующего Core, а не отдельным runtime. → дать bounded projection и явные Core-checked actions.
- `C03` — Agent/workflow run можно запускать без desktop renderer. → дать bounded projection и явные Core-checked actions.
- `C06` — CLI использует существующие profiles/budgets/checkpoints/event log. → дать bounded projection и явные Core-checked actions.
- `C08` — Credentials и sensitive output не протекают через CLI boundary. → показывать только redacted projection и provenance без raw payload.

### Client safety and replay

- Mutation requests несут correlation/idempotency/optimistic version; Core повторно проверяет authorization и возвращает typed stale/denied/unavailable outcomes.
- Events bounded и redacted; reconnect/replay gap/duplicate отображаются явно, а renderer не вычисляет state machine и не запускает effect.

## Критерии выхода

- [ ] Новая surface additive и authenticated.
- [ ] Mutations повторно проверяются Core и защищены idempotency/version.
- [ ] Renderer/CLI получает только bounded projection.
- [ ] Reconnect/replay и stale actions предсказуемы.
- [ ] UI показывает фактический Core state.

## Не входит

New transport, direct DB access, credentials в client state и renderer business logic.
