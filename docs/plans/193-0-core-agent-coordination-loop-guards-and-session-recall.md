# План 193.0 — Core-owned agent coordination, loop guards и session recall

Статус: active implementation contract. Направление расширяет существующие Core-owned agent/team/context owners; этот документ не означает, что новые гарантии уже реализованы.

## Цель
Повысить надёжность автономной работы Евы: ограничивать повторяющиеся и бесполезные tool loops, координировать делегированные задачи между агентами и возвращать релевантный подтверждённый контекст предыдущих сессий. Результаты должны быть наблюдаемыми, bounded и объяснимыми; делегирование и извлечение контекста не дают агенту дополнительных прав.

## Основание checkout
Уже существуют Persistent Agent Organization Registry (schema 92), Agent Role Profiles/Team SOP и immutable TeamSession snapshots, Causal Collaboration Bus, child handoff, Context Namespace, Memory Views & Adaptive Recall, conversation/event history, durable background execution и workflow runtime. План использует их как источники истины. Он не создаёт второй team registry, collaboration bus, memory/session store, scheduler или tool dispatcher.

## Граница
- Loop guard работает в Core вокруг фактического agent/tool execution и использует observed events, canonical tool identity/args hash, state/evidence revisions и существующие budgets.
- Coordination использует существующие assignments, team protocol, TeamSession, child handoff, collaboration messages, workflow runtime и A2A bridge (план 180) в пределах текущих grants.
- Session recall — read-only query facade над conversation, EventJournal, ArtifactStore и memory/context owners. Результаты содержат provenance refs и redacted summaries, а не полный transcript, secrets или hidden reasoning.

## Изменяемые контракты
- Versioned agent-execution-guard/v1: loop signatures, progress evidence, bounded per-agent/task budgets и typed outcomes.
- Coordination contract: task/dependency identity, assignment lease/revision, handoff state, result/evidence refs и deterministic merge policy.
- session-query/v1: scoped query, bounded result count/size, time range, provenance, trust/authority metadata и explicit unavailable/partial states.
- Redacted IPC projections для guard/coordination/query status; UI не владеет состоянием и не принимает policy decisions.

## Зависимости

### Блокирующие
- ToolRegistry, PermissionEngine, durable approval/receipts, EventJournal и фактический execution lifecycle.
- Persistent Agent Organization Registry, Team SOP/TeamSession, Causal Collaboration Bus и child handoff.
- Context Namespace, Memory Views & Adaptive Recall, conversation/event history и ArtifactStore.
- Durable Background Execution Plane (план 132) и workflow runtime.
- A2A bridge (план 180) только для уже поддерживаемых external-agent flows.

### Опциональные
- План 184 усиливает regression evidence; deterministic loop guards не должны зависеть от LLM evaluator.
- План 185 может потреблять guard outcomes для multi-turn scenarios.
- Remote A2A providers не обязательны: локальная координация должна работать без сети.

## Recovery и rollback
Unknown/partial execution trace не считается прогрессом. После рестарта running assignments сверяются с существующими leases/generation fencing и переходят в typed unknown_after_restart/interrupted, без blind retry. Дубликат handoff не создаёт повторный эффект: используются существующие idempotency keys и source revisions. Guard может остановить/приостановить только текущий run; он не отменяет внешние эффекты и не повышает права. Rollback отключает новые guard/query projections и coordinator policy profile, сохраняя исходные records.

## Этапы
- [Этап 1 — contracts и deterministic guard/query core](./193-1-core-agent-coordination-loop-guards-and-session-recall.md)
- [Этап 2 — runtime coordination, delegation и recovery](./193-2-core-agent-coordination-loop-guards-and-session-recall.md)
- [Этап 3 — IPC, agent context и UI projections](./193-3-core-agent-coordination-loop-guards-and-session-recall.md)
- [Этап 4 — verification, release evidence и closure](./193-4-core-agent-coordination-loop-guards-and-session-recall.md)

## Критерии готовности
- [ ] Повтор одинакового tool+canonical args, повторяющиеся ошибки, oscillation и отсутствие подтверждённого прогресса обнаруживаются детерминированно; false-positive cases имеют явные исключения.
- [ ] Per-task/per-agent/global budgets bounded и не обходятся делегированием.
- [ ] Coordinator делегирует только зарегистрированным агентам через существующие assignment/handoff/team contracts.
- [ ] Результаты агентов проходят provenance, revision и conflict checks до объединения.
- [ ] Session query возвращает scoped, bounded и provenance-backed результаты; partial history не выдаётся за отсутствие фактов.
- [ ] Не создан новый store, scheduler, team roster, tool dispatcher или permission owner.
- [ ] IPC/UI показывают status/reason/evidence refs, не raw prompts, secrets, transcript bodies или hidden reasoning.

## Non-goals
- Неограниченный self-spawning, бесконечная параллельность или автономное изменение grants/policies.
- LLM-as-judge как обязательный runtime guard.
- Автоматическое повторение ambiguous external effects.
- Новый memory/session database или полная копия истории в prompt.
- Передача полномочий между агентами без отдельного Core authorization.

## Verification и release evidence
Плановые gates: canonical signature/progress fixtures → loop/recovery и idempotency tests → multi-agent conflict/delegation tests → scoped session query/privacy tests → IPC/UI bounds и Windows acceptance. Хранить contract/policy versions, hashes signatures/query schema, redacted outcome codes, assignment/handoff refs, provenance refs и focused CI results. Не сохранять raw prompts, chain of thought, credentials, transcript bodies или absolute paths.

## Критерии выхода overview
- [ ] Ownership и failure semantics не дублируют действующие Core owners.
- [ ] Guard fail-closed при неопределённом прогрессе и не скрывает partial state.
- [ ] Delegation, query и UI остаются bounded и не расширяют grants.
