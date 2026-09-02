# План 118.0 — Persistent Agent Organization Registry: durable identities, reporting hierarchy и accountability

Статус: предложено по [issue #98](https://github.com/rkfsociety/EvoHime/issues/98). Это обзорный план направления; реализация начинается после отдельного evidence review. Закрытие issue означает перенос требований в этот исполнимый план, а не готовность функционала.

## Цель

Добавить в EvoHime Core-owned **Persistent Agent Organization Registry** — durable каталог логических agent identities, сохраняющихся между conversations/runs и имеющих versioned role assignment, reporting relationship, responsibility scope и typed связи с Persistent Goals.

Registry отвечает на вопрос «кто отвечает за эту работу и кому она организационно делегирована?», но не заменяет Agent Role Profile, concrete child/run, TeamSession или Goal. Persistent identity не означает постоянно работающий процесс.

## Текущее основание и граница

В checkout уже существуют Persistent Goals, Agent Role Profiles, Team SOP/TeamSession, Team Resource Budget, typed handoffs, coordination policies, task/run state и Artifact Handoff Registry. Новый слой связывает эти authority через stable identity и bounded projections, не создавая второй task database, goal state, cost ledger, scheduler, runtime или permission hierarchy.

Кандидатные поверхности: `crates/evohime-core/src/persistent_agent_registry.rs`, local-storage store/migration, existing goal/role/team/handoff integration, authenticated desktop IPC, Electron main/preload/renderer и canonical docs. Точные имена, schema revision и IPC tags подтверждаются на evidence freeze по live checkout.

## Граница сущностей

```text
AgentRoleProfile = reusable specialization/template
PersistentAgent = durable logical worker identity
RoleInstance/child run = concrete execution instance
TeamSession = bounded team execution snapshot
PersistentGoal = durable objective and success authority
```

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./118-1-persistent-agent-organization-registry.md)
- [Этап 2 — runtime-интеграция и recovery](./118-2-persistent-agent-organization-registry.md)
- [Этап 3 — IPC, client projection и UI](./118-3-persistent-agent-organization-registry.md)
- [Этап 4 — verification, release-evidence и закрытие](./118-4-persistent-agent-organization-registry.md)

## Зависимости

### Блокирующие

- Persistent Goals v1 и их Core-owned objective/state/budget authority.
- Agent Role Profiles v1 и exact profile revision/capability policy.
- Team SOP/TeamSession, Team Coordination Policies, Typed Agent Handoff и Team Resource Budget.
- Existing Core capability/policy/approval, task/run state, event journal, SQLite backup/migrations, authenticated IPC и artifact registry.

### Опциональные

- Model Purpose Routing и Execution Backend Registry — выбор backend/profile для будущего run.
- Diagnostics & Support Bundle — расширенный redacted activity export.
- Scheduler/Event Trigger Runtime — поздняя адресация existing identity без второго scheduler в Registry.

## Основной контракт направления

Core вводит versioned durable `PersistentAgent` с identity, revision, display name, role profile ref, organization scope (`Application`, `Workspace`, `WorkspaceSet`, `Project`), optional reporting parent, responsibility scopes, default goal bindings, policy refs, lifecycle status и content hash.

Lifecycle: `Draft → Active → Paused|Suspended → Active` и `→ Retired`; retired identity сохраняет history, но не выбирается для новых assignments. `Paused` блокирует новые autonomous assignments, `Suspended` требует explicit Core/policy recovery.

`AgentGoalBinding` содержит exact goal revision и роль `Owner|Contributor|Reviewer`; Goal остаётся authority по objective, success criteria, state и budget. `AgentAssignment` связывает identity с обычным task/run, goal, TeamSession или handoff и не исполняет model/tool call самостоятельно.

Reporting graph acyclic, scope-compatible и auditable. При запуске Core фиксирует immutable `ExecutionAgentSnapshot` с agent/profile/policy/goal/accountability projection; поздняя смена reporting, role или responsibility не мутирует активный run.

Availability разделяется: durable `OrganizationalStatus` и derived `ExecutionAvailability` (`Ready`, `Busy`, `Waiting`, `Blocked`, `RuntimeUnavailable`). Activity, cost, progress и artifacts только агрегируются из существующих authoritative subsystems с source refs, time window и profile metadata.

## Безопасность и non-goals

Persistent identity, reporting line, responsibility и goal ownership не являются capability grants. Supervisor не наследует credentials/filesystem grants подчинённого, а подчинённый — supervisor-а. Model/renderer не могут создать identity, повысить роль, снять suspension или подделать status/history. Credentials не хранятся в Agent record; effective grants повторно вычисляются существующей policy.

Не входят HR/SaaS/multi-tenancy, постоянный процесс на каждого агента, новый scheduler/heartbeat, отдельный task/goal/cost ledger, role-based permission hierarchy, marketplace и магический employee score.

## Критерии готовности направления

- [ ] Durable/versioned `PersistentAgent` отделён от RoleProfile и runtime RoleInstance.
- [ ] Reporting graph acyclic, scope-safe, revisioned и сохраняет history.
- [ ] Есть typed Goal owner/contributor/reviewer bindings на exact Goal revision.
- [ ] Assignment и execution snapshot связывают identity с обычными runs/teams без второго runtime.
- [ ] Role/profile/reporting changes не мутируют active execution.
- [ ] Activity/usage/artifact projection переиспользует existing ledgers and stores.
- [ ] Organizational status отделён от actual execution availability.
- [ ] Restart восстанавливает identities/revisions и reconciles transient Busy/assignments.
- [ ] IPC/UI остаются Core projection-only и не расширяют capabilities.

## Связанный issue

- [#98 Persistent Agent Organization Registry](https://github.com/rkfsociety/EvoHime/issues/98)
