# План 118.3 — Persistent Agent Organization Registry: IPC, client projection и UI

Статус: этап 3 для [плана 118.0](./118-0-persistent-agent-organization-registry.md); после [плана 118.2](./118-2-persistent-agent-organization-registry.md).

## Цель

Дать Electron bounded Operations/Agents projection для persistent identities, reporting hierarchy, goals, assignments, availability, history и derived accountability без переноса authority в renderer.

## Зависимости

### Блокирующие

- План 118.2 — commands, events, recovery and stable projection types.
- Authenticated desktop IPC, sequence replay/resync, generated TypeScript protocol, main/preload bridge и existing Operations/Goal/Team navigation.

### Опциональные

- Diagnostics bundle для расширенных source-linked metrics.
- Context refs для ссылок на agent/goal/assignment.

## Реализация

1. После проверки highest tag зарезервировать additive protocol commands/results/events для list/get/history, lifecycle/revision, reporting, goal binding, assignment, availability и activity; сохранить major, bounds, correlation, idempotency и replay.
2. Core принимает identity/ref/revision inputs, но заново проверяет scope, actor, policy, role/profile/goal revision, reporting cycle и lifecycle. Renderer не передаёт capability grants и не утверждает trusted state.
3. Передавать metadata-only projection: stable id/revision/name, role ref/version, scope/status, parent chain, responsibility labels, goal refs, assignment/run refs, derived availability, bounded metrics/source refs, blockers и warnings. Не передавать credentials, prompts, outputs, transcripts или hidden reasoning.
4. Связать `ipc_bridge.rs`, shared API, preload и main bridge с reconnect/replay/resync и optimistic revision conflict. Unknown/pending assignment явно отображается и не превращается в success.
5. Добавить Operations → Agents: список/поиск, карточка identity, role/profile revision, status controls, reporting hierarchy, goal ownership, active assignments, availability, blockers, recent artifacts/results и bounded derived usage.
6. Добавить explicit history/diff view для role/reporting/responsibility/scope/status revisions и lightweight org tree. Click-through к Goal/Run/TeamSession использует existing routes/refs, не копирует их state.
7. Показывать `Active` отдельно от `Ready/Busy/Waiting/Blocked/RuntimeUnavailable`; broken binding, stale data и denied scope должны иметь доступные текстовые warnings.

## Acceptance-to-projection matrix

- `C01` Agents → durable identity, revision, role, scope and lifecycle.
- `C02` Organization → bounded acyclic hierarchy and exact history.
- `C03` Goals/work → owner/contributor/reviewer bindings and assignments.
- `C04` Availability → actual derived run/runtime status, not editable presence.
- `C05` Accountability → source-linked activity/cost/artifact summaries without duplicate ledgers.
- `C06` Security → authenticated/replayed projection and no renderer-forged identity/grants/status.

## Критерии выхода

- [ ] IPC additive, authenticated, bounded, idempotent and replay-safe.
- [ ] Mutations are Core-validated and return typed stale/denied/broken outcomes.
- [ ] UI remains projection-only and reflects reconnect/recovery state.
- [ ] Sensitive content and duplicate authoritative state are absent from renderer data.

## Не входит

Direct filesystem/SQLite access, client-side org calculations, HR dashboard, scheduler UI и automatic approval of organizational changes.
