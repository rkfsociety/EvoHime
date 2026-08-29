# План 80.0 — Project Instruction Stack: conditional rules, AGENTS.md compatibility и deterministic precedence

Статус: предложено по [issue #60](https://github.com/rkfsociety/EvoHime/issues/60). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Project Instruction Stack**: Core-owned механизм загрузки, нормализации и детерминированного применения долговременных инструкций проекта с поддержкой глобальных пользовательских правил, workspace-local rules, совместимого `AGENTS.md` и условной активации по path/context.

Это отдельная сущность от Agent Skills.

```text
Skill = переиспользуемая процедура/знание, загружаемая когда нужна
Project Rule = ограничение/соглашение проекта, которое должно применяться автоматически в подходящем scope
```

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/project-instruction-stack.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- Специфических межплановых blocking зависимостей нет; используется текущий Core/IPC фундамент проекта.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 60.0 — Revision-Safe Workspace Files: uploads/workspace/outputs namespaces и stale-write protection.
- План 64.0 — Workspace Bootstrap Manifest: безопасная подготовка project environment перед agent run.
- План 111.0 — Project Guidance Registry: scoped coding conventions и read-only instruction layers.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- project rules никогда не расширяют capabilities;
- Markdown не является executable config;
- shell/code blocks не исполняются автоматически;
- system/security policies всегда вне обычного precedence stack;
- path matching использует canonical workspace-relative paths;
- symlink/reparse escape не позволяет подложить instruction file из чужого root;
- imported/untrusted repo rules считаются project content;
- Secret data внутри rule проходит sensitive-data policy;
- model не может выключить/переписать rule через tool без обычных file permissions и visible diff.

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

- [ ] Есть Core-owned ProjectRule registry/discovery.
- [ ] Поддержаны global, workspace, nested и `AGENTS.md` sources.
- [ ] Есть path-based conditional activation.
- [ ] Active instruction stack вычисляется детерминированно и hash-ится.
- [ ] Rule revisions фиксируются в model/run provenance.
- [ ] Context budget для rules bounded и observable.
- [ ] Rules не расширяют capabilities и отделены от Skills/security policy.
- [ ] UI показывает active rules, source и причину активации.

## Ограничения и non-goals

- считать Markdown rules security sandbox;
- исполнять code blocks из instruction files;
- cloud team policy management;
- автоматическая установка skills из rule;
- попытка идеально разрешать любые semantic contradictions LLM-ом;
- загружать все nested rules всего repository в каждый turn.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#60 Project Instruction Stack: conditional rules, AGENTS.md compatibility и deterministic precedence](https://github.com/rkfsociety/EvoHime/issues/60)
