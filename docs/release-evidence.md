# EvoHime — release evidence и rollback matrix

Этот документ описывает evidence для поставки. Artifact bundle должен быть
redacted: допускаются commit, contract/schema versions, test IDs, hashes,
typed outcomes, bounded metrics и recovery state; credentials, raw provider
output, transcripts, absolute paths и PII запрещены.

## Текущий статус выпуска

Статус: `TECHNICAL_GATES_PASS / RELEASE_GREEN`.

## Plan 78 — Capability Workbench v1 (2026-09-02)

- Contract: versioned Core-owned descriptor/instance lifecycle, four scopes,
  exclusive/serialized/parallel concurrency, dynamic tool discovery with
  capability recheck, shared-resource availability and typed cancellation.
- Durability/recovery: additive SQLite schema v74 stores instances, safe
  snapshots and resource leases; bounds are 128-byte IDs, 128 tools, 64
  resources, 32 leases/in-flight calls and 256 KiB snapshots. Heartbeat,
  expiry and degraded recovery do not persist OS handles or raw credentials.
- IPC/UI: authenticated additive command 228/event 73 and Electron
  metadata-only Capability Workbench panel; no arbitrary backend, renderer
  state machine, direct DB access or effect execution.
- Checks: focused Core/storage contract tests, cargo fmt/check/clippy,
  protocol generation/check, Electron typecheck, focused panel test and full
  Electron regression. `git diff --check` passed; evidence excludes secrets,
  prompts, raw outputs, handles, absolute paths and PII.

Continuation Policy v1 зафиксирован task-only коммитом `605c5ba1` с последующим
исправлением миграционного порядка и boxed IPC future в текущем task commit:
schema `v36`, authenticated IPC tags `151–156`, Core decision outcomes
`Continue/Complete/PauseForApproval/Blocked/BudgetLimited/StopFailed/StopUser`,
durable idempotency/action dedup, task binding, restart-to-block recovery и
metadata-only typed projection. Regression evidence: fresh-schema continuation
table assertions, 206 local-storage tests, 560 Core tests, 35 desktop-ipc
tests, `cargo fmt --all -- --check`, strict clippy, `npm run check:protocol`,
`npm run typecheck`, continuation panel test и реальный Core E2E (3 tests
passed: handshake, reconnect, workflow template). Prompt, workspace path,
secrets, raw provider output и hidden reasoning не входят в projection.

Последнее evidence для Persistent Goals зафиксировано на task-only коммите
`6d3c1c98` (29 августа 2026 года): `GoalV1`/`GoalStore` v1, SQLite schema
`v33`, canonical SHA-256 hash, immutable revisions/events, Core-minted
user-decision evidence, проверка runtime-ссылок и bounded `projection_truncated`.
Authenticated `desktop-ipc-v1` использует Goal commands tags `142–150` и typed
events oneof `20–22`; клиентские Verify-поля evidence/verifier зарезервированы,
поэтому renderer не может self-attest.

Reproducible gates: `cargo test -p evohime-core -p evohime-local-storage
-p evohime-desktop-ipc` — Core 556, local-storage 204, desktop-ipc 35 passed;
`cargo clippy -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc
--all-targets -- -D warnings`; `cargo fmt --all -- --check`; `cargo check -p
evohime-supervisor`; `npm run check:protocol`; `npm run typecheck`; `npm test
-- --run` — 60 files, 469 passed, 2 skipped; `git diff --check`. Focused
evidence includes `persistent_goal_ipc_is_typed_bounded_and_recoverable`,
`recovery_reports_missing_link_without_retrying_any_effect`, all Goal storage
contract/migration/corruption tests, typed pipe projection and GoalPanel tests.
No credentials, raw provider output, prompts, hidden reasoning, absolute paths
or PII enter the Goal projection/release evidence.

Свежая проверка запускается `scripts/final-release-audit.tests.ps1` и включает
Rust Core/storage/IPC tests, rustfmt, automation boundary, backup/restore и
redaction gates, Electron protocol и typecheck. Полный локальный прогон 26
августа 2026 года также подтвердил строгий `cargo clippy`, Electron `npm test`
(457 passed, 2 skipped), production build и bundle checks. Compatibility,
native-package, installer и upgrade/rollback gates проходят в Windows CI.
Documentation gate проверяет все tracked text-файлы, относительные Markdown-ссылки
и запрет устаревших удалённых audit-документов.

Code signing не входит в текущий release scope; manifest/hash остаётся
документированным trust root. Optional browser/voice/vision adapters работают
через typed `backend_unavailable` и не расширяют базовый runtime.

## Rollback / disable

| Компонент | До изменения | При crash/ошибке | Disable/cleanup | Evidence |
| --- | --- | --- | --- | --- |
| SQLite schema / automation tables | backup с checksum и schema version | restore safety backup; повторить migration только после проверки | удалить только expired snapshots/archive по retention | `evohime-local-storage` backup tests |
| Automation archive | canonical run/events/snapshots JSON с SHA-256 и сроком retention | transaction restores only after checksum and identity validation | `sweep_expired_archives` удаляет только истёкшие archives | `automation_store` archive/restore test |
| Core/supervisor package | полный install backup и transaction journal | transaction worker откатывает staging и очищает journal | остановить компонент, сохранить redacted diagnostic | `electron-fault` и installer rollback smoke |
| User-triggered self-repair update | isolated checkout, bounded diff/tests, commit SHA, CI state и installer marker | health timeout или failed startup возвращает полный backup | repair остаётся failed/recoverable, без повторного push или restart | Electron repair tests, updater health-marker tests, authenticated Core E2E |
| Optional browser/voice/vision adapter | capability manifest/hash и typed availability | `backend_unavailable`, без Core state mutation | disable adapter, remove staging/runtime cache | `decision-register.md`, adapter contract tests |
| Automation simulation | ephemeral state, fake-provider fixture | discard ephemeral state; no production recovery | delete temp workspace after run | automation A05/A06 fixtures |
| Persistent Goal projection | schema v33 backup, immutable Goal revisions/events and Core-owned evidence | restore the pre-v33 safety backup; corrupt/missing references stay typed error or bounded recovery warning; no blind retry | disable by withholding the additive `goals` capability; old clients remain compatible; retain audit rows per Core policy | Goal storage, recovery, IPC and Electron focused tests |
| Persistent Analysis Kernel (implemented, plan 28 closed 2026-08-30) | schema v38 backup, metadata-only session/object manifest, authenticated Core↔supervisor worker channel, ephemeral runtime state and allowlisted worker identity | reset/stop on limit, crash or recovery; crash fencing clears process memory and forbids blind retry; checkpoint/child refs are immutable and selected | disable by withholding additive `analysis_kernel` capability; preserve manifest/audit metadata; optional artifact/tool surfaces fail closed typed unavailable | release-evidence gate, Core 573, storage 212, supervisor 32+2, real packaged worker manifest and Core/supervisor fault smoke passed; evidence redacted |
| Continual Refinement v1 (implemented, plan 29 closed 2026-08-30) | schema v39 backup, bounded candidate/evidence metadata, immutable revisions, SHA-256 content hash, authenticated IPC 166–168 | optimistic stale transition is typed; unsupported Skill/PromptRule activation is `unavailable`; raw body/transcript/credentials never enter projection | disable additive refinement capability; retain bounded audit metadata; no blind activation retry | Core 578, storage 214, desktop IPC 35, Electron 472 passed/2 skipped, strict clippy, protocol/typecheck and release Core build passed; evidence redacted |

