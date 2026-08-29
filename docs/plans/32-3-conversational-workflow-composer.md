# План 32.3 — Conversational Workflow Composer: создание и правка workflow из естественного языка: IPC, client projection и UI

Статус: этап 3 для [плана 32.0](./32-0-conversational-workflow-composer.md); после [плана 32.2](./32-2-conversational-workflow-composer.md).

## Цель

Добавить typed client surface для чтения Core state и явных user actions без переноса authority.

## Зависимости

### Блокирующие

- План 32.2 — runtime vertical slice, recovery и стабильные commands/events.
- План 31.3 — builder client surface, в котором открывается draft.
- Authenticated named-pipe IPC, sequence replay/resync и generated TypeScript protocol.

### Опциональные

- План 30.0 — зависимость из обзора.

## Реализация

0. Проверить proto и зарезервировать additive names/tags, не меняя semantics старых clients.
1. Добавить bounded request/result/event messages для `GenerateProposal`, `ApplyEdit`, `ValidateDraft`, `SaveDraft`, `DiscardDraft` и Builder handoff с correlation, idempotency, optimistic draft revision и typed errors; исключить secrets, raw prompt/output и hidden reasoning.
2. Реализовать Electron main/preload и предусмотренный client adapter; он только сериализует и маршрутизирует Core commands.
3. Добавить чатовую projection/action surface для status, assumptions, blockers, missing integrations/credentials, risk preview и typed edits; передать validated draft в существующий WorkflowPanel/Builder handoff. Renderer не вычисляет state machine и не запускает effect. CLI не является обязательной частью этого плана.
4. Проверить reconnect, replay gap, duplicate event, stale/denied action и unavailable optional backend.
5. Привязать chat UI trace к Core event/provenance IDs без запрещённых payload.

## Предметная декомпозиция

### Protocol and client surfaces

- Proto: добавить additive `ConversationalWorkflowComposerRequest`, `ConversationalWorkflowComposerResponse`, `ConversationalWorkflowComposerEvent` и command/event oneof в `crates/desktop-ipc/proto/evohime.desktop.proto` после проверки свободных tags; сохранить major, replay/resync и bounded frame limits.
- Bridge: связать `crates/evohime-core/src/ipc_bridge.rs`, `desktop/evohime-electron/src/shared/api.ts`, `desktop/evohime-electron/src/preload/index.ts` и `desktop/evohime-electron/src/main/shell-bridge.ts`; renderer не получает Core/storage authority.
- UI: встроить Composer в chat surface и переиспользовать `WorkflowPanel`/Builder handoff вместо второго canvas; если отдельный компонент нужен по live UI структуре, его имя подтверждается перед реализацией. Тесты — `desktop/evohime-electron/tests/conversational_workflow_composer.test.tsx`, protocol и typecheck gates.

### Acceptance-to-projection matrix

- `C07` — Draft можно открыть в builder и сохранить как immutable version. → дать bounded projection и явные Core-checked actions.
- `C01` — Ева умеет создавать workflow draft из natural language. → показать только Core-returned proposal/validation status и bounded assumptions.
- `C02` — Proposal отделён от authoritative workflow contract. → UI явно разделяет proposal, validated draft и saved revision.
- `C03` — Core выполняет capability binding и validation. → показать binding statuses `bound/ambiguous/missing/incompatible`, не разрешая client-side override.
- `C05` — Missing integrations/credentials показываются отдельно. → отдельные redacted cards без credential values.
- `C06` — Есть risk/side-effect preview. → показать Core risk projection до Save/Run.
- `C08` — Composer не может расширить permissions или выполнить draft самовольно. → Save/Run/approval остаются явными Core-checked actions.

### Client safety and replay

- Mutation requests несут correlation/idempotency/optimistic version; Core повторно проверяет authorization и возвращает typed stale/denied/unavailable outcomes.
- Events bounded и redacted; reconnect/replay gap/duplicate отображаются явно, а renderer не вычисляет state machine и не запускает effect.

## Критерии выхода

- [ ] Новая surface additive и authenticated.
- [ ] Mutations повторно проверяются Core и защищены idempotency/version.
- [ ] Renderer получает только bounded projection; CLI не входит в обязательную границу.
- [ ] Reconnect/replay и stale actions предсказуемы.
- [ ] UI показывает фактический Core state.

## Не входит

New transport, direct DB access, credentials в client state и renderer business logic.
