# План 31.0 — Visual Workflow Builder: typed canvas, validation и live runtime inspection

Статус: предложено по [issue #11](https://github.com/rkfsociety/EvoHime/issues/11). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime визуальный **Workflow Builder** для существующего `workflow/v1`: пользователь собирает и редактирует workflow как граф из типизированных блоков, а во время исполнения может видеть состояние узлов и поток данных.

Builder не создаёт новый runtime. Он является безопасной authoring/inspection поверхностью над уже существующими Core-owned workflow contracts и registry.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/visual-workflow-builder.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./31-1-visual-workflow-builder.md)
- [Этап 2 — runtime-интеграция и recovery](./31-2-visual-workflow-builder.md)
- [Этап 3 — IPC, client projection и UI](./31-3-visual-workflow-builder.md)
- [Этап 4 — verification, release-evidence и закрытие](./31-4-visual-workflow-builder.md)

## Зависимости

### Блокирующие

- Специфических межплановых blocking зависимостей нет; используется текущий Core/IPC фундамент проекта.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 30.0 — Workflow Package: переносимый import/export без секретов и с rebinding зависимостей.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

Renderer не становится владельцем workflow semantics.

```text
Renderer Builder
  -> draft commands / typed edits
  -> desktop IPC
  -> Core Workflow Authoring Service
  -> WorkflowRegistry validation
  -> immutable versioned definition
```

Renderer может держать presentation draft для UX, но authoritative validation, identity resolution, canonical hash и сохранение выполняет Core.

### Безопасность

- renderer не может отправить arbitrary shell/URL как block identity;
- все node identities разрешаются Core registry;
- visual metadata не влияет на grants;
- connection не расширяет capability;
- sensitive values masked;
- approval node нельзя удалить из policy-mandated path без validation failure;
- imported draft проходит те же проверки;
- runtime inspector read-only относительно уже запущенного snapshot.

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

- [ ] Есть canvas над существующим workflow contract.
- [ ] Pins и block metadata приходят из Core registry.
- [ ] Core выполняет authoritative validation.
- [ ] Сохранение создаёт immutable новую version.
- [ ] Layout metadata отделена от execution hash.
- [ ] Есть recovery draft.
- [ ] Есть read-only live runtime inspection.
- [ ] Sensitive payload не утекает в renderer.

## Ограничения и non-goals

- новый workflow runtime;
- arbitrary scripting nodes;
- выполнение кода в renderer;
- marketplace;
- collaborative multi-user editing;
- изменение running graph на лету;
- ручное обходное редактирование grants/registry через canvas.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#11 Visual Workflow Builder: typed canvas, validation и live runtime inspection](https://github.com/rkfsociety/EvoHime/issues/11)