| Visual Workflow Builder v1 (implemented, plan 31 closed 2026-08-30) | additive schema v39, typed graph/layout contract, draft/version/handoff metadata, IPC 173/event 33 | optimistic revision and owner-scoped single-use handoff; publish is atomic and immutable; inspection is read-only | withhold `workflow_builder` capability; retain bounded draft/version metadata; no renderer-side dispatch | Core 583, storage 217, desktop IPC regression passed, Electron focused builder tests 2 passed, protocol/typecheck, strict clippy and Core build passed; hash separation and redaction covered |
| Conversational Workflow Composer v1 (implemented, plan 32 closed 2026-08-30) | strict `composer-request/v1`/`composer-proposal/v1`, shared Builder provenance metadata, IPC 174/event 34 | bounded Core gateway call; proposal validation/binding; typed edit/save/handoff; stale owner/revision/hash rejection; no automatic workflow effect | withhold Composer capability; retain only redacted request/proposal/catalog hashes; model unavailable/invalid remains typed failure | Core/storage checks, stale-handoff storage regression, Electron Composer focused test, 474 Electron tests with 2 штатно skipped, protocol/typecheck/build/bundle passed; evidence redacted |
| Invocation Presets v1 (implemented, plan 35 closed 2026-08-30) | immutable version-pinned revisions, sanitizer/hash contract, automation schedule preset snapshot, authenticated IPC 179–180/event 37 | migration preview/commit is explicit; temporary override is run-only; scheduler checks hash/state and uses ordinary WorkflowRuntime; no raw secrets/prompt/output | disable by withholding `invocation_preset`; retain redacted revisions and blocked drift evidence; unknown outcome never retries blindly | Core 600, storage 222, desktop IPC 35, Electron 478 passed/2 skipped, protocol/typecheck, fmt, clippy and scheduler/migration focused tests; evidence redacted |
| Agent Benchmark Matrix v1 (implemented, plan 36 closed 2026-08-31) | schema v42 metadata-only suites/runs/attempts/baselines, Core contract hash, bounded executor and P50/P95/P99 metrics, authenticated IPC 181–182/event 38 | deterministic executor is offline and reproducible; provider mode is explicit and unavailable without configured environment; unknown/skipped/unavailable never pass; baseline promotion is optimistic and explicit | withhold `benchmark_matrix`; retain redacted run metadata; cancel/disable prevents new attempts; no raw prompt/output, credentials, production data or blind external retry | Core benchmark 5/5, storage 1/1, `cargo eval benchmark --attempts 3` and PowerShell smoke redacted JSON report, protocol/typecheck, focused panel test and Electron 480 passed/2 skipped; evidence redacted |
| Agent Middleware Pipeline v1 (implemented, plan 37 closed 2026-08-31) | schema v43 immutable definition/run snapshots, eight typed phases, deterministic ordering, bounded immutable overrides, authenticated IPC 183–184/event 39 | Core rechecks capability snapshot before effect; duplicate is not replayed; blocked/stale/unknown remain typed non-success; raw prompt/output and executable imported code excluded | withhold `agent_middleware_pipeline`; retain only redacted definition/run metadata; transient hook payloads are discarded; no blind retry after unknown | Core contract/recovery 7/7, storage 1/1, desktop IPC 35/35, protocol/typecheck, focused Electron panel 1/1; evidence redacted |
| Adaptive Tool Catalog v1 (implemented, plan 38 closed 2026-08-31) | derived Core catalog from `ToolRegistry` manifests; default 8/hard 32; deterministic and optional semantic/model id validation; process-local cache key includes revision/registry/policy/grant/query/selector/limit | permission preflight is authoritative; unknown/duplicate model ids fail closed; empty/no-match uses deterministic top-ranked fallback; no legacy schema widening; no new SQLite state | withhold catalog projection by returning no authorized loadout; cache disappears on restart; keep existing `model.context` event and journal diagnostics bounded/redacted | Core 613/613, adaptive focused 4/4, Electron focused 1/1 and full 481/483 (2 source-update skipped), protocol/typecheck, fmt/check and diff-check; evidence redacted |
| Sensitive Data Guardrails v1 (implemented, plan 40 closed 2026-08-31) | Core contract v1, deterministic policy hash, bounded redact/mask/hash/block, recursive JSON, cross-chunk streaming, model/tool/stream/trace admission; ephemeral state and no schema migration | precedence and bounds fail closed; private key block, structured traversal, provider/tool redaction and restart discard are covered; permissions/approval/effect ledger remain authoritative | withhold additive `sensitive_data_guardrails`; existing trace remains metadata-only; no blind retry or external DLP dependency | Core 621/621 + recovery 3/3, local storage 224/224, desktop IPC 36/36, Electron 484/486 (2 source-update skipped), protocol/typecheck, fmt/clippy and diff-check; evidence redacted |
| Execution Policy Profiles v1 (implemented, plan 41 closed 2026-08-31) | versioned shared ToolRegistry resolver for `shell.execute`/`process.run`, schema v44 catalog, canonical profile hash, bounded timeout/output, deny-by-default environment, Windows Job Object backend, authenticated IPC 187/event 42 | profile/backend cannot be selected by command text; user env is rejected; required backend fails before dispatch; workspace is canonicalized; handles/output/leases are ephemeral; unknown/unavailable are non-success | withhold profile status/projection on invalid or unavailable backend; preserve existing permission/approval/effect ledger; no blind retry after cancellation/unknown; no credentials/raw output in IPC | runtime profile 2/2, storage catalog 1/1, Core/IPC compile, protocol check/typecheck, Electron focused panel 1/1, fmt/check and diff-check; Windows Job Object smoke is required on Windows release agents; evidence redacted |
| Model Resilience Policy v1 (implemented, plan 42 closed 2026-08-31) | Core contract/hash, normalized failure taxonomy, bounded retry/fallback budgets, allowlisted profile compatibility, ephemeral run overlay, authenticated IPC 188/event 43 | profile fallback rechecks capability/privacy/residency; provider payload stays in adapter; cancellation and dispatch unknown are non-success; schema remains v44; no raw prompt/output/credentials in projection | withhold resilience projection on invalid policy; provenance recovery remains authoritative; restart never blind-retries an external effect; existing routing/gateway authority is reused | Core contract 7/7, Core/IPC check, Electron focused panel 1/1, protocol/typecheck; full regression evidence and exact commands recorded at release; evidence redacted |

