# План разработки EvoHime Desktop

Статус: foundation, desktop shell, automation, self-repair/self-update и
технические release-gates реализованы. Пользовательский self-repair/self-update
включён как строго ручной production-контур; автоматический ремонт и
автоматический push не входят в продукт. Фактическое состояние checkout находится в
[`current-state.md`](current-state.md), архитектурные контракты — в
[`architecture.md`](architecture.md), а долгосрочные направления — в
[`roadmap.md`](roadmap.md).

## Цель текущего цикла

Сохранить стабильный локальный Windows AI-agent: пользователь запускает один
desktop-клиент, выбирает workspace, выполняет задачу и получает поток событий
через authenticated versioned named pipe. Core остаётся владельцем состояния,
прав доступа, эффектов и SQLite; Electron отображает только IPC-проекцию.

Пользовательские версионные релизы для текущего цикла не создаются. Постоянный
релиз `installer` определяется коммитом и веткой в `evohime.build.json`.

## Текущий порядок работ

Планы 22, 23, 24 и 25 реализованы и закрыты. План 58 реализован и закрыт:
workspace file checkpoints отделены от TaskCheckpoint; подробный контракт и
границы находятся в `docs/architecture.md`, evidence — в
`docs/release-evidence.md`. План 23 — TaskCheckpoint для compaction
и recovery — прошёл этапы 23.1 (contract/storage), 23.2 (runtime/recovery),
23.3 (IPC/UI) и 23.4 (acceptance/release evidence); комплект `23-0` … `23-4`
удалён после переноса контракта и состояния в актуальную документацию. После
плана 23 порядок продолжится согласно `docs/plans/README.md`. План 24 — Agent
Skills — закрыт после проверки registry, fail-closed validation, typed IPC/UI,
metadata-only trace и полного release-контура; его комплект также удалён.
План 25 — Persistent Goals — закрыт после проверки durable Core contract,
SQLite schema v33, revision/event history, recovery, typed IPC/UI и redacted
release evidence; его комплект также удалён.

План 26 — Continuation Policy — реализован: Core contract/decision table,
durable runtime storage и recovery, bounded continuation loop, typed IPC/UI,
redaction и реальные Core IPC проверки перенесены в каноническую документацию.
Workflow/evidence adapters с отсутствующим provider остаются fail-closed
`unavailable`; они не являются implicit success и не дают policy новых прав.

1. **Планы 26–115.** Выполнять численно по `(NN, M)`, не перескакивая через
   blocking dependency; закрытый комплект переносить в каноническую
   документацию и удалять из `docs/plans/`.
2. **Поддержка релиза.** Сохранять зелёными Rust/Electron/package gates и
   Windows compatibility/installer acceptance.
3. **Reliability и security.** Развивать credential, recovery, diagnostics и
   backup/restore UX только в существующих границах Core и main-процесса.
4. **Совместимость.** Поддерживать Windows 10/11 CI; informative ARM64/Insider
   runs остаются исследовательскими и не меняют базовый release scope.
5. **Продуктовая граница.** Не возвращать web runtime, public HTTP, внешний
   Node/Python runtime или автоматические repair/push/restart действия.

План 27 закрыт: retained child contexts, durable mailbox, restart reconciliation
и metadata-only IPC/UI перенесены в canonical architecture/current-state
и подтверждены свежим полным прогоном 29 августа 2026 года: Rust 563/208/35,
Electron 470 passed и 2 штатно skipped, clippy/typecheck/protocol check passed.

План 28 закрыт: Persistent Analysis Kernel перенесён в canonical
architecture/current-state/release-evidence. Подтверждены schema v38,
metadata-only object registry, Core-authoritative host bridge, allowlisted
supervisor worker, crash/recovery fencing, checkpoint/selected-child refs,
typed IPC/UI и redacted packaged fault evidence; stage-комплект плана удалён.
контракт; schema revision — v37.

