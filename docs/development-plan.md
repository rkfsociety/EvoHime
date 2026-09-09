# План разработки EvoHime Desktop

Обновлено: 2026-09-09.

## Цель

Сохранять стабильный локальный Windows AI-agent: пользователь запускает один
desktop-клиент, выбирает workspace и модель, выполняет задачу и получает поток
событий через authenticated versioned named pipe. Core остаётся владельцем
состояния, прав, эффектов и SQLite; Electron отображает только IPC-проекцию.

Foundation, desktop shell, automation, self-repair/self-update и основные
технические release-gates уже реализованы. Пользовательский self-repair —
строго ручной: provider и model выбираются до запуска, diagnose/commit/push/
restart подтверждаются отдельно. Автоматический ремонт, автоматический push и
новые версионные релизы в текущий scope не входят.

## Источник текущего состояния

Факты о checkout находятся в [`current-state.md`](current-state.md).
Архитектурные контракты — в [`architecture.md`](architecture.md), security
границы — в [`../SECURITY.md`](../SECURITY.md), release evidence — в
[`release-evidence.md`](release-evidence.md). Каталог планов и точные зависимости
этапов — в [`plans/README.md`](plans/README.md).

## Исполняемая очередь

Незавершённые планы выполняются по графу зависимостей и по этапам `0 → 4`.
Текущий активный каталог — планы `133–143` и `145–167`; блокирующие и
опциональные зависимости указаны в overview каждого плана и сведены в
[`plans/README.md`](plans/README.md). Закрытые MVP-планы `127–130` не входят в
очередь повторно.

| Диапазон | Содержание | Статус |
| --- | --- | --- |
| 102 | Agent Git Change Sets v1: baseline, attribution, safe commit/undo | реализован 2026-09-09 |
| 118 | persistent agent organization registry | реализован 2026-09-04 |
| 120 | grounded research workspace | реализован 2026-09-09 |
| 121 | local model performance calibration | реализован 2026-09-09 |
| 122 | verification evidence ledger | реализован 2026-09-09 |
| 123 | content-aware context compression | реализован 2026-09-09 |
| 124 | project quality contract | реализован 2026-09-09 |
| 125 | free provider reliability routing | реализован 2026-09-09 |
| 126 | design intent review lane | реализован 2026-09-09 |
| 127–130 | remote control, local inference, model cascade, task leases | MVP-контуры реализованы 2026-09-09 |
| 131 | unified context namespace | реализован 2026-09-09 |
| 132 | durable background execution plane | реализован 2026-09-09 |
| 133–143 | utilities, resource guard, review, analysis, loadouts, skills, capability, security, service graph, optimizer, notebook | активная очередь по графу |
| 144 | модульный manifest и выборочное обновление компонентов | реализовано 2026-09-04 |
| 145–167 | publication, voice, offline consolidation, review, model compare, policy, suggestions, experiments, computer use, execution board, grounding, temporal memory, IDE, checkpoints, diagrams, compatibility, motion, recipes, voice output, command center | активная очередь по графу |

Планы 01–130 и 144 закрыты и удалены из временного каталога после переноса
контрактов и evidence в канонические документы. Нельзя считать план закрытым
по одному stage-файлу или по наличию кода: закрытие требует реализации,
recovery, IPC/UI при наличии, focused tests, release evidence и обновления
канонической документации.

## План 102: Agent Git Change Sets v1 (реализован)

План 102 закрыт после Core/storage/runtime vertical slice, additive
authenticated IPC 233/78, bounded Electron projection, Incremental Change и
Task Worktree references, staged-path isolation, durable commit reconciliation,
safe undo, durable idempotency claim, bounded Git timeout и redacted release evidence. Контракт перенесён в
`architecture.md`, подтверждённое состояние — в `current-state.md`, а
временный комплект этапов отсутствует.

## План 118: Persistent Agent Organization Registry

План 118 закрыт после итерационного ревью и реализации Core/storage/runtime,
authenticated IPC 259/104, Electron projection/UI, startup recovery,
focused/regression checks и переноса контракта в `architecture.md` и состояния
в `current-state.md`. Schema v92 также активирует пропущенную migration v91.

## План 119: Execution Environment Profiles (реализован)