Rollback не обещает откат уже совершённых внешних side effects: такие effects
идут через existing receipts/reconciliation и требуют typed unknown outcome.

## Evidence format

Каждый bundle содержит `manifest.jsonl` с полями `evidence_version`, `commit`,
`test_id`, `contract_version`, `environment_class`, `expected_outcome`,
`actual_outcome`, `event_sequence`, `artifact_sha256` и `redaction_status`.
Время и абсолютные пути не являются частью replay hash. Retention: CI evidence
1 день, local diagnostic export 7 дней, automation archive 30 дней, а durable
audit хранится по его собственному Core policy.

## Privacy, egress и maintenance

- базовый package не содержит credentials, Python/Node runtime, model assets,
  public HTTP listener или cloud control plane;
- optional adapter manifest содержит только stable ID/version/hash/license и
  typed availability, никогда не ключи и не raw media;
- `scripts/release-evidence.tests.ps1` проверяет обязательные документы,
  запускает focused backup/restore и redaction tests и запрещённые markers;
- license/attribution inventory ведётся в [`licenses/README.md`](licenses/README.md)
  и обновляется в том же коммите, что и новый distributed artifact.
## Plan 39 — Structured Response Contract v1 (2026-08-31)

- Contract: `evohime-model-gateway::structured_response`, schema v1, bounded
  64 KiB schema and 3 total attempts / 2 repair retries.
- Safety: local Core validation; synthetic output tool is non-capability and
  never reaches ToolRegistry; provenance is redacted; restart does not blind
  retry an unfinished provider request; no SQLite migration.
- IPC/UI: additive authenticated command tag 185 and event tag 40;
  Electron metadata-only panel and bridge.
- Fresh checks: model-gateway structured-response unit test, Core lifecycle
  unit test, `npm run check:protocol`, `npm run typecheck`, focused Vitest,
  `cargo fmt --all -- --check`, and `git diff --check`.

## Plan 42 — Model Resilience Policy v1 (2026-08-31)

- Contract/runtime: `cargo test -p evohime-core model_resilience_policy
  --no-fail-fast`; `cargo test -p evohime-core --test
  model_resilience_policy_contract --no-fail-fast` — 7/7.
- Regression: `cargo test -p evohime-core -p evohime-local-storage
  -p evohime-desktop-ipc --no-fail-fast` — Core 626, storage 225, IPC 36.
- Electron: `npm run check:protocol`, `npm run typecheck`, `npm test` — 486
  passed, 2 штатно skipped; `npm run build`; `npm run check:bundle`.
- Safety: schema remains v44; IPC is authenticated additive 188/event 43;
  policy projection is metadata-only and provenance recovery forbids blind retry
  after dispatch unknown outcome.

## Plan 43 — Execution Backend Registry v1 (2026-08-31)

- Contract: `evohime-core::execution_backend_registry`, version 1; bounded
  lowercase backend ids, canonical HTTPS remote endpoints, typed health/failure
  states, Core-policy capability intersection and immutable run snapshot.
- Storage: additive SQLite schema v45 with metadata-only backend/default/event
  tables; credentials are refs only and remote transport is not installed.
- IPC/UI: authenticated additive command 189/event 44; Electron projection is
  metadata-only. Remote handshake is explicitly `transport_unavailable`; no
  automatic failover or blind retry.
- Focused checks: Core 3/3 registry tests, storage 1/1 focused and 226/226
  storage regression, desktop IPC 36/36, Electron focused panel 1/1,
  `npm run check:protocol`, `npm run typecheck`; full Core regression 629 tests
  passed after the schema assertion update. Evidence contains no credentials,
  raw prompts/outputs or absolute paths.

## Plan 44 — Tool Simulation Runtime v1 (2026-08-31)

- Contract: schema v1, explicit Real/Fixture/Emulated/DryRun modes, deterministic
  tool-id plus SHA-256 input fixture matching, bounded input/output, typed
  synthetic/fixture provenance and Structured Response validation.
- Runtime: Core interception occurs after policy recheck and before any
  ToolRegistry effect adapter. Missing/stale/invalid fixtures fail closed;
  simulation never falls back to Real. Workflow fixture path and
  `FixtureToolBenchmarkExecutor` are provider-free and side-effect-free.
- Recovery/storage: idempotent duplicate delivery is stable; run/fixture/policy
  state is ephemeral and restart discards it. SQLite schema remains v45; no
  migration or durable simulation payload is introduced.
- IPC/UI: authenticated additive command 190/event 45; Electron receives only
  bounded mode/state/provenance/count metadata. Raw fixture/input/output,
  prompts, credentials and executable identities are absent; panel warning is
  always visible.
- Evidence: Core contract 2/2, recovery 1/1, workflow interception 1/1,
  fixture benchmark 1/1, Electron panel 1/1, protocol check and typecheck pass;
  full regression and final commands are recorded with the closing commit.

## Plan 45 — External Coding Agent Adapter v1 (2026-08-31)

- Contract: `evohime.external-agent/v1`, bounded newline-delimited frames,
  manifest/capability handshake, immutable agent snapshot, declared credential
  slots and typed unknown/unavailable outcomes.
- Storage/runtime: additive SQLite schema v46 stores metadata-only presets,
  revisions, conversations and idempotent events; Core sends opaque run specs,
  supervisor resolves allowlisted executables without shell and assigns a
  per-run Job Object. Unknown effects are never blind-retried.
- IPC/UI: authenticated additive commands 191–192/event 46; Electron exposes
  only bounded state, counts, protocol/hash and `core_control_level`; raw frames,
  prompts, outputs, credential values and paths are excluded.
- Fresh checks: Rust focused/full checks, protocol check, TypeScript typecheck,
  focused adapter Vitest, `cargo fmt --all -- --check` and `git diff --check`.

## Plan 46 — Agent Role Profiles v1 (2026-08-31)

- Contract/runtime: bounded versioned profile with typed input/output
  contracts, `human`/`ai` execution mode, canonical SHA-256 hash and runtime
  snapshot pinned to profile revision. Effective grants are Core-side
  `parent ∩ policy ∩ registry ∩ requested`; profile text is not authority.