Владельцы, критерии закрытия и влияние на выпуск находятся в
[`decision-register.md`](decision-register.md). Порядок работ не меняет
границы продукта: внешний cloud control plane, public HTTP, обязательный GPU,
внешний Node/Python runtime и unrestricted adapter fallback не добавляются.

## Критерии готовности

- Rust Core, storage, desktop IPC и supervisor проходят свои тесты и проверки
  формата;
- Electron `check:protocol`, `typecheck`, unit/contract tests и bundle checks
  проходят;
- automation boundary и release evidence gates проходят без credentials и
  необезличенных данных;
- Windows package, installer, upgrade, rollback и compatibility smoke проходят
  в CI;
- каждый закрытый open decision имеет код, focused test, redacted evidence и
  обновлённые `current-state.md`, `decision-register.md` и `release-evidence.md`;
- `git diff --check` проходит, а task-only изменения зафиксированы коммитом.

## Правило обновления документов

При расхождении сначала проверяются код и тесты, затем обновляется
[`current-state.md`](current-state.md). Архитектурные изменения фиксируются в
[`architecture.md`](architecture.md), решения — в
[`decision-register.md`](decision-register.md), а статус выпуска — в
[`release-evidence.md`](release-evidence.md). Исторические результаты не смешиваются
с текущей проверкой: для них указываются дата, команда и область проверки.

План 29 закрыт: Continual Refinement v1 перенесён в canonical
architecture/current-state/release-evidence. Подтверждены bounded evidence
admission, schema v39, metadata-only candidate store, optimistic lifecycle,
authenticated IPC 166–168 и Electron OperationsPanel. Skill/PromptRule без
Core-owned activation owner остаются typed `unavailable`; blind activation,
authority expansion и raw transcript projection запрещены.

План 30 закрыт: Workflow Package v1 перенесён в canonical
architecture/current-state. Подтверждены bounded JSON export/import,
metadata-only SQLite ownership, canonical SHA-256, fail-closed redaction,
credential-slot rebinding, atomic package-file write, explicit commit,
authenticated IPC 169–172 и Electron Workflow Package panel. Комплект плана
30 удалён после полного Rust/Electron regression-прогона и bundle gates.

План 31 закрыт: Visual Workflow Builder v1 перенесён в canonical
architecture/current-state. Core-owned authoring, registry validation,
immutable publish, recovery, read-only inspection, bounded IPC handoff и
redaction gates подтверждены focused и regression-тестами; комплект планов
удалён после проверки ссылок и evidence.

План 32 закрыт: Conversational Workflow Composer v1 перенесён в canonical
architecture/current-state. Подтверждены strict proposal envelope/parser,
Core-owned registry validation, bounded model gateway invocation, typed edits,
Builder handoff/save, redacted provenance persistence, additive IPC 174/event 34
и Electron metadata-only Composer surface. После финальной проверки комплект
32-0 … 32-4 удалён.

План 34 закрыт: Event Trigger Runtime v1 перенесён в canonical
architecture/current-state. Подтверждены bounded local/system ingress,
workflow pinning, mapping/dedup/rate limits, schema v41, authenticated IPC
177–178/event 36, Electron projection и typed unavailable для provider webhook.
Комплект планов 34-0 … 34-4 удалён после полного regression-прогона.

План 35 закрыт: Invocation Presets v1 перенесён в canonical
architecture/current-state. Реализованы Core-owned version-pinned contract,
deterministic redacted hash, completed-run sanitizer, immutable/idempotent
SQLite revisions, explicit migration preview/commit, temporary run overrides,
immutable preset revision/hash/workspace snapshot в automation scheduler,
обычный WorkflowRuntime path, authenticated IPC 179–180/event 37 и
metadata-only Electron WorkflowPanel. Drift, rebinding, stale revision,
duplicate slot и unknown outcome остаются fail-closed; комплект 35-0 … 35-4
удалён после полного regression-прогона.

