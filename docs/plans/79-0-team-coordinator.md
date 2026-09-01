# План 79.0 — Team Coordinator: capability-aware delegation, dynamic task routing и managerial validation

Статус: предложено по [issue #59](https://github.com/rkfsociety/EvoHime/issues/59). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Team Coordinator**: отдельную orchestration-роль, которая внутри уже разрешённой team/workflow session может динамически распределять ещё не назначенную работу между зарегистрированными Agent Role Profiles, запрашивать консультации у специалистов, контролировать загрузку и проверять готовность результатов перед handoff.

Coordinator не создаёт новый multi-agent runtime. Он работает поверх существующих/запланированных:

- Agent Role Profiles (#26);
- Team SOP Protocols (#28);
- Retained Child Contexts;
- Causal Collaboration Bus (#31);
- workflow/child grants и Core approvals.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/team_coordinator.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./79-1-team-coordinator.md)
- [Этап 2 — runtime-интеграция и recovery](./79-2-team-coordinator.md)
- [Этап 3 — IPC, client projection и UI](./79-3-team-coordinator.md)
- [Этап 4 — verification, release-evidence и закрытие](./79-4-team-coordinator.md)

## Зависимости

### Блокирующие

- Team Resource Budget v1 — shared cost envelope, per-role allocations и reserved verification budget; контракт перенесён в канонические документы.
- Composable Termination Conditions v1 — реализованный Core-контракт из канонических документов.
- План 65.0 — Team Coordination Policies: pluggable routing for multi-agent collaboration.
- План 66.0 — Typed Agent Handoff Contract: explicit transfer of task ownership and context.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 68.0 — Experience Replay Library: episodic trajectories, success/failure retrieval и context injection.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

Модель Coordinator **предлагает orchestration decisions**, но Core остаётся authority по тому, можно ли их применить.

```text
Team state
 -> Coordinator observation
 -> DelegationProposal
 -> Core validation
 -> assignment / question / review request
 -> ordinary child/team runtime
```

Coordinator не может:

- создать неизвестную роль;
- расширить grants;
- назначить capability, которой нет у role instance;
- обойти required approval;
- увеличить budget сверх parent/team policy.

### Безопасность

- coordinator не является root authority;
- participant identity Core-owned;
- dynamic delegation только внутри roster;
- effective grants всегда пересчитываются Core;
- task decomposition не расширяет parent budget/grants;
- consultation не даёт доступ ко всему context;
- reassignment не переносит secret state автоматически;
- manager validation не заменяет security approval;
- model не может назначить произвольный external agent по строковому имени.

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

- [ ] Есть durable `TeamWorkItem` contract.
- [ ] Coordinator может предлагать dynamic assignments и consultations.
- [ ] Core выполняет capability/output-contract compatibility checks.
- [ ] Есть bounded decomposition/reassignment.
- [ ] Есть load-aware candidate projection.
- [ ] Managerial validation отделена от security/acceptance gates.
- [ ] State восстанавливается после restart без duplicate delegation.
- [ ] UI показывает queue, roster, assignments и escalation.

## Ограничения и non-goals

- новый workflow engine;
- unrestricted manager agent;
- автоматическое создание новых executable roles;
- глобальный marketplace workers;
- dynamic grant elevation;
- бесконечная task decomposition;
- consensus нескольких моделей как замена security approvals.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#59 Team Coordinator: capability-aware delegation, dynamic task routing и managerial validation](https://github.com/rkfsociety/EvoHime/issues/59)