- Storage/recovery: additive SQLite schema v47 stores metadata-only current
  profiles and immutable revisions; v46→v47 uses backup-before-migrate.
  Runtime instances/proposals are transient; duplicate, stale, cancel and
  unknown outcomes remain non-success and are not blindly retried.
- IPC/UI: authenticated additive commands 193–194/event 47; Electron panel is
  metadata-only and excludes raw prompts, credentials, executable code and
  hidden reasoning.
- Focused checks: Core contract/recovery tests 3/3, `cargo check -p
  evohime-core -p evohime-local-storage -p evohime-desktop-ipc`, `npm run
  check:protocol`, `npm run typecheck`, focused Vitest 1/1. Full regression,
  clippy and Windows-only packaging gates remain required before release.
## План 47 — Skill Trust Pipeline v1

Подтверждено в текущем checkout: offline deterministic scanner и stable
finding codes, redacted fingerprints, hash/version-bound decisions, Core gate
для load/reference/capability selection, optional fail-closed review contract,
metadata-only IPC projection и SQLite schema 48. Focused evidence: `skill_trust_pipeline`
4/4, `skill_registry` 4/4, `skill_trust_pipeline_store` 1/1, protocol check и
SkillCatalogPanel regression. Полные release gates выполняются ниже отдельными
командами и не заменяются этим focused набором.
# Plan 48 — Team SOP Protocols v1 (2026-08-31)

- Core v1 validates bounded role refs, phases, handoffs, review loop and
  completion evidence; TeamSession pins canonical protocol hash.
- SQLite schema v49 stores metadata-only definitions/revisions/sessions.
- Authenticated IPC 195–196/event 48 and Electron metadata-only UI are additive;
  grants remain Core-owned and unknown effects are never blindly retried.
- Focused evidence: Core 2/2, storage 1/1, Electron 1/1, protocol/typecheck,
  fmt and diff-check.

## План 49 — Resumable Conversation Event Log v1 (2026-08-31)

- Contract/storage: envelope v1, 64 KiB payload and 200-event page bounds,
  SQLite schema v50, transactional user-message acceptance, per-conversation
  sequence, stable client id/hash dedup, before/after cursors and logical
  compaction retention metadata.
- Runtime/security: Core projects existing activity into one redacted log;
  authoritative payload never crosses desktop IPC, streaming remains transient,
  finalized/failed is durable, child projection is summary-only and retry never
  repeats a task or unknown effect.
- IPC/UI: authenticated additive commands 197–198/event 49 and StartTask fields
  6–7; Electron detects gaps/conflicts/duplicates, reconciles only by client id,
  resumes after the last sequence and exposes sending/retry/failed state.
- Evidence: storage conversation tests 7/7, Core conversation tests 6/6,
  idempotent StartTask IPC 1/1; full Rust — Core 654 unit + 28 integration,
  local storage 236, desktop IPC 36; Electron 499 passed / 2 source-update
  tests штатно skipped. `check:protocol`, typecheck, production build, bundle
  security check, `cargo fmt --check`, strict Clippy и `git diff --check`
  прошли. Evidence не содержит raw transcript, credentials, PII или absolute
  runtime paths.
| Memory Governance v1 (plan 50, implemented 2026-09-01) | durable storage schema v51; Core `MemoryWriteGate`; typed authority/durability/confidence; independent evidence guard; existing authenticated memory IPC and metadata-only OperationsPanel | additive migration with legacy safe defaults; invalid/secret/unverified records fail before SQL; pending/model/ambient records stay non-retrievable until validation and confirmation | disable extraction or withhold memory capability; preserve redacted metadata/tombstones; no blind replay of external effects | Core governance 3/3, local-storage memory 18/18, full Core 657 + integration tests, local-storage 236, desktop-ipc 36; Electron 499 passed and 2 skipped; protocol/typecheck and diff-check passed; evidence contains no credentials, raw body or PII |
| Causal Collaboration Bus v1 (plan 51, implemented 2026-09-01) | Core typed envelope, TeamSession-pinned routing, retained-child sequence substrate, SQLite schema v52, authenticated IPC 199–200/event 50, metadata-only Electron panel | additive migration; 32 KiB payload / 64 KiB envelope / 128-message inbox bounds; duplicate and compare-and-set delivery; subscription state is ephemeral; unknown dispatch is not retried | disable additive commands/events; preserve v51 backup on migration failure; reconcile unknown explicitly; no provider/tool/artifact side effects | Core 660/660 unit + focused module tests, local-storage 237/237, desktop-ipc doc tests; protocol generated; Electron typecheck/test and strict lint gates recorded for release commit; evidence contains no payload bodies, credentials, prompts, transcripts, absolute paths or PII |
| Conversation Workbench v1 (plan 52, implemented 2026-09-01) | Read-only Core composer over conversation event log; schema v52; authenticated IPC command 201/event 51; six capability-aware tabs; shell ChatStore presentation state | scope/cursor/limit bounds; metadata-only redaction; Tasks/Usage available; Files/Diff/Terminal/Browser typed unavailable; no effects | disable 201/51; clear projection on conversation switch; fresh snapshot after restart/stale cursor; no Core migration/store or blind retry | Core Workbench 2/2 focused tests; Rust check/fmt; Electron protocol/typecheck; Electron suite 500+ passed with source-update tests skipped; ChatStore per-conversation bounds; no raw content, credentials, prompts, paths or PII |

## Plan 53 — Diagnostics & Support Bundle v2 (2026-09-01)

- Contract/runtime: authenticated command 202 returns ephemeral schema-v2 JSON
  with `PASS/WARN/FAIL/SKIPPED` health results, measured duration, bounds,
  redaction omissions, metadata-only selected run references and SHA-256 hash.
  No SQLite migration/store or external effect is introduced.
- Export/UI: Electron main creates a local store-only ZIP with manifest,
  health/runtime/errors/events/logs, issue draft and redaction report; the
  final archive scan rejects credential/path fixtures. Settings preview shows
  Core state and redaction summary before save, and draft copying remains
  local. No upload/publication exists.
- Evidence: Core check, Electron support-bundle 2/2 and shell bridge 56/56,
  `npm run check:protocol`, `npm run typecheck`, `cargo fmt --all -- --check`
  and `git diff --check` passed; full Rust/Electron regression gates are run
  at release time. Evidence contains no credentials, raw payloads or PII.

## Plan 54 — Human Work Items v1 (2026-09-01)