План 36 закрыт: Agent Benchmark Matrix v1 перенесён в canonical
architecture/current-state/release-evidence. Подтверждены versioned synthetic
suite, ModelProfile/AgentProfile matrix, repeated attempts, bounded statistics,
baseline comparison, maintain/improve/explore sets, hard security failures,
redacted JSON report, schema v42, authenticated IPC 181–182/event 38 и
metadata-only Electron panel. Deterministic PR gate остаётся отдельным, а
  provider-unavailable не считается pass; комплект 36-0 … 36-4 удалён.

План 37 закрыт: Agent Middleware Pipeline v1 перенесён в
`architecture.md`, `current-state.md` и `release-evidence.md`. Подтверждены
versioned typed hooks для agent/model/tool phases, deterministic run snapshot,
immutable bounded overrides, Core-only capability authority, schema v43,
recovery/idempotency semantics, authenticated IPC 183–184/event 39 и
metadata-only Electron projection. Комплект `37-0` … `37-4` удалён после
свежих focused и regression gates.
План 38 закрыт: Adaptive Tool Catalog v1 реализует Core-owned bounded selection
между `ToolRegistry` и `ToolAgent`; критерии, cache invalidation, fallback,
semantic/model allowlist и metadata-only Electron projection подтверждены
focused Core/Electron tests и свежими protocol/typecheck gates.

План 39 закрыт: Structured Response Contract v1 перенесён в canonical
architecture/current-state/release-evidence. Подтверждены deterministic
contract hash и Core validation, capability-aware native/synthetic/auto
strategy, bounded repair, ephemeral restart semantics, authenticated IPC
185/40 и metadata-only Electron projection.

План 40 закрыт: Sensitive Data Guardrails v1 перенесён в canonical
`architecture.md`, `current-state.md` и `release-evidence.md`. Реализованы
versioned Core detector/redactor, deterministic policy snapshot, recursive JSON,
cross-chunk streaming, model/tool/stream/trace admission, fail-closed bounds,
ephemeral restart semantics, authenticated IPC 186/41 и metadata-only Electron
panel. Комплект `40-0` … `40-4` удалён после свежих focused и regression gates.

План 41 закрыт: Execution Policy Profiles v1 перенесён в canonical
`architecture.md`, `current-state.md` и `release-evidence.md`. Реализованы
общий ToolRegistry resolver для `shell.execute`/`process.run`, deny-by-default
environment, bounded process I/O/timeout, Windows Job Object tree cleanup,
storage schema v44, authenticated IPC 187/42 и metadata-only Electron panel.
Комплект `41-0` … `41-4` удалён после focused contract/storage/Core/Electron и
protocol/typecheck проверок.

План 42 закрыт: Model Resilience Policy v1 перенесён в canonical
`architecture.md`, `current-state.md` и `release-evidence.md`. Реализованы
Core-owned versioned policy/hash, normalized failure classes, bounded
retry/fallback budgets, allowlisted profile compatibility по capability,
privacy и residency, cancellation и provenance-safe recovery без blind retry.
Policy state ephemeral, schema остаётся v44; authenticated IPC использует
188/event 43, Electron показывает metadata-only panel. Комплект 42-0 … 42-4
удалён после свежих focused и regression gates.

План 43 закрыт: Execution Backend Registry v1 перенесён в canonical
`architecture.md`, `current-state.md` и `release-evidence.md`. Реализованы
Core-owned durable registry, schema v45, typed local/remote handshake и health
failures, Core-policy capability intersection, optimistic/idempotent mutation,
immutable run-affinity snapshot и fail-closed remote `transport_unavailable`.
Authenticated IPC использует additive 189/event 44, Electron — metadata-only
panel. Комплект 43-0 … 43-4 удалён после focused/full Rust, IPC, protocol,
typecheck, Electron и diff-check проверок.

План 44 закрыт: Tool Simulation Runtime v1 перенесён в canonical
architecture/current-state/release-evidence. Подтверждены explicit
Real/Fixture/Emulated/DryRun modes, deterministic schema-v1 fixture matching,
Structured Response validation, no-real-dispatch workflow interception,
fixture benchmark executor, ephemeral restart semantics и metadata-only
authenticated IPC 190/event 45. Комплект 44-0 … 44-4 удалён после focused и
regression-проверок.

