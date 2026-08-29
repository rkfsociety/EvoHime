# План 67.0 — Schema-Driven Agent Configuration: Core-owned schemas для agent/conversation settings

Статус: предложено по [issue #47](https://github.com/rkfsociety/EvoHime/issues/47). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Schema-Driven Agent Configuration**: Core публикует versioned schema доступных настроек агента, conversation/run и backend-specific опций, а desktop UI строит формы и validation на основании этой authoritative схемы вместо разрастания hardcoded экранов под каждую новую модель, агент, backend или capability.

Главный принцип:

> Core определяет допустимые настройки и их semantics; UI отвечает за отображение.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/schema-driven-agent-configuration.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- Специфических межплановых blocking зависимостей нет; используется текущий Core/IPC фундамент проекта.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 44.0 — Tool Simulation Runtime: fixture/emulated dry-run без реальных side effects.
- План 47.0 — Skill Trust Pipeline: deterministic scanning, contextual review и quarantine перед активацией.
- План 80.0 — Project Instruction Stack: conditional rules, AGENTS.md compatibility и deterministic precedence.
- План 111.0 — Project Guidance Registry: scoped coding conventions и read-only instruction layers.
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

- [ ] Core публикует versioned configuration schemas.
- [ ] UI не является source of truth для допустимых runtime settings.
- [ ] Есть layered defaults/overrides и effective snapshot.
- [ ] Dynamic references идут через Core registries.
- [ ] Secret settings не возвращаются renderer как raw values.
- [ ] Apply/restart semantics формализованы.
- [ ] Active run фиксирует immutable effective config snapshot/hash.

## Ограничения и non-goals

- arbitrary UI code из backend schema;
- JSON Schema как permission system;
- хранение credentials в обычном settings JSON;
- live mutation running workflow/agent snapshot;
- превращение каждой внутренней Core-константы в пользовательскую настройку;
- web-specific form framework как часть Core contract.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#47 Schema-Driven Agent Configuration: Core-owned schemas для agent/conversation settings](https://github.com/rkfsociety/EvoHime/issues/47)
