# План 111.0 — Project Guidance Registry: scoped coding conventions и read-only instruction layers

Статус: предложено по [issue #91](https://github.com/rkfsociety/EvoHime/issues/91). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Project Guidance Registry**: versioned Core-owned механизм discovery, normalization и безопасной загрузки project-specific правил работы с кодом, которые должны стабильно учитываться агентом, но не являются Skills, tool capabilities или executable bootstrap instructions.

Примеры guidance:

- coding conventions;
- preferred libraries;
- naming/style rules;
- архитектурные ограничения;
- directories/files, которые нельзя менять без причины;
- project-specific test expectations;
- требования к документации/commit messages;
- локальные инструкции для конкретного subtree.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/project_guidance_registry.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./111-1-project-guidance-registry.md)
- [Этап 2 — runtime-интеграция и recovery](./111-2-project-guidance-registry.md)
- [Этап 3 — IPC, client projection и UI](./111-3-project-guidance-registry.md)
- [Этап 4 — verification, release-evidence и закрытие](./111-4-project-guidance-registry.md)

## Зависимости

### Блокирующие

- План 80.0 — Project Instruction Stack: conditional rules, AGENTS.md compatibility и deterministic precedence.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 24.0 — Agent Skills: registry, SKILL.md и progressive disclosure.
- План 47.0 — Skill Trust Pipeline: deterministic scanning, contextual review и quarantine перед активацией.
- План 85.0 — Customization Inventory: единый каталог Skills, Integrations, Profiles, Workflows и UI Extensions.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- project guidance ниже system/user security policy;
- guidance никогда не расширяет grants;
- repository content не становится system prompt;
- trust связан с exact hash/workspace;
- source path canonicalized;
- secret files не auto-discover как guidance;
- guidance edits проходят обычный revision-safe write path;
- imported workflow/skill не может silently register new persistent project guidance;
- prompt injection в обычном source file не повышается до guidance class.

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

- [ ] Есть ProjectGuidanceDocument registry и discovery adapters.
- [ ] Guidance имеет explicit scope/hash/trust/provenance.
- [ ] Effective guidance resolution deterministic и hierarchical.
- [ ] Project rules отделены от Skills/Bootstrap/ReferenceData.
- [ ] Guidance не может повышать security authority.
- [ ] Context projection bounded и target-aware.
- [ ] Prompt Cache Planner может стабильно cache-ить guidance projection.
- [ ] UI показывает sources/scope/conflicts/revision.

## Ограничения и non-goals

- считать любой Markdown инструкциями;
- executable project policy language;
- отмена Core security rules через repository file;
- автоматическое редактирование conventions агентом;
- синхронизация conventions через публичный marketplace;
- semantic theorem prover для любых противоречивых правил;
- превращение Project Guidance в универсальную замену Skills.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#91 Project Guidance Registry: scoped coding conventions и read-only instruction layers](https://github.com/rkfsociety/EvoHime/issues/91)
