# План 31.3 — Visual Workflow Builder: typed canvas, validation и live runtime inspection: IPC, client projection и UI

Статус: этап 3 для [плана 31.0](./31-0-visual-workflow-builder.md); после [плана 31.2](./31-2-visual-workflow-builder.md).

## Цель

Добавить typed client surface для чтения Core state и явных user actions без переноса authority.

## Зависимости

### Блокирующие

- План 31.2 — runtime vertical slice, recovery и стабильные commands/events.
- Authenticated named-pipe IPC, sequence replay/resync и generated TypeScript protocol.

### Опциональные

- План 30.0 — зависимость из обзора.

## Реализация

0. Проверить proto и зарезервировать additive names/tags, не меняя semantics старых clients.
1. Добавить bounded request/result/event messages с correlation, idempotency, optimistic version и typed errors; исключить secrets, raw prompt и hidden reasoning.
2. Реализовать Electron main/preload и предусмотренный client adapter; он только сериализует и маршрутизирует Core commands.
3. Расширить существующий `WorkflowPanel` либо добавить builder-поверхность для draft/canvas и read-only run inspection; CLI в текущий scope не входит. Renderer не вычисляет state machine и не запускает effect.
4. Проверить reconnect, replay gap, duplicate event, stale/denied action и unavailable optional backend.
5. Привязать UI trace к Core event/provenance IDs без запрещённых payload.

## Предметная декомпозиция

### Protocol and client surfaces

- Proto: добавить additive `VisualWorkflowBuilderRequest`, `VisualWorkflowBuilderResponse`, `VisualWorkflowBuilderEvent` и command/event oneof в `crates/desktop-ipc/proto/evohime.desktop.proto` после проверки свободных tags; сохранить major, replay/resync и bounded frame limits.
- Bridge: расширить `crates/evohime-core/src/ipc_bridge.rs`, `desktop/evohime-electron/src/shared/api.ts`, `desktop/evohime-electron/src/preload/index.ts`, `desktop/evohime-electron/src/renderer/src/shell-api.ts` и `desktop/evohime-electron/src/main/shell-bridge.ts`; renderer не получает Core/storage authority.
- UI: расширить существующий `desktop/evohime-electron/src/renderer/src/WorkflowPanel.tsx` или добавить отдельную panel только после проверки UX-интеграции в `App.tsx`; тесты — `desktop/evohime-electron/tests/visual_workflow_builder.test.tsx` и protocol/typecheck gates.

### Acceptance-to-projection matrix

- `C07` — Есть read-only live runtime inspection. → дать bounded projection и явные Core-checked actions.
- `C08` — Sensitive payload не утекает в renderer. → показывать только redacted projection и provenance без raw payload.

### Client safety and replay

- Mutation requests несут correlation/idempotency/optimistic version; Core повторно проверяет authorization и возвращает typed stale/denied/unavailable outcomes.
- Events bounded и redacted; reconnect/replay gap/duplicate отображаются явно, а renderer не вычисляет state machine и не запускает effect.

## Критерии выхода

- [ ] Новая surface additive и authenticated.
- [ ] Mutations повторно проверяются Core и защищены idempotency/version.
- [ ] Renderer получает только bounded projection.
- [ ] Reconnect/replay и stale actions предсказуемы.
- [ ] UI показывает фактический Core state.

## Не входит

New transport, direct DB access, credentials в client state и renderer business logic.