- Contract/storage: Core-owned bounded `HumanWorkItem`, typed `Text`/`Choice`
  response schema, optimistic transitions and schema v53 JSON/transition store.
  The common migration ladder retains backup-before-migrate behaviour.
- Security/recovery: a shell submission is data rather than approval or grant;
  expiry is terminal, restart replays durable state only, and Team SOP bindings
  require a pinned human execution-mode slot. There is no v1 external dispatch.
- IPC/UI: authenticated command 203/event 52 and Electron Inbox projection;
  raw model prompts, credentials, hidden reasoning and approval payloads are
  excluded. Focused Core contract/recovery and Electron privacy tests accompany
  the full Rust/Electron protocol, typecheck and diff-check gates.

## Plan 55 — Agentic Browser Session v1 (2026-09-01)

- Contract/storage: Core-owned lifecycle, page revision/fingerprint refs,
  Agent/Human control generation, bounded snapshots and SQLite schema v54
  metadata store. The store contains no DOM, cookies, credentials, CDP URL or
  host path.
- Security/recovery: stale refs and takeover races fail closed; raw selector/
  env-CDP legacy route is disabled; missing packaged backend is typed unavailable
  and no browser effect is claimed. The packaged backend is `EvoHime.exe`,
  launched by Core under supervisor Job Object; network policy runs at the
  request boundary and rejects private resolved IPs and blocked schemes.
- IPC/UI: authenticated additive command 204/event 53 and metadata-only
  `AgenticBrowserSessionPanel`; protocol/typecheck tests prove no direct CDP or
  sensitive payload projection. Focused Core/storage tests cover lifecycle,
  stale/takeover, binary ArtifactStore objects and metadata round-trip.

## Plan 57 — Plan Artifact v1 (2026-09-01)

- Contract: `cargo test -p evohime-local-storage plan_artifact` — PASS;
  `cargo test -p evohime-core plan_artifact` — PASS.
- Integration: `cargo check -p evohime-core -p evohime-desktop-ipc` — PASS;
  `npm run check:protocol` — PASS; `npm run typecheck` — PASS.
- Redaction/authority: only bounded metadata events are emitted; renderer sends
  explicit Core actions and no direct database, filesystem, credential or
  effect authority. Fault/recovery scenarios remain represented by typed
  `stale`, `invalid_transition` and `unknown_outcome` states.
- Evidence is tied to the task commit; no credentials, raw prompt/output,
  absolute paths or personal data are included.

## Plan 56 — Artifact Handoff Registry v1 (2026-09-01)

- Core/storage: `artifact-handoff/v1`, bounded semantic revisions, lifecycle,
  metadata-only lineage/handoffs/acceptance and schema v55 additive migration;
  existing ArtifactStore remains the only byte owner.
- IPC/UI: authenticated additive command 205/event 54 and Electron
  `ArtifactHandoffRegistryPanel`; refs/projection only, with secret/raw
  prompt/output rejection and no capability expansion.
- Evidence: focused Core/storage tests, `cargo check`, generated protocol
  check, Electron typecheck and `git diff --check`; no credentials, raw
  payloads, transcripts, absolute paths or PII are included.

## Plan 58 — Workspace State Checkpoints v1 (2026-09-01)

- Contract: `evohime_core::workspace_state_checkpoints` is independent from
  immutable `TaskCheckpointV1`; it captures bounded regular files, computes
  deterministic SHA-256 hashes, excludes VCS/build/dependency directories and
  rejects symlink/reparse entries.
- Storage: additive schema v57 creates metadata and restore-journal tables;
  snapshot metadata is bounded and snapshot bytes remain outside the renderer
  and user Git history. Existing build snapshot IPC is the compatibility
  surface and now calls the conflict-safe plan-58 restore adapter.
- Recovery/security: preflight rechecks every captured file immediately before
  the first write; dirty user/external files produce typed conflict and no
  overwrite. The adapter does not mutate task history, SQLite task state,
  credentials or external effects.
- Checks: `cargo fmt --all`, focused Core contract tests (3 PASS), local-storage
  storage test (1 PASS), recovery integration test (1 PASS), plus the final
  workspace/package/protocol checks recorded with the task commit. Additive
  desktop IPC tag 209, generated bindings, Core dispatch and the Electron
  developer panel cover all five explicit operations; restore conflicts are
  journaled and task projection restore remains independent from file restore.
## Plan 64 — Workspace Bootstrap Manifest v1 (2026-09-02)

- Contract/security: bounded schema and hash validation, allowlisted local
  discovery, exact trust approval, relative-path checks, deny-by-default
  network handling, and unsupported-effect fail-closed behavior.
- Durability/recovery: additive SQLite schema v62, exact hash/fingerprint cache,
  single-flight leases, stale lease fencing to `unknown_outcome`, and prepared
  result deduplication.
- IPC/UI: authenticated additive command 215/event 61, generated bindings, and
  Electron developer panel with metadata-only event projection.
- Checks: focused Core/storage tests, schema migration test, cargo check,
  protocol generation/check, Electron typecheck, cargo format and diff-check.
  Evidence contains no credentials, raw process output, environment or PII.
## Plan 65 — Team Coordination Policies v1 (2026-09-02)

- Contract: versioned bounded TeamSpec with four policies; Core validates
  selector targets, directed handoff owner, event refs and loop limits.
- Durability: additive SQLite schema v63 stores policy revisions and fenced
  idempotent coordination state; restart uses persisted state, no blind effect
  retry is introduced.
- IPC/UI: authenticated additive command 216/event 60, generated bindings and
  Electron metadata-only developer panel; Core remains the only authority.
- Checks: focused contract/recovery/storage tests, cargo check, protocol check,
  Electron typecheck and full Electron regression. Evidence excludes secrets,
  raw prompts/outputs, absolute paths and PII.
## Plan 95 — Team Coordination Strategies v1 (2026-09-03)

- Contract: versioned Core-owned `TeamCoordinationStrategy` with
  RoundRobin/RuleSelector/ModelSelector/HandoffSwarm/GraphDirected; eligible
  roles are derived from the TeamSpec roster before selection, and model input
  is restricted to typed `ParticipantIdentity`.
- Runtime/security: explicit deterministic fallback only, bounded team and
  repeated-selection limits, immutable `session_id`/strategy revision/
  protocol hash fencing, and protocol-snapshot producer/consumer validation
  before handoff/graph dispatch. No capability grant or effect is performed by
  selection; route denial is fail-closed.
- Durability/client: additive idempotent
  `team_coordination_strategy_snapshots` storage with optimistic versioning;
  authenticated existing command 216/event 60 operation `select_strategy` and
  bounded Electron projection. No raw prompts, rationale, credentials,
  transcripts or PII are exposed.