План 45 закрыт: External Coding Agent Adapter v1 перенесён в canonical
`architecture.md`, `current-state.md` и `release-evidence.md`. Подтверждены
framed typed protocol `evohime.external-agent/v1`, Core-owned preset/conversation
snapshots, declared-slot credential boundary, schema v46, supervisor Job Object
lifecycle, capability handshake, IPC 191–192/event 46 и metadata-only UI с
`supervised_opaque`/`unavailable`. Комплект `45-0` … `45-4` удалён после
свежих focused и regression gates.

План 46 закрыт: Agent Role Profiles v1 перенесён в canonical
`architecture.md`, `current-state.md` и `release-evidence.md`. Подтверждены
versioned Core contract, typed input/output, human/AI mode, Core-side grants
intersection, pinned revision/hash, SQLite schema v47, recovery semantics,
authenticated IPC 193–194/event 47 и metadata-only Electron panel. Комплект
`46-0` … `46-4` удалён после focused и regression gates.

План 47 закрыт: Skill Trust Pipeline v1 перенесён в canonical
`architecture.md`, `current-state.md` и `release-evidence.md`. Подтверждены
offline deterministic scan, hash-bound trust gate, fail-closed contextual
review contract, metadata-only IPC/UI projection и schema 48. Комплект
`47-0` … `47-4` удалён после focused и regression gates.
# План 48 закрыт: Team SOP Protocols v1 перенесён в canonical
`architecture.md`, `current-state.md` и `release-evidence.md`; комплект
`48-0` … `48-4` удалён после свежих focused и regression gates.

# План 49 закрыт: Resumable Conversation Event Log v1 перенесён в canonical
`architecture.md`, `current-state.md` и `release-evidence.md`. Реализованы
Core-owned schema v50 и per-conversation cursor, transactional stable-message
dedup, redacted unified projections, before/after history, resume/live IPC
197–198/event 49 и Electron reconciliation с gap/conflict/retry states.
Комплект `49-0` … `49-4` удалён после свежих Rust/Electron regression,
protocol/typecheck, clippy, formatting и diff-check gates.
# План 50 закрыт: Memory Governance v1

Выполнен 1 сентября 2026 года. Typed governance metadata и
Core-owned `MemoryWriteGate` добавлены поверх существующих memory
domain/API/extraction/store без второго durable record/store; schema — v51.

План 51 реализован 1 сентября 2026 года: Causal Collaboration Bus v1
использует Core-owned typed envelope, TeamSession protocol hash и Core-derived
sender/routing поверх retained-child sequence substrate. Значимые сообщения
хранятся metadata-only в `collaboration_messages` (schema v52), общий inbox
ограничен 128 pending сообщений, payload — 32 KiB; subscriptions ephemeral.
Authenticated IPC additive: commands 199–200/event 50, Electron показывает
только bounded redacted projection. Duplicate, route denial, secret/oversize,
compare-and-set delivery и unknown recovery проверены focused tests; внешние
provider/tool/artifact effects bus не запускает.
Подтверждены storage/core focused tests, migration compatibility, independent
evidence guard, metadata-only projection и существующая authenticated IPC/UI
граница. Комплект `50-0` … `50-4` удалён после финального regression-прогона.

План 52 закрыт 1 сентября 2026 года: Conversation Workbench v1 перенесён в
canonical architecture/current-state/release-evidence. Подтверждены read-only
Core composer над event log, schema v52 без новой migration, authenticated IPC
201/event 51, шесть capability-aware tabs, bounded per-conversation ChatStore
presentation state, stale-safe switching, redacted projection и live refresh.
Комплект `52-0` … `52-4` удалён после свежих Rust/Electron regression gates.

