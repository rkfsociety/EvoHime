# EvoHime — доменный контракт: качество, ревью, телеметрия и верификация

Канонический контракт домена верификации, quality contract, review lanes, телеметрии и оценки. Обновлено: 2026-11-18.
Остов архитектуры: [../architecture.md](../architecture.md).
См. также: [../features/reflection.md](../features/reflection.md).

## Local telemetry и deterministic evaluation

`telemetry/v1` — bounded derived projection над Core event journal, receipts и
model-request provenance; эти источники остаются source of truth. Projection
проверяет correlation/event IDs, attempt, bounded event count, redaction и
размер report. `judge_signal` хранится отдельно от deterministic gate verdict.
Offline evals используют literal fixtures, frozen inputs и не имеют доступа к
production filesystem, network, tools или SQLite; malformed traces получают
typed diagnostics, а неизвестный результат не считается pass.

## Privacy & Telemetry Governance v1 (план 92, реализован 2026-09-03)

Telemetry governance принадлежит Core и не требуется для работы приложения.
`TelemetryCategory::{Product,Operational,Diagnostics}` имеет отдельный
versioned consent (`unknown`, `denied`, `granted`); default state не разрешает
внешнюю отправку. Typed `TelemetryEventV1` принимает только allowlisted
metadata properties (`app_version`, `platform`, `action_class`, `outcome`,
`reason_code`, `duration_ms`, `schema_version`, `count`). Неизвестные поля,
превышение bounds и значения с секретными маркерами отвергаются до записи.

Разрешённые события попадают только в bounded offline SQLite queue (512
элементов/64 KiB event bound) с deterministic `event_id` deduplication. Core
handler предоставляет consent, enqueue, list и clear; clear удаляет queue и
consent, а UI предоставляет просмотр, явный revoke и очистку. Authenticated
IPC additive command 242/event 87, storage schema 88. Queue не является
provider SDK или network sink: diagnostics и потенциальная external telemetry
разделены, внешний egress остаётся default-closed и за пределами v1.

## Reliability, recovery и diagnostics

Electron main maintains only a bounded diagnostic projection. The additive shell command `shell.exportDiagnostics` opens a native save dialog and writes `evohime-support-bundle-v2` as a local ZIP with manifest, health, runtime, errors, bounded events/logs, issue draft and redaction report. It never reads workspace files, prompts, tool output or credential payloads; a final whole-archive scan fails closed before saving. ZIP entries use deflate when it reduces size and the destination is created with restrictive permissions. Log collection remains bounded to at most four files, 64 KiB from each file, 120 total lines and a 512 KiB v1-compatible shell projection.

The authenticated additive `CreateDiagnosticsSnapshot` command (tag 202) asks
Core for an ephemeral, bounded JSON snapshot. It maps existing Doctor checks to
`PASS/WARN/FAIL/SKIPPED`, includes measured collection duration, redaction
omissions, bounds and optional conversation/run metadata references, and
computes a SHA-256 fingerprint. It does not create a store or migration, does
not include raw prompts/files/tool payloads/credentials, and does not perform
repair or any external effect. A manually exported ZIP may include the result
after the user reviews the preview; an automatic failure report uses the same
bounded projection and does not wait for a preview action. The diagnostics
settings panel also offers an explicit, confirmed `shell.submitDiagnostics`
action. Main performs
the same final redaction scan, resolves the existing GitHub credential from the
environment/configuration or `gh auth token`, and creates an issue. Bundles that
fit the issue bound are included as base64; larger bundles are uploaded first to
a secret GitHub Gist through the same credential, and the issue contains its
link, archive size and SHA-256. A Gist is unlisted rather than private: anyone
with the issue link can read it. The same redacted report is started
automatically once for each live `task.failed` task ID; replayed failures during
startup/resync are excluded. Automatic publication is best-effort and does not
turn a GitHub/network failure into a second Core task failure.