- Checks: Core contract 5/5, local-storage strategy snapshot 1/1, prior
  policy regression 3/3, cargo fmt, protocol/typecheck and diff-check; focused
  evidence is reproducible and redacted. Unknown/stale/denied outcomes do not
  retry an external effect.
## Plan 66 — Typed Agent Handoff Contract v1 (2026-09-02)

- Contract: versioned bounded HandoffPacket, structured context budget,
  provenance and explicit Proposed/Accepted/Active/Completed plus rejection,
  expiry, failure and return outcomes.
- Durability/recovery: additive SQLite schema v64, duplicate/stale fencing,
  pending state persistence and expiry-safe transitions; no implicit grant or
  credential inheritance.
- IPC/UI: authenticated additive command 217/event 62, generated bindings and
  Electron metadata-only lifecycle panel.
- Checks: focused contract/recovery/storage tests, schema migration, cargo
  check/clippy, protocol check, Electron typecheck and full Electron regression.
  Evidence excludes secrets, raw prompts/outputs, absolute paths and PII.

## Plan 96 — Memory Views & Adaptive Recall v1 (2026-09-03)

- Contract: Core-owned hierarchical logical scopes and versioned MemoryView
  with bounded roots/depth, separate read/write rights and explicit shared
  read-only support; invalid hierarchy, control input and write-without-read
  fail closed.
- Runtime: Shallow/Deep/Auto modes are typed and bounded by the view; Core
  applies deterministic explainable composite lexical/freshness/provenance
  scoring only after scope authorization. Each decision records a read-barrier
  generation for explicit background-ingestion consistency semantics.
- Durability/client: additive idempotency/version-fenced view and barrier
  tables; authenticated command 244/event 89; Electron metadata-only panel
  for save/inspect/recall. Memory bodies, secrets, prompts and hidden
  reasoning remain outside the projection.
- Checks: Core contract 4/4 plus external contract 2/2 and recovery 1/1,
  local-storage 1/1, strict clippy, cargo check/fmt, protocol/typecheck,
  focused UI 1/1 and full Electron regression 107 passed files / 531 passed
  tests / 2 source-update tests skipped. Evidence is redacted.

## Plan 101 — Knowledge Source Registry v1 (2026-09-02)

- Contract: separate versioned KnowledgeSource/Binding/View/Chunk/Hit types,
  source lifecycle, sensitivity/trust, role/project/workflow targets and
  source revision/locator provenance; Knowledge remains separate from Memory.
- Runtime/durability: additive SQLite schema v78 for sources, bindings,
  manifests and chunks; monotonic source revisions, Ready-only authorized
  KnowledgeView, bounded keyword index/retrieval and stale/secret fail-closed
  behavior; scripts/macros and embedded fetch are not executed.
- IPC/UI: authenticated additive command 232/event 77, generated bindings and
  Electron metadata-only Knowledge Sources panel; raw chunks, credentials,
  prompts, outputs and absolute paths stay out of the renderer projection.
- Checks: focused Core contract/retrieval and storage revision tests, cargo
  fmt/check/clippy, migration check, protocol check, Electron typecheck,
  focused UI test, full Electron regression and native package smoke PASS.
  Evidence excludes secrets, raw prompts/outputs, absolute paths and PII.

## Plan 100 — Workspace Sets v1 (2026-09-02)

- Contract: versioned bounded WorkspaceSet with up to 8 roots, unique aliases,
  per-root grants/kind/VCS/revision identity, canonical root-qualified refs and
  typed validation errors.
- Runtime/recovery: durable schema v77 definitions, idempotency,
  version-fenced update, bind snapshots with exact set/root hash and restart
  recovery; cross-root search enforces per-root grant and traversal bounds.
- IPC/UI: authenticated additive command 231/event 76, generated bindings and
  Electron metadata-only Workspace Sets panel; canonical paths and sensitive
  payloads remain Core-only.
- Checks: focused Core contract/search tests, storage/version/migration tests,
  cargo fmt/check/clippy, protocol check, Electron typecheck, full Electron
  regression (103 files passed, 527 tests passed, 2 skipped) and native package
  smoke PASS. Evidence excludes secrets, raw prompts/outputs, absolute paths
  and PII.

## Plan 80 — Project Instruction Stack v1 (2026-09-02)

- Contract: Core-owned allowlisted discovery, frontmatter normalization,
  canonical paths, deterministic activation/precedence, source hashes and
  revisions for global/workspace/nested/AGENTS rules.
- Safety/bounds: 64 files, 64 KiB per rule, 256 KiB snapshot and 16 384 token
  budget; symlink/path escape, authority metadata, malformed input and
  overflow fail closed without silent truncation; markdown is never executed.
- Durability/runtime: additive SQLite schema v76, immutable snapshots,
  idempotency and restart-safe enabled/revision state; model context pins the
  snapshot hash and provenance includes rule hash/revision metadata.
- IPC/UI: authenticated additive command 230/event 75, generated bindings and
  Electron metadata-only Project Instruction Stack panel.
- Checks: cargo fmt/check/clippy, focused Core discovery/contract and storage
  tests, protocol check, Electron typecheck, full Electron regression (102
  files passed, 526 tests passed, 2 skipped) and native package smoke PASS.
  Evidence excludes secrets, raw prompts/outputs, absolute paths and PII.

## Plan 79 — Team Coordinator v1 (2026-09-02)

- Contract/runtime: bounded versioned `TeamWorkItem`, deterministic
  capability/output matching, consultations, decomposition/reassignment and
  managerial review with independent security/acceptance gates.
- Durability: additive SQLite schema v75 with revision-fenced work items and
  assignment/consultation/decision records; stale and unavailable outcomes
  remain typed.
- IPC/UI: authenticated additive command 229/event 74, generated bindings and
  projection-only Electron panel for queue, roster/load, assignments and
  escalation.
- Redaction: no secrets, raw prompts/outputs, grants, executable identities,
  absolute paths or PII in the coordinator contract or release evidence.

## Plan 77 — Headless Core CLI v1 (2026-09-02)

- Contract: Core-owned bounded RunRequest, `evohime.cli.event/v1`, explicit
  human/one-shot/NDJSON modes, stable exit-code mapping and redacted payloads.
- Runtime/client: authenticated named-pipe role `cli`, existing Core task and
  workflow commands, detached acceptance, status/watch/cancel and reconnect
  from event cursor; `resume` is a safe follow alias over that cursor, with no
  direct DB or second agent runtime.
- Packaging: official Windows `eva.exe` companion is included in the native
  package; Node/Python are not required by the CLI.
