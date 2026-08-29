# План 74.0 — Declarative Agent Component Registry: versioned runtime components with schema-safe loading

Статус: предложено по [issue #54](https://github.com/rkfsociety/EvoHime/issues/54). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Declarative Agent Component Registry** для версионируемого описания, сохранения, загрузки и миграции agent-runtime компонентов без жёсткой привязки конфигураций к внутренним Rust type names.

Это не plugin marketplace и не Integration Provider SDK (#13). Здесь речь о **внутренних компонентах агентной архитектуры**: роли, team policies, termination policies, context policies, model profiles, workbenches и другие reusable runtime building blocks.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/declarative_agent_component_registry.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./74-1-declarative-agent-component-registry.md)
- [Этап 2 — runtime-интеграция и recovery](./74-2-declarative-agent-component-registry.md)
- [Этап 3 — IPC, client projection и UI](./74-3-declarative-agent-component-registry.md)
- [Этап 4 — verification, release-evidence и закрытие](./74-4-declarative-agent-component-registry.md)

## Зависимости

### Блокирующие

- План 67.0 — Schema-Driven Agent Configuration: Core-owned schemas для agent/conversation settings.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 46.0 — Agent Role Profiles: versioned специализация, ограничения и strategy contracts.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

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

- [ ] Есть stable ComponentDescriptor
- [ ] Provider IDs не зависят от внутренних Rust paths
- [ ] Registry Core-owned
- [ ] Loading требует schema validation
- [ ] Version migration explicit и testable
- [ ] Unknown provider fail-closed
- [ ] Secrets не сериализуются напрямую
- [ ] Workflow/team configs могут ссылаться на компоненты декларативно
- [ ] Есть inspect/diff/migration UX

## Ограничения и non-goals

- Не реализовывать marketplace.
- Не разрешать arbitrary dynamic code loading в первой версии.
- Не заменять Integration Provider SDK (#13).
- Не превращать каждую структуру Core в Component.
- Не использовать registry как способ обойти capability/approval model.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#54 Declarative Agent Component Registry: versioned runtime components with schema-safe loading](https://github.com/rkfsociety/EvoHime/issues/54)
