# План 97.0 — Model Edit Protocol Registry: строгие patch/search-replace стратегии и repair feedback

Статус: предложено по [issue #77](https://github.com/rkfsociety/EvoHime/issues/77). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime Core-owned **Model Edit Protocol Registry**: набор versioned протоколов, через которые model/editor предлагает изменения исходников, а Core детерминированно парсит, dry-run проверяет и применяет их через Revision-Safe Workspace Files.

Главный принцип:

> модель предлагает структурированную правку; Core решает, применима ли она к точной revision файла.

Не делать raw model response прямой файловой операцией.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/model-edit-protocol-registry.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 39.0 — Structured Response Contract: schema-first ответы модели с provider/tool fallback.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 60.0 — Revision-Safe Workspace Files: uploads/workspace/outputs namespaces и stale-write protection.
- План 70.0 — Code Diagnostics Feedback Loop: LSP/compiler evidence и regression delta после agent edits.
- План 84.0 — Output Guardrail Pipeline: semantic validators, transforms и bounded correction loops.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- parser Core-owned/versioned;
- model payload не определяет host path authority;
- every mutation carries revision/hash precondition;
- ambiguous/fuzzy match fails closed;
- protocol cannot expand write scope;
- malformed edit не исполняется как shell/text command;
- whole-file replace bounded policy;
- repair loop bounded;
- output/provenance не содержит лишние sensitive file fragments.

## План реализации

1. Зафиксировать versioned typed contract, state machine, provenance, limits,
   failure/unknown-outcome semantics и threat model; отдельно перечислить
   поля, которые могут быть предложены моделью, и authoritative Core evidence.
2. Реализовать Core validation и durable storage/event transitions. Миграция
   должна быть additive, транзакционной, с backup/recovery и deterministic
   serialization/hash там, где сущность versioned.
3. Подключить существующие registry/tool/workflow/provider/child контуры,
   повторные grant/policy/approval проверки и bounded retry/cancellation.
4. Добавить additive IPC, main/preload adapter и metadata-only renderer/UI;
   sensitive payload, raw prompt/output и credentials не передавать.
5. Провести focused unit/storage/integration/recovery/security/eval tests,
   обновить architecture/current-state только после фактической реализации
   и сохранить команду воспроизведения проверки.

## Критерии готовности из issue

- [ ] Есть versioned EditProtocol registry.
- [ ] Минимум SEARCH/REPLACE + patch + structured/whole-file protocols оформлены явно.
- [ ] Любой edit проходит parse + dry-run/preflight до mutation.
- [ ] Revision/hash preconditions обязательны.
- [ ] Ambiguous/fuzzy edits не применяются молча.
- [ ] Failure feedback позволяет bounded repair только неуспешных edits.
- [ ] Protocol selection привязан к ModelProfile/strategy, а не model-name branches.
- [ ] Метрики позволяют сравнивать protocol reliability.

## Ограничения и non-goals

- собственный language-aware refactoring engine;
- arbitrary fuzzy replacement;
- применение patch к stale file «примерно туда»;
- генерация shell commands из malformed patch;
- автоматический whole-file rewrite любого размера;
- обход Revision-Safe Workspace Files.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#77 Model Edit Protocol Registry: строгие patch/search-replace стратегии и repair feedback](https://github.com/rkfsociety/EvoHime/issues/77)