Recovery UI consumes Core events as the source of truth and preserves typed `reason_code`, correlation, sequence and `UNKNOWN_OUTCOME`. Terminal task IDs are indexed once per projection, cancellation is offered only when Core explicitly marks `can_cancel`, and user-visible recovery details use a bounded allowlist of non-secret fields. Database operations use `core.cancelDatabaseOperation`, task operations use `core.stopTask`.

Repair and update statuses retain bounded stage evidence. Commit, push, CI refresh, update preparation and restart remain separate user actions; CI or an unknown outcome never triggers a blind effect.

The Security settings panel selects backup/restore paths in main and forwards only the selected path to Core. Core remains responsible for checksum, preview, approval, progress, cancellation, safety backup and rollback. DPAPI via Electron `safeStorage` remains the canonical provider credential contract. Provider persistence applies a defense-in-depth size/type check to stored ciphertext and writes through a mode-600 temporary file with flush, atomic rename and cleanup on failure.

### Agent Benchmark Matrix v1

Benchmark Matrix находится в `crates/evohime-core/src/agent_benchmark_matrix.rs`
и является Core-owned metadata-only контуром для versioned synthetic
challenges, ModelProfile × AgentProfile matrix и повторных attempts. Встроенный
`BenchmarkExecutor` разделяет воспроизводимый deterministic executor и явно
включаемый provider/nightly режим; при отсутствии provider результат остаётся
typed `unavailable`. На каждую комбинацию считаются pass-rate, timeout/failure
classes и P50/P95/P99 latency/cost. Unknown, skipped и unavailable не считаются
успешными и не продвигают baseline.

`benchmark_store` добавляет schema v42 с immutable suite/run/attempt/baseline
metadata и idempotent keys. В report входят contract/suite/profile hashes,
bounded metrics, comparison verdict и `redacted` status; raw prompt/output,
credentials, transcripts, absolute paths и production data запрещены. Security
regression — hard failure независимо от среднего score; baseline меняется
только отдельным Core-approved optimistic-version action. Authenticated IPC
использует additive commands 181–182 и event 38. Electron имеет
metadata-only `AgentBenchmarkMatrixPanel`, а deterministic PR gate
`cargo eval --mode deterministic` остаётся отдельным от nightly/manual matrix.

### Core-Owned Execution Trace Evaluation v1 (plan 184)

`crates/evohime-core/src/execution_trace_evaluation.rs` defines the
metadata-only trace and deterministic evaluator contracts consumed by the
existing Agent Benchmark Matrix. Schema v1 and evaluation-contract v1 bind
run/attempt identity, terminal state, ordered event ids, tool ids, canonical
argument digests, approval/effect references, bounded evidence refs and typed
outcomes. Raw prompts, tool arguments, provider output, transcripts,
credentials and reasoning are excluded from the serialized trace and its hash.

The contract bounds traces to 4,096 events, policies to 128 assertions and
assertion lists to 256 items. Assertions cover exact/subsequence tool order,
forbidden tools, approval-before-effect, required evidence, retry equivalence
and event limits. Validation rejects malformed or duplicate event identities,
non-contiguous sequence, invalid digest shape, retries without a preceding
ToolCall, duplicate effect ids, inconsistent event outcomes and effects without
a preceding matching approval. Empty traces are incomplete, not oversized.
Canonical SHA-256 is prefixed with `sha256:`; the legacy digest adapter returns
the same bytes as 64 lowercase hex for the existing
`AttemptResult::tool_trace_digest` field. No second trace store is introduced.

The current boundary is still contract/evaluator only: runtime capture,
partition by run/attempt, frozen-report lifecycle and recovery are not wired.
The generic ToolAgent event stream is not a typed trace source, and receipt
ledger events currently lack the attempt identity required for safe partition.
Stage 184.2 must connect the existing Core tool/effect boundary to the existing
EventJournal/execution-ledger owner, with redaction before hashing; it must not
infer identifiers from untyped payloads or add another persistence owner.

## Code Diagnostics Feedback Loop v1 (план 70)

