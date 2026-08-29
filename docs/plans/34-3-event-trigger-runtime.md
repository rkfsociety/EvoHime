# План 34.3 — Event Trigger Runtime: безопасный запуск workflow по внешним событиям: IPC, client projection и UI

Статус: этап 3 для [плана 34.0](./34-0-event-trigger-runtime.md); после [плана 34.2](./34-2-event-trigger-runtime.md).

## Цель

Добавить typed client surface для чтения Core state и явных user actions без переноса authority.

## Зависимости

### Блокирующие

- План 34.2 — runtime vertical slice, recovery и стабильные commands/events.
- Authenticated named-pipe IPC, sequence replay/resync и generated TypeScript protocol.

### Опциональные

- План 33.0 — provider trigger capability projection contract; без него UI
  показывает typed `unavailable/degraded` для provider-backed paths и сохраняет
  локальные/system-event actions доступными.
- Provider-specific UI details; без конкретного provider UI показывает typed unavailable/degraded, не скрывая состояние trigger.

## Реализация

0. Проверить proto и зарезервировать additive names/tags, не меняя semantics старых clients.
1. Добавить bounded request/result/event messages с correlation, idempotency, optimistic version и typed errors; исключить secrets, raw prompt и hidden reasoning.
2. Реализовать Electron main/preload и предусмотренный client adapter; он только сериализует и маршрутизирует Core commands.
3. Добавить renderer/CLI projection для status, progress, blockers, refs, warnings и actions; renderer не вычисляет state machine и не запускает effect.
4. Проверить reconnect, replay gap, duplicate event, stale/denied action и unavailable optional backend.
5. Привязать UI/CLI trace к Core event/provenance IDs без запрещённых payload.

## Предметная декомпозиция

### Protocol and client surfaces

- Proto: добавить additive `EventTriggerRuntimeRequest`, `EventTriggerRuntimeResponse`, `EventTriggerRuntimeEvent` и command/event oneof в `crates/desktop-ipc/proto/evohime.desktop.proto` после проверки свободных tags; сохранить major, replay/resync и bounded frame limits.
- Bridge: связать `crates/evohime-core/src/ipc_bridge.rs`, `desktop/evohime-electron/src/shared/api.ts`, `desktop/evohime-electron/src/preload/index.ts` и `desktop/evohime-electron/src/main/shell-bridge.ts`; renderer не получает Core/storage authority.
- UI: создать `desktop/evohime-electron/src/renderer/src/EventTriggerRuntimePanel.tsx` только как projection/action surface; тесты — `desktop/evohime-electron/tests/event_trigger_runtime.test.tsx` и protocol/typecheck gates.

### Acceptance-to-projection matrix

- `C01` — Есть versioned TriggerDefinition. → показывать stable id/version/workflow binding и не позволять renderer выбирать новую workflow version.
- `C02` — Есть normalized EventEnvelope. → показывать только bounded redacted source/event/time/hash metadata.
- `C03` — Webhook authenticity и schema проверяются до enqueue. → показывать фактические Core outcomes `accepted/rejected/unavailable`, без локального self-attestation.
- `C04` — Есть dedup/replay protection. → показывать duplicate/coalesced/replayed outcomes и counters из Core events.
- `C05` — Workflow version pinned. → показывать pinned version и typed stale outcome при изменении binding.
- `C06` — Input mapping ограничивает payload. → показывать allowlisted mapping summary и validation blockers, без raw payload.
- `C07` — Есть rate limits/circuit breaker. → дать bounded projection и явные Core-checked pause/resume/reconcile actions.
- `C08` — State durable/recoverable. → показывать persisted status, pending count, last execution ref и recovery/error state из replayable Core events.
- `C09` — Existing workflow approvals/grants сохраняются. → показывать approval-required/denied outcomes, не превращая trigger configuration в approval.

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
