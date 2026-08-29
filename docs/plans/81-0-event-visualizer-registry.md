# План 81.0 — Event Visualizer Registry: typed renderers для tool, workflow и artifact events

Статус: предложено по [issue #61](https://github.com/rkfsociety/EvoHime/issues/61). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Event Visualizer Registry**: Core-described, host-rendered систему специализированного отображения typed runtime events и artifacts в Conversation/Workbench без превращения каждого нового tool/result type в очередной hardcoded `if/else` внутри UI.

Главный принцип:

> Runtime event остаётся authoritative data contract; visualizer является только presentation projection.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/event-visualizer-registry.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./81-1-event-visualizer-registry.md)
- [Этап 2 — runtime-интеграция и recovery](./81-2-event-visualizer-registry.md)
- [Этап 3 — IPC, client projection и UI](./81-3-event-visualizer-registry.md)
- [Этап 4 — verification, release-evidence и закрытие](./81-4-event-visualizer-registry.md)

## Зависимости

### Блокирующие

- План 72.0 — Core Topic/Subscription Event Bus: typed pub/sub routing for agent runtime.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 76.0 — Safe UI Extension Framework: declarative pages, panels и themes без renderer authority.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- visualizer не является capability;
- only redacted safe projection;
- host controls security/approval renderers;
- no arbitrary HTML/script/native code в v1;
- related resources typed/validated;
- actions Core-owned;
- extension visualizer cannot suppress event/failure;
- unknown schema always has fallback;
- secrets remain masked независимо от visualizer implementation.

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

- [ ] Есть versioned VisualizerDescriptor/Matcher contracts.
- [ ] Resolution deterministic и имеет safe fallback.
- [ ] Built-in visualizers покрывают основные tool/file/test/workflow/artifact events.
- [ ] Visualizer получает только sensitivity-filtered projection.
- [ ] Security-critical renderers host-controlled.
- [ ] Related actions/resources typed и Core-validated.
- [ ] Registry готов к безопасным extension contributions.

## Ограничения и non-goals

- arbitrary HTML/JS event cards;
- изменение runtime semantics через renderer;
- скрытие failures по presentation preference;
- выполнение tool calls напрямую из visualizer;
- полный low-code dashboard builder;
- замена Conversation Workbench;
- гарантированный красивый renderer для любого произвольного JSON в природе.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#61 Event Visualizer Registry: typed renderers для tool, workflow и artifact events](https://github.com/rkfsociety/EvoHime/issues/61)