План 53 закрыт 1 сентября 2026 года: Diagnostics & Support Bundle v2
перенесён в canonical architecture/current-state/release-evidence. Core
предоставляет ephemeral authenticated snapshot command 202 с typed health
outcomes, bounds, duration, redaction omissions и metadata-only failed-run
context. Electron main собирает локальный ZIP с manifest/preview/issue draft и
делает final whole-archive scan; Settings показывает preview и redaction
summary. Новая migration/store и network publication отсутствуют. Комплект
53-0 … 53-4 удалён после focused Core/Electron, protocol/typecheck, formatting
и diff-check gates.

План 54 закрыт 1 сентября 2026 года: Human Work Items v1 добавляет durable
Core-owned Inbox, schema v53, typed response schema и optimistic state machine.
Human response не является approval или capability grant; Team SOP binding
допускает только pinned `ExecutionMode::Human` role slot. Authenticated IPC
использует command 203/event 52, Electron показывает bounded Inbox без raw model
prompts, credentials или hidden reasoning. Комплект 54-0 … 54-4 удалён после
focused Rust/Electron, protocol/typecheck, format и diff-check gates.

План 57 закрыт 1 сентября 2026 года: Plan Artifact v1 добавляет durable
Core-owned versioned planning contract, immutable revisions, explicit Plan to
Execute transitions, canonical hash, policy snapshot и bounded Electron
projection. SQLite schema v56 и additive authenticated IPC tags 206–208/event 55
реализованы; неизвестный effect не повторяется вслепую.

План 56 закрыт 1 сентября 2026 года: Artifact Handoff Registry v1 добавляет
Core-owned semantic ProjectArtifact revisions поверх ArtifactStore, immutable
lineage/handoff/acceptance metadata, bounded freshness fingerprints и
idempotent lifecycle. SQLite schema v55 и authenticated IPC command 205/event
54 аддитивны; Electron показывает metadata-only projection без bytes, raw
prompts/outputs, credentials или capability authority. Комплект 56-0 … 56-4
удалён после focused contract/storage/Core, protocol/typecheck и diff-check
проверок.

План 59 закрыт 1 сентября 2026 года: Incremental Change Protocol v1 реализован
как bounded Core-owned pipeline от requirement delta/baseline refs до fenced
metadata transition. Реализованы schema 58, stale/idempotency/terminal-safe
storage, Core handler, authenticated IPC command 210/event 57 и Electron
projection. Комплект `59-0` … `59-4` удалён после свежих Rust/Electron
regression, protocol/typecheck, format и diff-check gates.

План 60 (issue #40, Revision-Safe Workspace Files) закрыт 1 сентября 2026
года. Реализованы typed namespaces, hash-based stale protection, immutable
uploads, run-scoped scratch, unified filesystem tool boundary и metadata-only
IPC/UI projection; комплект плана удалён после свежих проверок.

План 61 (issue #41, Task Worktree Isolation) закрыт 1 сентября 2026 года.
Подтверждены durable registry, task pinning к ready worktree root, approved
detached worktree tool, lifecycle version fencing, safe cleanup и additive
IPC/UI projection; комплект плана удалён после проверок.
План 62 (issue #42, Team Resource Budget) закрыт 1 сентября 2026 года.
Контракт, SQLite schema v60, Core preflight/recording, authenticated additive
IPC, Electron projection и focused/full verification перенесены в
docs/architecture.md и docs/current-state.md; комплект плана удалён.
План 63 (issue #43, Composable Termination Conditions) закрыт 1 сентября
2026 года. Реализованы тринадцать встроенных условий, `Any`/`All`, durable
replay-safe state, first-trigger evidence, hard-stop precedence, schema v61,
authenticated IPC command 214/event 60 и metadata-only Electron projection.
Контракт и подтверждённое состояние перенесены в
`docs/architecture.md` и `docs/current-state.md`; комплект плана удалён.
План 64 закрыт 2 сентября 2026 года. Следующий незавершённый этап выбирается
из `docs/plans` по следующему номеру.
План 65 закрыт 2 сентября 2026 года. Следующий незавершённый этап выбирается
из `docs/plans` по следующему номеру.
