# План разработки EvoHime Desktop

Обновлено: 2026-09-21.

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
Текущий активный каталог содержит планы `173–181`; точные темы, статусы и
зависимости новых направлений находятся в [`plans/README.md`](plans/README.md).
Планы 01–172 перенесены в канонические документы, а закрытые MVP-планы
`127–130` не входят в очередь повторно.

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
| 133 | built-in developer utilities | реализован 2026-09-09 |
| 134 | host resource telemetry & pressure guard | реализован 2026-09-09 |
| 135 | Core-owned Code Review Lane | реализован 2026-09-09 |
| 136 | evidence-preserving static analysis packs | реализован 2026-09-09 |
| 137 | agent context loadouts | реализован 2026-09-09 |
| 138 | skill source & update lifecycle | реализован 2026-09-09 |
| 139 | kernel capability facade | реализован 2026-09-09 |
| 140 | authorized security assessment lane | реализован 2026-09-09 |
| 144 | модульный manifest и выборочное обновление компонентов | реализовано 2026-09-04 |
| 149–167 | review, model compare, policy, suggestions, experiments, computer use, execution board, grounding, temporal memory, IDE, checkpoints, diagrams, compatibility, motion, recipes, voice output, command center | реализованы 2026-09-09 |
| 168 | Hardware Fit Evidence Catalog | закрыт 2026-09-14 |
| 169 | Agent Client Protocol Bridge | закрыт 2026-09-14 |
| 170 | Multi-Reviewer Ensemble & Adjudication | закрыт 2026-09-14 |
| 171 | Language Intelligence Runtime | закрыт 2026-09-15 |
| 172 | self-healing updater и recovery без нового модуля | закрыт 2026-09-15 |

Планы 01–172 закрыты и удалены из временного каталога после переноса
контрактов и evidence в канонические документы. Нельзя считать план закрытым
по одному stage-файлу или по наличию кода: закрытие требует реализации,
recovery, IPC/UI при наличии, focused tests, release evidence и обновления
канонической документации.

Подробности закрытых планов не дублируются здесь: их контракты находятся в
`architecture.md`, подтверждённое состояние — в `current-state.md`, а проверки
и release-gates — в `release-evidence.md`. Каталог `docs/plans/` содержит только
незавершённые планы.

## Правила реализации

- работать в текущей ветке `main`, не создавать новую ветку без прямого запроса;
- перед работой проверять sync, `.codex`, проектные правила и чистоту дерева;
- не выносить runtime-состояние или бизнес-логику из Rust Core в renderer;
- любое изменение IPC обновлять на Rust и Electron сторонах с contract tests;
- новые Rust-функции и исправления покрывать тестами;
- сохранять sandbox, timeout, cancellation, approval и bounded resource limits;
- после изменений запускать быстрые релевантные checks и `git diff --check`;
- изменения коммитить task-only; push выполнять по правилам корневого
  `AGENTS.md`.

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
машине по умолчанию используются быстрые проверки, а полный набор
предпочтительно запускается через GitHub Actions workflow:

```powershell
pwsh -File .\scripts\documentation.tests.ps1
```

Для изменённого Electron/IPC-модуля дополнительно допустимы только узкие
`npm run check:protocol` и `npm run typecheck`; для изменённого Rust-crate —
точечные `cargo check` или тесты этого crate. Команды полного набора ниже в
`AGENTS.md` предназначены для GitHub Actions и не являются локальным smoke
прогоном.

Windows native package acceptance workflow запускается только вручную через
`workflow_dispatch`. Он не подписан на `push`/PR, не вызывается центральным
`module-router` и не входит в release path. Центральный `module-router`
dispatch’ит только затронутые module workflows, compatibility manifest и
единственный web-installer workflow. Полный локальный прогон не требуется по
умолчанию, но не запрещён: объём локальной проверки выбирается по риску и
области изменения. Описание workflow
находится в [`.github/workflows/`](../.github/workflows/).
