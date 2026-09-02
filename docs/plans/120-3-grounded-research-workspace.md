# План 120.3 — Grounded Research Workspace: IPC, client projection и UI

Статус: этап 3 для [плана 120.0](./120-0-grounded-research-workspace.md); после [плана 120.2](./120-2-grounded-research-workspace.md).

## Цель

Дать Electron bounded surface для collections, sources, ingestion/research progress, evidence/citations, conflicts, coverage, artifact acceptance и rerun без переноса provenance authority в renderer.

## Зависимости

### Блокирующие

- План 120.2 — stable source/session/artifact commands/events and recovery.
- Authenticated desktop IPC, sequence replay/resync, generated TypeScript protocol, main/preload bridge, ArtifactStore/evidence open route и existing Project/Operations navigation.

### Опциональные

- ContextRefs, browser reading mode, Diagnostics Bundle и export delivery.

## Реализация

1. После проверки highest tag зарезервировать additive commands/events/results для collection/source list/get/add/remove/refresh, evidence query/open, research start/stop/resume, artifact get/accept/promote/rerun/compare and status/history. Preserve major, bounds, correlation, idempotency and replay.
2. Core validates scope, source revision, locator, connector/network policy, model/tool budget, artifact state and actor on every mutation. Renderer cannot choose authoritative revision, forge citation or mark claim verified.
3. Передавать metadata-only projections: source kind/origin/trust/hash/revision/status/locator summary, collection state, session/subtask progress, coverage, citation validation, conflict refs, artifact revision/hash and bounded text excerpts only when explicitly authorized.
4. Связать `ipc_bridge.rs`, shared API, preload/main adapters and reconnect/resync. Unknown acquisition/validation/acceptance visibly differs from success; raw credentials/prompts/outputs/source corpus stay out of IPC.
5. Добавить Project → Research: Collections (source count/state/refresh/failures), Sources (revision/parser/trust/open original/newer revision), Research setup (objective/mode/source policy/budget), progress and stop/resume.
6. Добавить Result view: report/notes, inline citation markers, sources panel, claim unsupported/uncertain badges, coverage/conflicts, exact evidence navigation, Accept/Save to Project, rerun latest and compare revisions.
7. Add optional document reading mode (original page/section + research) over evidence locator, not a second source truth. Add bounded manual notes explicitly distinguished from extracted evidence.
8. Ensure accessibility and stale/denied/local-only/partial/failed states are textual and explicit; renderer never assembles giant corpus or graph.

## Acceptance-to-projection matrix

- `C01` Collections/Sources → immutable revision/trust/status/locator metadata.
- `C02` Research run → mode, selected sources, policy, budget and bounded progress.
- `C03` Result → claims/citations/coverage/conflicts and exact evidence navigation.
- `C04` Reuse/drift → newer revisions, rerun and ResearchDelta.
- `C05` Promotion → Core-mediated accept/save/context refs.
- `C06` Security → authenticated projection, redaction and no forged provenance.

## Критерии выхода

- [ ] IPC additive, authenticated, bounded and replay-safe.
- [ ] Every mutation is Core-validated/idempotent and returns typed stale/denied outcome.
- [ ] UI never computes source identity, citation matching, coverage or research completion.
- [ ] Raw corpus, secrets, prompts, outputs and arbitrary connector payloads are absent.

## Не входит

Direct filesystem/SQLite access, client-side RAG/citation validation, generic document editor, unrestricted source browser и independent research transport.