План закрыт после итерационного ревью, Core/storage vertical slice, schema v93,
authenticated IPC 260/105, replay/resync, metadata-only Electron projection,
fail-closed owner resolution, focused/full Rust and Electron checks, production
bundle и native-package smoke. Контракт и ограничения перенесены в
`architecture.md` и `current-state.md`; evidence находится в
`release-evidence.md`. Следующие планы используют этот canonical contract, а
не удалённые stage-файлы.

## План 131: Unified Context Namespace (реализован 2026-09-09)

План закрыт после итерационного ревью и реализации Core/storage/runtime,
schema v102, authenticated IPC 261/106, redacted Electron Context Explorer,
typed detail-resolver unavailable gate, deterministic bounded retrieval и
restart-safe metadata persistence. В соответствии с текущим ограничением
Романа локальные тесты, сборки, линтеры и smoke-тесты не выполнялись; контракт
перенесён в `architecture.md`, подтверждённое состояние и evidence — в
канонические документы; комплект временных stage-файлов удалён.

## План 132: Durable Background Execution Plane (реализован 2026-09-09)

План закрыт после итерационного ревью и реализации Core/storage/runtime,
schema v103, authenticated IPC 262/107, generated Electron bindings и
developer-only Background Execution panel. Контракт расширяет существующий
`automation/v1`, а не создаёт второй scheduler/queue/lease/event authority;
recovery восстанавливает durable wakeups, fenced transitions и immutable
attempt outcomes. Локальные tests/builds/linters/smoke/E2E не запускались по
явному ограничению задачи; свежий CI остаётся unavailable до push.

## План 144: модульные релизы (реализован)

План 144 реализован в текущем checkout. Его scope:

1. манифест компонентов с версиями, совместимостью и hash/signature metadata;
2. выборочная транзакция обновления одного или нескольких компонентов;
3. recovery, backup, health marker и rollback для частичного обновления;
4. build pipeline, shell UI, verification и release evidence.

Full installer-релиз `installer` сохранён как fallback; реализация не меняет
release channel, установленный клиент или security boundary.

## Правила реализации

- работать в текущей ветке `main`, не создавать новую ветку без прямого запроса;
- перед работой проверять sync, `.codex`, проектные правила и чистоту дерева;
- не выносить runtime-состояние или бизнес-логику из Rust Core в renderer;
- любое изменение IPC обновлять на Rust и Electron сторонах с contract tests;
- новые Rust-функции и исправления покрывать тестами;
- сохранять sandbox, timeout, cancellation, approval и bounded resource limits;
- после изменений запускать быстрые релевантные checks и `git diff --check`;
- изменения коммитить task-only; `git push` выполнять только по прямому запросу.

## Gate для каждого этапа

Перед переходом к следующему этапу должны быть подтверждены:

1. contract/schema и миграция совместимы с предыдущей версией;
2. runtime владеет состоянием, recovery и cancellation;
3. IPC projection typed и authenticated, UI не получает лишних полномочий;
4. focused tests и релевантные Rust/Electron/package checks зелёные;
5. release evidence redacted, ссылки исправны, `git diff --check` проходит;
6. канонические `current-state.md`, `architecture.md` и при необходимости
   `release-evidence.md` обновлены, а закрытые plan-файлы удалены.

## Команды проверки

Полный список команд находится в [`../AGENTS.md`](../AGENTS.md). На рабочей
машине используются только быстрые проверки; полный набор запускается через
GitHub Actions workflow:

```powershell
pwsh -File .\scripts\documentation.tests.ps1
```

Для изменённого Electron/IPC-модуля дополнительно допустимы только узкие
`npm run check:protocol` и `npm run typecheck`; для изменённого Rust-crate —
точечные `cargo check` или тесты этого crate. Команды полного набора ниже в
`AGENTS.md` предназначены для GitHub Actions и не являются локальным smoke
прогоном.

На `push`/PR Windows workflow вычисляет изменённые workspace-crates и замыкает
граф их обратных зависимостей. Для этого набора выполняются format, Clippy,
тесты и `cargo build`; Electron отдельно проверяется и собирается только при
изменении Electron shell или desktop IPC proto. Полный Rust/Electron/native
package, installer и Windows acceptance gates запускаются только вручную через
`workflow_dispatch`. Полный локальный прогон запрещён рабочим процессом
проекта; локально выполняются только документационные, protocol/typecheck и
узкие проверки изменённых модулей. Описание workflow
находится в [`.github/workflows/`](../.github/workflows/).
