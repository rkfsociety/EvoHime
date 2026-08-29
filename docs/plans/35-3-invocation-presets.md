# План 35.3 — Invocation Presets: version-pinned шаблоны запусков без копирования секретов: IPC, client projection и UI

Статус: этап 3 для [плана 35.0](./35-0-invocation-presets.md); после [плана 35.2](./35-2-invocation-presets.md).

## Цель

Добавить typed client surface для чтения Core state и явных user actions без переноса authority.

## Зависимости

### Блокирующие

- План 35.2 — runtime vertical slice, recovery и стабильные commands/events.
- Authenticated named-pipe IPC, sequence replay/resync и generated TypeScript protocol.

### Опциональные

- План 33.0 — зависимость из обзора.
- План 30.0 — optional portable export/import; UI не зависит от него.
- План 34.0 — optional trigger base mapping с явным unavailable fallback.

## Реализация

0. Проверить proto и зарезервировать additive names/tags, не меняя semantics старых clients.
1. Добавить bounded request/result/event messages с correlation, idempotency, optimistic version и typed errors; исключить secrets, raw prompt и hidden reasoning.
2. Реализовать Electron main/preload и предусмотренный client adapter; он только сериализует и маршрутизирует Core commands.
3. Расширить существующий `WorkflowPanel` projection/action surface для списка
   preset, manual create, save-from-completed-run preview, edit, duplicate,
   delete, run, migration и credential rebinding; renderer не вычисляет state
   machine и не запускает effect.
4. Проверить reconnect, replay gap, duplicate event, stale/denied action и unavailable optional backend.
5. Привязать UI/CLI trace к Core event/provenance IDs без запрещённых payload.

## Предметная декомпозиция

### Protocol and client surfaces

- Proto: добавить additive `InvocationPresetsRequest`, `InvocationPresetsResponse`, `InvocationPresetsEvent` и command/event oneof в `crates/desktop-ipc/proto/evohime.desktop.proto` после проверки свободных tags; сохранить major, replay/resync и bounded frame limits.
- Bridge: связать `crates/evohime-core/src/ipc_bridge.rs`, `desktop/evohime-electron/src/shared/api.ts`, `desktop/evohime-electron/src/preload/index.ts` и `desktop/evohime-electron/src/main/shell-bridge.ts`; renderer не получает Core/storage authority.
- UI: расширить `desktop/evohime-electron/src/renderer/src/WorkflowPanel.tsx`;
  показывать pinned version/schema drift, masked sensitive values,
  NeedsRebinding, temporary overrides и schedule snapshot status. Тесты —
  `desktop/evohime-electron/tests/invocation_presets.test.tsx` и protocol/typecheck gates.

### Acceptance-to-projection matrix

- `C08` — Preset можно использовать scheduler без обхода approvals. → показывать состояние только из Core event/evidence, без локального вывода renderer.
- `C01`/`C02` — contract и pinned version. → показывать revision, definition/schema hashes и drift из Core projection.
- `C03` — Можно создать preset из completed run. → дать review-before-save preview с очищенными полями.
- `C04`/`C05` — refs и secret inputs. → показывать только masked values/status; raw secrets не входят в DTO/state.
- `C06`/`C13` — migration и version drift. → показывать diff/compatibility result и требовать явного migrate/duplicate.
- `C09` — Ручное создание. → поддержать schema-driven form и Core validation errors.
- `C10` — NeedsRebinding. → дать explicit rebinding action без локального credential authority.
- `C11` — Temporary override. → отделить run-only overlay от сохранения preset.
- `C12` — Schedule snapshot. → показывать зафиксированную revision/hash и drift после edit.
- `C14` — Trigger base mapping. → отображать bounded mapping status; protected identities не редактируются event payload.

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
