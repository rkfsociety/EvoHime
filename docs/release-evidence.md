# EvoHime — release evidence и rollback matrix

Этот документ описывает evidence для поставки. Artifact bundle должен быть
redacted: допускаются commit, contract/schema versions, test IDs, hashes,
typed outcomes, bounded metrics и recovery state; credentials, raw provider
output, transcripts, absolute paths и PII запрещены.

## Текущий статус выпуска

Статус: `TECHNICAL_GATES_PASS / RELEASE_GREEN`.

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
