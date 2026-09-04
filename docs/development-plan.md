# План разработки EvoHime Desktop

Обновлено: 2026-09-04.

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
Рекомендуемый порядок планов:

`139 → 141 → 122 → 124 → 134 → 130 → 132 → 119 → 131 → 137 → 121 → 128 →
125 → 129 → 120 → 123 → 133 → 135 → 136 → 140 → 143 → 138 → 127 → 142`.

| Диапазон | Содержание | Статус |
| --- | --- | --- |
| 118 | persistent agent organization registry | реализован 2026-09-04 |
| 119–123 | execution profiles, grounded research, calibration, evidence ledger, context compression | очередь по графу |
| 124–129 | project quality, provider routing, design review, remote control, local inference, model cascade | очередь по графу |
| 130–135 | task leases, context namespace, durable background execution, deterministic utilities, resource guard, code review | очередь по графу |
| 136–140 | static-analysis packs, context loadouts, skill updates, capability facade, authorized security assessment | очередь по графу |
| 141–143 | service graph, program optimizer, project knowledge notebook | очередь по графу |
| 144 | модульный manifest и выборочное обновление компонентов | реализовано |

Планы 01–118 закрыты и удалены из временного каталога после переноса
контрактов и evidence в канонические документы. Нельзя считать план закрытым
по одному stage-файлу или по наличию кода: закрытие требует реализации,
recovery, IPC/UI при наличии, focused tests, release evidence и обновления
канонической документации.

## План 118: Persistent Agent Organization Registry

План 118 закрыт после итерационного ревью и реализации Core/storage/runtime,
authenticated IPC 259/104, Electron projection/UI, startup recovery,
focused/regression checks и переноса контракта в `architecture.md` и состояния
в `current-state.md`. Schema v92 также активирует пропущенную migration v91.

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
- после изменений запускать релевантные checks и `git diff --check`;
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

Полный список команд находится в [`../AGENTS.md`](../AGENTS.md). Для обычного
изменения используются:

```powershell
pwsh -File .\scripts\documentation.tests.ps1
cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc
cargo check -p evohime-supervisor
cd desktop\evohime-electron
npm run check:protocol
npm run typecheck
npm test
```

CI дополнительно выполняет native package, installer и Windows acceptance gates;
описание workflow находится в [`.github/workflows/`](../.github/workflows/).
