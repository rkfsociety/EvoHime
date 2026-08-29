# План 85.0 — Customization Inventory: единый каталог Skills, Integrations, Profiles, Workflows и UI Extensions

Статус: предложено по [issue #65](https://github.com/rkfsociety/EvoHime/issues/65). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime единый **Customization Inventory**: Core-owned каталог всех устанавливаемых, подключаемых и настраиваемых расширений поведения/возможностей продукта с общей моделью identity, source, scope, version, enablement, compatibility, trust и health.

Каталог объединяет управление, но **не объединяет runtime semantics** разных сущностей.

Примеры типов:

```text
Skill
Integration
McpServer
AgentProfile
RoleProfile
WorkflowTemplate
UiExtension
PolicyProfile
```

Главный принцип:

> Пользователь должен видеть в одном месте, что именно расширяет EvoHime, откуда это взялось и активно ли оно, но Core по-прежнему обрабатывает каждый тип через его собственный authoritative subsystem.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/customization-inventory.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./85-1-customization-inventory.md)
- [Этап 2 — runtime-интеграция и recovery](./85-2-customization-inventory.md)
- [Этап 3 — IPC, client projection и UI](./85-3-customization-inventory.md)
- [Этап 4 — verification, release-evidence и закрытие](./85-4-customization-inventory.md)

## Зависимости

### Блокирующие

- План 24.0 — Agent Skills: registry, SKILL.md и progressive disclosure.
- План 33.0 — Integration Provider SDK: единый контракт auth, actions, webhooks и test fixtures.
- План 46.0 — Agent Role Profiles: versioned специализация, ограничения и strategy contracts.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 65.0 — Team Coordination Policies: pluggable routing for multi-agent collaboration.
- План 74.0 — Declarative Agent Component Registry: versioned runtime components with schema-safe loading.
- План 80.0 — Project Instruction Stack: conditional rules, AGENTS.md compatibility и deterministic precedence.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- единый UI не означает единый privileged installer;
- owner subsystem остаётся authority;
- inventory action не расширяет grants;
- enable одного type не включает соседний type;
- source/revision pinning сохраняется;
- secret config не попадает в index/search/events;
- backend/workspace caches scoped;
- recommended != trusted;
- remote package metadata считается untrusted display data;
- destructive uninstall следует subsystem policy и dependency checks.

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

- [ ] Есть normalized CustomizationItem contract.
- [ ] Skills/Integrations/MCP/Profiles/Workflows/UI Extensions можно увидеть в одном inventory.
- [ ] Runtime semantics/actions остаются у owner subsystems.
- [ ] Scope/trust/compatibility/health/update представлены единообразно.
- [ ] Есть dependency visibility без auto-install magic.
- [ ] Workspace/backend switching не смешивает inventories.
- [ ] UI имеет единый Customize catalog с filters/search/details.

## Ограничения и non-goals

- публичный marketplace;
- рейтинг/отзывы/монетизация;
- один универсальный plugin runtime;
- автоматическая установка recommended items;
- включение всего catalog в system prompt;
- generic installer, обходящий subsystem-specific trust/security;
- SaaS organization-wide fleet management.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#65 Customization Inventory: единый каталог Skills, Integrations, Profiles, Workflows и UI Extensions](https://github.com/rkfsociety/EvoHime/issues/65)