Core нормализует зарегистрированные provider diagnostics в versioned
revision-bound snapshots. Canonical `sha256:` hashes и workspace-relative file
refs дают deterministic introduced/resolved/persisting delta; stale bindings
отклоняются как evidence. Registry, snapshots и deltas сохраняются в SQLite
schema v67, а quality gate возвращает только typed passed/blocked outcome.
Только Core может зарегистрировать provider или сохранить snapshot; raw output,
commands, credentials и arbitrary code actions не получают capabilities.
Authenticated additive IPC использует command 221/event 66, Electron получает
bounded metadata projection и не вычисляет state machine.

## Guided Calibration Sessions v1 (план 107)

`CalibrationSession` — Core-owned supervised loop с bounded iterations,
baseline/revised hashes, structured human feedback и provenance refs. Raw
prompts/output не сохраняются; dataset хранит только hashes и redacted notes.
Session guidance остаётся scoped к сессии. После повторного независимого
feedback `consolidate` создаёт durable `RefinementCandidateV1` через
существующий Continual Refinement pipeline со статусом
`proposed_for_refinement`; прямой activation отсутствует. Session lifecycle,
dataset hash, replay metadata и candidates защищены optimistic
revision/idempotency fence в schema 91. Additive authenticated IPC 249/event
94 и Electron panel показывают только bounded metadata.

## Extension Conformance Kit v1 (план 108)

Core-owned in-memory harness проверяет versioned descriptors и bounded probes
для Integration Provider, External Coding Agent Adapter, Workbench, UI
Extension и Declarative Component Provider. Specialized suite, security
assertions, instance isolation, disabled-path no-effect и deterministic fault
modes входят в machine-readable `ConformanceReport` с hash/version binding.
`RegistrationTransaction` использует staging и atomic commit; любой duplicate,
unsupported version или injected fault откатывает весь набор. Harness не
запускает extension code, production credentials или network и не выдаёт trust
или capabilities. Additive authenticated IPC 250/event 95 и Electron panel
показывают только bounded report metadata; состояние harness ephemeral.

## Verification Evidence Ledger v1 (план 122, реализован 2026-09-09)

Ledger фиксирует content-oriented `WorkspaceVerificationFingerprint` и typed
`VerificationEvidence` с before/after fingerprint, lane/verifier identity,
status и bounded artifact reference. Только `Passed` с совместимым fingerprint
может участвовать в readiness; failed, unavailable, cancelled, timeout,
protocol error и unknown остаются неуспешными. `evaluate_readiness` возвращает
Core-owned `Ready` либо `NeedsVerification` и не доверяет commit SHA вместо
содержимого.

SQLite schema v97 добавляет metadata-only ledger; raw output остаётся в
ArtifactStore. В текущем checkout verifier runner, selective filesystem
scanner и consumer integrations ещё не имеют versioned owner adapters, поэтому
их отсутствие отражается typed unavailable и не создаёт Passed/Ready. Renderer
получает только bounded projection после Core validation.

## Project Quality Contract v1 (план 124, реализован 2026-09-09)

Core-owned contract связывает required constraints с Verification Evidence
Ledger, хранит immutable revision metadata в schema 99 и выдаёт conservative
quality readiness. Без Passed evidence required constraint остаётся
NeedsVerification; model-authored metrics не принимаются.

## Design Intent Review Lane v1 (план 126, реализован 2026-09-09)

Design Intent — bounded Core-owned statement с exact scope/hash/revision;
review record хранит только verdict, reviewer и bounded evidence refs. Plan
Artifact и Architecture Snapshot сохраняют собственных владельцев. SQLite
schema v101 добавляет metadata-only review storage; отсутствие evidence даёт
Unknown/NeedsReview.

## Host Resource Telemetry & Pressure Guard v1 (план 134, реализован 2026-09-09)