- Checks: CLI unit tests (3), Core contract tests (2), cargo check/fmt/clippy,
  package smoke, protocol check, Electron typecheck and full Electron
  regression: 99 files passed, 523 tests passed, 2 skipped. Evidence excludes
  secrets, raw prompts/outputs, absolute paths and PII.

## Plan 76 — Safe UI Extension Framework v1 (2026-09-02)

- Contract: versioned bounded `UiExtensionManifest`, declarative contribution
  kinds, Core-owned trust/compatibility metadata, install-disabled lifecycle,
  optimistic revision fence and fail-closed unknown bindings.
- Durability/safety: additive SQLite schema v73 stores scoped installed state;
  restart does not auto-enable extensions; arbitrary renderer code,
  shell/filesystem/network bindings and path traversal are rejected.
- IPC/UI: authenticated additive command 227/event 72, generated bindings and
  Electron metadata-only UI Extensions panel.
- Checks: 2 focused Core unit tests, schema 73 migration test, strict clippy,
  protocol generation/check, typecheck, focused UI test and full Electron
  regression: 99 files passed, 523 tests passed, 2 skipped. Evidence excludes
  secrets, raw prompts/outputs, absolute paths and PII.

## Plan 75 — Typed Context References v1 (2026-09-02)

- Contract: versioned ContextRef/ResolvedContextRef, closed built-in kinds,
  availability/projection enums, exact revision/hash binding and bounded
  resolver metadata.
- Safety/runtime: lazy deterministic context budget, path traversal/SSRF
  rejection, untrusted referenced content and no capability widening; mutable
  aliases resolve to concrete evidence before model use.
- Durability/IPC/UI: additive SQLite schema v72, authenticated command 226 /
  event 71 and Electron metadata-only resolver/budget panel.
- Checks: focused Rust resolver tests, schema migration test, cargo
  check/clippy, protocol check, Electron typecheck, focused UI test and full
  Electron regression. Evidence excludes secrets, raw prompts/outputs,
  absolute paths and PII.

## Plan 74 — Declarative Agent Component Registry v1 (2026-09-02)

- Contract: stable public provider IDs, typed descriptors, separate
  spec/component versions, built-in trust allowlist, bounded config/refs and
  canonical serialization.
- Safety/loading: schema/type/version/dependency validation, explicit migration,
  fail-closed unknown/untrusted providers, cycle detection and raw-secret
  rejection; no arbitrary dynamic code loading.
- Durability/IPC/UI: additive SQLite schema v71, authenticated command 225 /
  event 70, bounded metadata-only Electron inspect/diff/migration surface.
- Checks: focused Rust contract tests, schema migration test, cargo
  check/clippy, protocol check, Electron typecheck, focused registry UI test
  and full Electron regression. Evidence excludes secrets, raw prompts/outputs,
  absolute paths and PII.
## Plan 73 — Dependency-Aware Task Graph v1 (2026-09-02)

- Contract: bounded typed execution tasks/dependencies, DAG validation,
  deterministic ready-set, typed statuses, grants ceiling and immutable
  completed revisions.
- Replanning: atomic revision-fenced semantic patch; unknown references and
  cycles reject the whole patch; only downstream non-completed tasks become
  invalidated.
- Durability/IPC/UI: SQLite schema v70, authenticated additive command 224 /
  event 69 and Electron metadata-only graph projection; renderer has no graph
  authority or effect execution.
- Checks: focused Rust graph tests, schema migration test, cargo check/clippy,
  protocol check, Electron typecheck and full Electron regression. Evidence
  excludes secrets, raw prompts/outputs, absolute paths and PII.
## Plan 71 — Workflow Optimization Lab v1 (2026-09-02)

- Contract: bounded versioned OptimizationRun/Candidate, declarative mutations,
  multi-metric objective, constraints and train/validation/holdout semantics.
- Runtime: candidate evaluation uses the Core Agent Benchmark Matrix with
  frozen suite/policy; limits and security regressions fail closed; durable
  SQLite schema v68 preserves run/candidate metadata and explicit promotion.
- IPC/UI: authenticated additive command 222/event 67, generated bindings and
  Electron metadata-only offline lab projection.
- Checks: focused Rust contract, cargo check/clippy, protocol check, Electron
  typecheck and full Electron regression; evidence excludes credentials, raw
  benchmark output, prompts, absolute paths and PII.
## Plan 72 — Core Topic/Subscription Event Bus v1 (2026-09-02)

- Contract: versioned typed Topic/Event/Subscription, correlation/causation,
  exact/prefix/type selectors, bounded payloads and capability requirements.
- Delivery: ephemeral and durable modes, SQLite schema v69, ACK/NACK with
  bounded retries, dead-letter records, idempotent event identity and crash
  reconciliation to unknown without blind retry.
- IPC/UI: authenticated additive command 223/event 68 and Electron
  metadata-only projection; no external broker or renderer authority.
- Checks: focused Rust contract/recovery/schema tests, cargo check/clippy,
  protocol check, Electron typecheck and full regression (96 files passed,
  520 tests passed, 2 skipped). Evidence excludes secrets, raw payloads,
  credentials, absolute paths and PII.
## Plan 70 — Code Diagnostics Feedback Loop v1 (2026-09-02)

- Contract: Core-owned registered providers, bounded versioned diagnostics,
  canonical workspace/file binding and SHA-256 snapshot hashes.
- Delta/gate: durable snapshots and deltas classify introduced, resolved and
  persisting diagnostics; stale bindings are rejected and quality gate returns
  typed blocked/passed outcomes.
- IPC/UI: authenticated additive command 221/event 66, generated bindings and
  Electron metadata-only Problems projection.
- Checks: focused Rust contract/storage tests, cargo fmt/check, protocol check,
  Electron typecheck and full Electron regression (94 files passed, 518 tests
  passed, 2 skipped). Evidence excludes credentials, raw diagnostics, prompts,
  absolute paths and PII.
## Plan 69 — Runtime Intervention Pipeline v1 (2026-09-02)

- Contract: typed hook phases, deterministic ordering, explicit decisions,
  handler modes/failure policy, mutation audit hashes and reentrancy limits.
- Runtime safety: fail-closed security handlers, approval separation and
  bounded Core-only evaluation; renderer/plugins cannot gain authority.
- IPC/UI: authenticated additive command 220/event 65, generated bindings and
  Electron metadata-only diagnostics panel.
- Checks: focused contract/recovery tests, cargo fmt/check/clippy, protocol
  check, Electron typecheck and full Electron regression. Evidence excludes
  secrets, raw prompts/outputs, absolute paths and PII.
## Plan 68 — Experience Replay Library v1 (2026-09-02)

