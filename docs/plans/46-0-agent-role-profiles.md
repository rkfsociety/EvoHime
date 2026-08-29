# План 46.0 — Agent Role Profiles: versioned специализация, ограничения и strategy contracts

Статус: предложено по [issue #26](https://github.com/rkfsociety/EvoHime/issues/26). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime versioned **Agent Role Profile**: переиспользуемое описание специализации агента, которое отделяет «кто этот агент и как он должен работать» от конкретного workflow run, prompt и child instance.

Role Profile должен определять:

- профиль/назначение;
- цель роли;
- ограничения;
- допустимые actions/tools/skills;
- максимальные grants;
- стратегию принятия решений;
- ожидаемые входы/выходы;
- допустимые типы сообщений/артефактов;
- budget defaults;
- human/AI execution mode.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/agent-role-profiles.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- Специфических межплановых blocking зависимостей нет; используется текущий Core/IPC фундамент проекта.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 24.0 — Agent Skills: registry, SKILL.md и progressive disclosure.
- План 65.0 — Team Coordination Policies: pluggable routing for multi-agent collaboration.
- План 79.0 — Team Coordinator: capability-aware delegation, dynamic task routing и managerial validation.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- Role Profile не может расширить parent grants;
- role prompt не является authority;
- tool/skill identities разрешаются через Core registry;
- импортированный workflow не может зарегистрировать новую executable role implementation;
- role version фиксируется на run;
- human role не обходит approvals для side effects;
- model override допускается только в разрешённой model policy.

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

- [ ] Есть versioned AgentRoleProfile.
- [ ] Role описывает objective/constraints/skills/tools/strategy.
- [ ] Effective grants вычисляются Core-side как intersection.
- [ ] Runtime instance фиксирует role version/hash.
- [ ] Output contracts typed/versioned.
- [ ] Поддержан human/AI execution mode.
- [ ] Role profiles можно использовать в child/workflow/team layers.

## Ограничения и non-goals

- свободный marketplace executable agents;
- arbitrary role code из Markdown;
- выдача role profile собственных долгоживущих secrets;
- автоматическое повышение permissions;
- отдельный новый agent runtime вместо существующего Core loop.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#26 Agent Role Profiles: versioned специализация, ограничения и strategy contracts](https://github.com/rkfsociety/EvoHime/issues/26)