`host_resource_telemetry` — Core-owned read-only contract для bounded CPU,
available-memory и storage-free samples. `MeasuredMetric` хранит value/unit,
collector source, timestamp, freshness и explicit `MetricStatus`; unsupported,
permission-denied, transient, stale и invalid sensors никогда не кодируются как
числовой ноль. `PressurePolicy` v1 применяет bounded freshness и thresholds,
возвращая `Unknown`, `Normal`, `Elevated`, `High` или `Critical`. `HostTelemetryService`
хранит не более 120 последних валидных snapshots в памяти и после restart не
выдаёт старое значение как current authority. `TaskCoordinator` предоставляет
единственную запись через `record_host_resource_snapshot`; renderer и
collectors не могут подделать pressure. Network probes, shell/PowerShell
polling, serials и process lists не входят в этот контракт.

## Code Review Lane v1 (план 135)

`evohime-local-storage::code_review_lane_store` — единственная durable
authority для bounded review revisions. Core target identity фиксирует kind,
base/head, diff hash, workspace fingerprint и changed paths; findings имеют
stable fingerprint и typed lifecycle, а coverage и verdict fail closed.
Migration v104 использует общий backup/transaction ladder, revision fence и
idempotency key. В storage не попадают patch bodies, prompts, credentials или
raw logs.

`CodeReviewLane` runtime принимает только `save`, `get`, `reconcile` и
`interrupt`, журналирует projection и после restart восстанавливает только
проверенный durable record. Partial/failed/unknown coverage либо interrupted
review дают `ReviewIncomplete`; stale re-review находится по fingerprint.
Authenticated additive IPC command 263 и generated Electron bindings
проецируют только bounded metadata. Renderer не вычисляет verdict и не
получает Core/storage authority.

## Evidence-Preserving Static Analysis Packs v1 (план 136)

`static_analysis_packs` хранит Core-owned bounded metadata для versioned
pack/rule registry, trust state, rollout mode, analyzer identity, coverage,
findings и immutable baseline/adoption/delta reports. Migration v105 сохраняет
только bounded JSON/revision rows с idempotency; untrusted pack не получает
execution authority и не может включить `Enforce`.

Analyzer execution остаётся registered adapter capability. Только `Complete`
является полным coverage state; partial/unsupported/failed/unknown дают
`incomplete`, а deterministic high-severity new findings дают `blocked`.
Commands `register`, `inspect` и `evaluate` проходят Core coordinator/journal,
authenticated IPC command 264/event 109 и metadata-only Electron projection.
Code Diagnostics, Verification Evidence Ledger и Code Review Lane сохраняют
свои authorities.

## Authorized Security Assessment Lane v1 (план 140)

`authorized_security_assessment.rs` — единственный Core-owned contract для
bounded scope, authorization, assessment status и evidence-backed findings.
`validate_authorization` fail-closed проверяет actor, policy hash, expiry,
scope/path limits и canonical hash; `start_allowed` повторно связывает
assessment с authorization перед любым переходом. Storage schema v109 хранит
только revisioned metadata и idempotency key. Команды IPC 268/event 113 дают
authenticated replay-safe metadata projection; renderer не получает secrets,
raw prompts, logs или authority. Это adapter/lane, а не новый scanner,
executor, scheduler или permission system: фактический эффект остаётся у
существующих Core owners.

## Multi-Reviewer Ensemble & Adjudication v1 (план 170)

Core владеет bounded orchestration metadata для reviewer ensemble: immutable
profile и input snapshot, reviewer slots с independence snapshot, run revision,
normalized finding candidates, conservative clusters и proof-gated completion.
Существующие Code Review Lane, model-purpose routing и ACP остаются владельцами
исполнения reviewer slots; ensemble не создаёт второй review lane, gateway или
majority authority. SQLite schema v170 и authenticated desktop IPC 278/123
хранят и проецируют только bounded refs, hashes, statuses, classifications и
evidence refs. Renderer не может подделать verdict, а cancellation и повторный
запуск защищены revision/hash boundaries.

## Deterministic Review Execution Plan v1 (план 148)

Core владеет bounded versioned review plan с canonical SHA-256, lifecycle,
actor/scope/evidence refs и безопасным verdict `unknown_without_evidence`.
SQLite schema v116 хранит revision-aware metadata и immutable run boundary;
IPC tags 275/120 добавлены аддитивно и проецируют только redacted metadata.
