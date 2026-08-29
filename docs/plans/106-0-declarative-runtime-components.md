# План 106.0 — Declarative Runtime Components: versioned component config, provider registry и safe rehydration

Статус: предложено по [issue #86](https://github.com/rkfsociety/EvoHime/issues/86). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Declarative Runtime Component** contract: безопасный способ описывать model clients, agents, teams, selectors, workbenches и другие runtime-компоненты как versioned declarative configs, которые Core может валидировать, сериализовать, сравнивать и восстанавливать через зарегистрированный provider/factory.

Это не plugin system для произвольного кода. Config описывает только экземпляр **заранее зарегистрированного Core component type**.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/declarative_runtime_components.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./106-1-declarative-runtime-components.md)
- [Этап 2 — runtime-интеграция и recovery](./106-2-declarative-runtime-components.md)
- [Этап 3 — IPC, client projection и UI](./106-3-declarative-runtime-components.md)
- [Этап 4 — verification, release-evidence и закрытие](./106-4-declarative-runtime-components.md)

## Зависимости

### Блокирующие

- План 67.0 — Schema-Driven Agent Configuration: Core-owned schemas для agent/conversation settings.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 74.0 — Declarative Agent Component Registry: versioned runtime components with schema-safe loading.
- План 85.0 — Customization Inventory: единый каталог Skills, Integrations, Profiles, Workflows и UI Extensions.
- План 103.0 — Stateful Tool Workbench Sessions: lifecycle, shared state и snapshot для tool collections.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- config является data, не code;
- provider implementation только из Core registry;
- imported config не устанавливает executable provider;
- secrets только refs;
- migrations pure/side-effect free;
- rehydration повторно валидирует current policy;
- component config не расширяет grants;
- unknown provider fail-closed.

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

- [ ] Есть общий versioned ComponentConfig envelope.
- [ ] Есть Core Component Registry/provider descriptor.
- [ ] Config schema валидируется до instantiation.
- [ ] DefinitionConfig отделён от RuntimeState и SecretBindings.
- [ ] Есть explicit deterministic migrations.
- [ ] Rehydration повторно проверяет current policy/capabilities.
- [ ] Config hashes пригодны для provenance/evals.

## Ограничения и non-goals

- arbitrary Python/.NET/Rust class loading по имени из config;
- remote executable plugin marketplace;
- сериализация полного process/model memory;
- secrets внутри portable JSON;
- generic reflection-driven UI для всех компонентов;
- доверие старой config в обход новых security policies.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#86 Declarative Runtime Components: versioned component config, provider registry и safe rehydration](https://github.com/rkfsociety/EvoHime/issues/86)
