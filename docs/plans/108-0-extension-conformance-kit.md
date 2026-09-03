# План 108.0 — Extension Conformance Kit: contract tests и transactional registration для providers/adapters/extensions

Статус: предложено по [issue #88](https://github.com/rkfsociety/EvoHime/issues/88). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Extension Conformance Kit**: единый набор contract tests, fixtures и transactional-registration правил для всех расширяемых runtime surfaces, чтобы новый provider/adapter/extension можно было проверить на совместимость, failure semantics и security invariants до включения в production runtime.

Первый scope:

- Integration Provider SDK (#13);
- External Coding Agent Adapter (#25);
- Runtime Intervention / middleware providers (#17/#49);
- Workbench implementations (#58);
- UI extensions (#56), где применимо;
- будущие declarative component providers (#54).

Это не новый plugin API. Это **общая проверочная инфраструктура** поверх уже существующих контрактов.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/extension_conformance_kit.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./108-1-extension-conformance-kit.md)
- [Этап 2 — runtime-интеграция и recovery](./108-2-extension-conformance-kit.md)
- [Этап 3 — IPC, client projection и UI](./108-3-extension-conformance-kit.md)
- [Этап 4 — verification, release-evidence и закрытие](./108-4-extension-conformance-kit.md)

## Зависимости

### Блокирующие

- План 33.0 — Integration Provider SDK: единый контракт auth, actions, webhooks и test fixtures.
- План 37.0 — Agent Middleware Pipeline: typed hooks вокруг model/tool execution.
- План 45.0 — External Coding Agent Adapter: подключение Codex/Claude/Gemini-подобных executors через typed protocol.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 69.0 — Runtime Intervention Pipeline: Core-owned middleware for agent messages and tool boundaries.
- План 74.0 — Declarative Agent Component Registry: versioned runtime components with schema-safe loading.
- План 76.0 — Safe UI Extension Framework: declarative pages, panels и themes без renderer authority.
- План 106 — закрытый Declarative Runtime Components: versioned component config, provider registry и safe rehydration; используется как готовый runtime contract.
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

- [ ] Есть versioned common conformance harness.
- [ ] Registration transactional и rollback тестируется.
- [ ] Instance isolation проверяется отдельно от provider identity.
- [ ] API compatibility fail-closed для неподдерживаемых versions.
- [ ] Disabled path гарантированно не создаёт runtime side effects.
- [ ] Есть deterministic fault injection.
- [ ] Security assertions входят в обязательный suite.
- [ ] Минимум IntegrationProvider, ExternalAgentAdapter и Workbench имеют specialized suites.
- [ ] ConformanceReport machine-readable и hash/version-bound.

## Ограничения и non-goals

- публичная certification authority;
- marketplace badge как гарантия безопасности;
- запуск реальных production credentials/services в CI;
- один гигантский suite для совершенно разных extension kinds;
- автоматическая выдача trust/capabilities после passing tests;
- fuzzing всех внешних SDK как обязательное условие MVP.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#88 Extension Conformance Kit: contract tests и transactional registration для providers/adapters/extensions](https://github.com/rkfsociety/EvoHime/issues/88)
