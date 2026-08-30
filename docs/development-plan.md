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

Планы 22, 23, 24 и 25 реализованы и закрыты. План 23 — TaskCheckpoint для compaction
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

План 35 пока не закрыт. Реализован и проверен первый безопасный срез
Invocation Presets: Core-owned versioned contract, deterministic redacted hash,
fail-closed completed-run sanitizer, immutable/idempotent SQLite revisions,
authenticated IPC 179–180/event 37 и metadata-only Electron WorkflowPanel.
Обязательные незавершённые части — explicit migration preview/commit и
immutable preset reference между `automation_store` scheduler и обычным
workflow runtime; комплекты `35-0` … `35-4` сохраняются до их реализации.
