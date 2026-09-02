# План 118.2 — Persistent Agent Organization Registry: runtime-интеграция и recovery

Статус: этап 2 для [плана 118.0](./118-0-persistent-agent-organization-registry.md); после [плана 118.1](./118-1-persistent-agent-organization-registry.md).

## Цель

Разрешать persistent identity в обычные assignments/runs/team participation, фиксировать accountability snapshot, строить derived availability/activity и безопасно восстанавливать registry после restart.

## Зависимости

### Блокирующие

- План 118.1 — contract, storage, revisions, reporting and binding validation.
- Existing run/child/team runtime, Role Profiles, Goal, coordination/handoff, policy/capability/approval, budget, artifact and event systems.

### Опциональные

- Model Purpose Routing/Backend Registry для выбора текущего profile/model.
- Scheduler/Event Trigger Runtime для future activation path.

## Реализация

1. Реализовать Core commands/service для create/revise/activate/pause/suspend/retire, change reporting/responsibility, bind/unbind goal, create/cancel assignment и read history. Every mutation validates actor, scope, expected revision and policy.
2. Реализовать reporting graph admission: reject self/cyclic edge, incompatible organization scope and retired target; preserve old graph in active execution snapshots.
3. Реализовать assignment resolver: Active identity + current RoleProfile revision + purpose/model policy + workspace/project policy + Team/Goal constraints → immutable `ExecutionAgentSnapshot`. PersistentAgent не получает прямой model/tool authority.
4. Интегрировать child runs and TeamSession roster slots so one identity can participate in many runs/sessions while each execution pins exact identity/profile/goal/accountability revisions.
5. Реализовать Goal binding checks: Owner/Contributor/Reviewer permissions remain existing Goal policy; binding only attributes responsibility and routing, never grants Goal mutation.
6. Реализовать derived availability/activity: inspect live run/runtime state for Ready/Busy/Waiting/Blocked/RuntimeUnavailable; aggregate assignments, goals, teams, artifacts, usage/cost and blockers from existing stores with bounded windows and source refs.
7. Реализовать delegation/escalation adapters for coordination/handoff. Reporting relationship is an input to route selection, but effective route remains TeamProtocol/policy-approved and does not transfer secrets or grants.
8. Реализовать recovery: restore revisions/edges/bindings, reconcile assignments with actual run state, clear stale Busy, preserve retired history, report missing Profile revision as broken binding and revalidate graph before exposure.

## Fault/recovery matrix

- duplicate create/assignment → idempotent existing result, no second run;
- stale revision → typed conflict, no partial mutation;
- cycle/cross-scope/retired parent → rejected without graph change;
- profile revision removed/unavailable → explicit broken binding, no guessed fallback;
- Core restart while assignment starts → unknown/pending outcome reconciled from run ledger;
- run crash/timeout → availability derived as Blocked/RuntimeUnavailable, not false Ready;
- registry change during active run → old snapshot remains pinned;
- missing activity source → bounded partial projection with source warning, no invented metrics.

## Критерии выхода

- [ ] Identity resolves into ordinary run/team paths and never creates a second runtime.
- [ ] Exact agent/profile/goal/accountability snapshot is pinned per execution.
- [ ] Multiple runs/TeamSessions share stable identity safely.
- [ ] Reporting/goal changes affect only eligible future assignments.
- [ ] Availability and activity are derived from authoritative state and recover after restart.
- [ ] Duplicate/stale/unknown outcomes do not create duplicate work or false success.

## Не входит

New scheduler/daemon, direct external-agent execution, new budget/cost ledger, capability inheritance и arbitrary model-selected organizational mutations.