- Contract: bounded episodic records/steps, typed outcomes, evidence-backed
  score, scope/sensitivity/retention metadata and deterministic content hash.
- Write/recovery: Write Gate rejects missing evidence, unsafe refs and unknown
  outcome; duplicate-safe additive SQLite schema v66 and bounded context
  projection preserve advisory/non-authoritative semantics.
- IPC/UI: authenticated additive command 219/event 64, generated bindings and
  Electron metadata-only experience panel.
- Checks: focused contract/recovery/storage/migration tests, cargo fmt/check/
  clippy, protocol check, Electron typecheck and full Electron regression.
  Evidence excludes secrets, raw prompts/outputs, absolute paths and PII.
## Plan 67 — Schema-Driven Agent Configuration v1 (2026-09-02)

- Contract: versioned five-layer schema, typed field/registry/apply enums,
  semantic patches, diagnostics, restart semantics and deterministic effective
  snapshot hash.
- Durability/safety: additive SQLite schema v65, optimistic revision fencing,
  redacted credential state, unknown/executable-like fields rejected, active
  snapshots immutable.
- IPC/UI: authenticated additive command 218/event 63, generated bindings and
  Electron metadata-only schema/snapshot/action panel.
- Checks: focused contract/recovery/storage/migration tests, cargo fmt/check/
  clippy, protocol check, Electron typecheck and full Electron regression.
  Evidence excludes secrets, raw prompts/outputs, absolute paths and PII.
## Plan 102 — Agent Git Change Sets v1 (2026-09-02)

- Rust contract/storage: baseline, attribution, candidate hash, path traversal,
  ambiguity and bounded payload tests.
- Schema 79 and additive authenticated IPC command 233/event 78.
- Electron bridge/panel exposes only redacted included/excluded path metadata.
- Commit/undo/keep effects refuse without explicit Git preflight; no shared
  index, force operation, automatic push or secret payload.

## Plan 90 — Runtime Stall Guard (2026-09-02)

- Static detector scans known sync filesystem/process/sleep/network/database
  APIs and emits a bounded machine-readable JSON report.
- Findings carry stable fingerprints and explicit suppression reason; report
  excludes absolute paths and sensitive arguments. Detector never executes code.
- Windows CI runs the smoke gate; local `runtime-stall-guard.tests.ps1` passed.

## Plan 103 — Stateful Tool Workbench Sessions (2026-09-02)

- Existing Capability Workbench v1 is the authoritative implementation:
  lifecycle/revision, bounded scope/concurrency, leases, capability-filtered
  tools, reset/degraded recovery and metadata-only snapshots.
- Reuse evidence: focused Core/storage/IPC/UI tests and full Electron regression;
  no second authority or duplicate transport was introduced.
- Snapshot validation rejects secrets, process handles and forbidden private
  state; unknown/unavailable backend outcomes remain typed and fail closed.

## Plan 91 — Architect-Editor Model Pipeline (2026-09-02)

- Schema 80 stores the versioned Core-owned pipeline and typed EditIntent with
  content hash and exact workspace revision fence.
- Additive IPC command/event tags 234/79 expose only bounded redacted metadata;
  Electron UI is projection/action-only and does not receive hidden reasoning or
  capability grants.
- Evidence: focused Rust tests, clippy with `-D warnings`, generated protocol
  check, Electron typecheck/tests and `git diff --check`.

## Plan 81 — Event Visualizer Registry (2026-09-02)

- Schema 81 and IPC tags 235/80 provide versioned descriptors, deterministic
  matchers, built-in visualizers and a safe generic fallback.
- Extension descriptors are bounded/redacted and cannot claim host-controlled
  security rendering; renderer receives projection/action metadata only.
- Evidence: focused Rust tests, storage round-trip, clippy `-D warnings`,
  protocol check, typecheck, full Electron regression (528 passed, 2 skipped),
native package smoke and `git diff --check`.

## Plan 82 — Context Mentions (2026-09-02)

- Explicit user-only lexer covers file/folder/git/diagnostics/terminal/artifact
  syntax and rejects traversal/control/oversized locators.
- Core reuses Typed Context References v1 for revision/hash, sensitivity and
  bounded projections; untrusted imported text cannot trigger expansion.
- Evidence: three focused Core tests, clippy `-D warnings`, full Electron
  regression (528 passed, 2 skipped), native smoke and `git diff --check`.

## Plan 83 — Reasoning Operator Library (2026-09-02)

- Schema 82 and IPC tags 236/81 provide typed Generate/Review/Revise/Ensemble
  definitions and bounded proposal requests.
- Operators remain Core-owned computation only; no tool/capability/mutation
  authority is inferred from model output.

## Plan 84 — Output Guardrail Pipeline (2026-09-02)

- Schema 83 and IPC tags 237/82 provide typed Validate/Transform/Redact stages
  and bounded evaluation with three retries.
- Evidence: focused guardrail tests, clippy, protocol/typecheck and
  `git diff --check`; existing Sensitive Data Guardrails remain authoritative.
- Evidence: focused Core tests, storage migration, clippy `-D warnings`,
  protocol/typecheck, regression and `git diff --check`.

## Plan 92 — Privacy & Telemetry Governance (2026-09-03)

- Core-owned per-category consent and typed `TelemetryEventV1` use an
  allowlisted metadata dictionary; unknown properties and sensitive markers
  fail closed before persistence or any possible egress.
- Schema 88 and authenticated IPC command/event 242/87 provide a bounded
  offline queue, deterministic event-id deduplication, idempotency claims and
  revoke/clear lifecycle. No provider SDK or external network sink is enabled;
  diagnostics remain a separate category.
- Evidence: focused Core 3/3, local-storage 3/3, clippy `-D warnings`,
  generated protocol check, TypeScript typecheck, Electron regression 528
  passed/2 skipped, UI smoke test and `git diff --check`; evidence is
metadata-only and contains no credentials, prompts or raw output.

## Plan 94 — Conversation Bridge Adapters (2026-09-03)

- Core contract: provider allowlist, durable `ConversationBridge`/
  `ThreadBinding`, explicit principal pairing, revision-fenced revoke and
  typed attention/approval/Human Work Item command kinds.
- Runtime/storage: schema 89, bounded inbound queue with message-id
  deduplication, stored principal checks before bind/inbound/remote command,
  redacted outbound metadata projection; no network ingress or credentials.
- IPC/UI: authenticated additive command/event 243/88, generated bindings,
  metadata-only panel and typed bridge API. Evidence: Core 3/3, storage 1/1,
  clippy `-D warnings`, protocol/typecheck, focused UI 1/1 and full Electron
  regression 530 passed/2 skipped; `git diff --check` passed.
