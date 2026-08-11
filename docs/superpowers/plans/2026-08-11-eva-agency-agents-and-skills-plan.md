# План развития Евы: роли, skills и управляемый lifecycle

## Цель и границы

Добавить поверх существующего плана `2026-08-11-eva-opencode-inspired-improvements.md` слой специализированных ролей и production-grade skills. Ева должна выбирать подходящий рабочий режим, применять проверяемый workflow и показывать пользователю результат, не превращая WinUI в место хранения состояния или бизнес-логики.

Границы продукта сохраняются: WinUI 3 — thin client; `evohime-core` владеет workspace, tools, approvals, orchestration и SQLite; supervisor отвечает за lifecycle; единственный транспорт — versioned named-pipe `desktop-ipc-v1`. Web UI/Vite и перенос чужой runtime-архитектуры не нужны.

## Выводы по источникам

- [`agency-agents`](https://github.com/msitarzewski/agency-agents) полезен как каталог ролей: у специалиста есть identity/personality, mission, workflow, deliverables, success metrics и стиль коммуникации. Категории/division помогают искать нужную экспертизу, а нативный каталог с установкой подсказывает UX-паттерн управления коллекцией.
- [`agent-skills`](https://github.com/addyosmani/agent-skills) полезен как операционная модель: DEFINE → PLAN → BUILD → VERIFY → REVIEW → SHIP, slash-команды, автоматический выбор skill, отдельные evals/hooks/references и plugin packaging. Важный принцип — автоматизация сокращает ручные переходы, но не отменяет тесты, approval и остановку на рисковых шагах.
- Переносить нужно контракты и quality gates, а не тексты prompts целиком: роль не должна обходить allowlist, sandbox, approval или IPC boundary.

## Матрица идей и решений

| Идея источника | Решение для Евы | Приоритет / ценность |
| --- | --- | --- |
| Каталог divisions и специалистов | Встроенный каталог `RoleDefinition` с фильтрами по задаче, стеку, риску и доступным инструментам | P0 / предсказуемый выбор режима |
| Identity и personality роли | Короткий tone/communication profile, который влияет только на объяснение результата, не на права | P1 / понятный UX |
| Mission/workflow/deliverables | Структурированный контракт роли с входами, шагами, артефактами и acceptance criteria | P0 / меньше расплывчатых задач |
| Lifecycle `/spec`…`/ship` | Команды Core, каждая с отдельным состоянием и проверками | P0 / воспроизводимый процесс |
| Automatic skill discovery | Детерминированный match по намерению, файлам, языкам и текущему этапу; пользователь видит и может изменить выбор | P0 / скорость без скрытой магии |
| Evals, hooks, references | Версионируемые fixtures, pre/post hooks и ссылки на локальные правила проекта | P0 / измеримое качество |
| `/build auto` | Автоматически исполнить утверждённый план, но остановиться на approval, failure или scope change | P1 / экономия ручных переходов |
| Plugin/package installation | Подписанный/проверенный пакет skills без произвольного исполнения install-скриптов | P1 / расширяемость и безопасность |
| Multi-agent orchestration | Дочерние read-only роли для исследования, ревью и тест-планирования; запись делает только родитель | P1 / параллельная экспертиза |

## Модель Skill и Role

В Core ввести версионируемые определения:

```text
SkillDefinition {
  id, version, title, description, triggers,
  lifecycle_stage, required_context, references,
  allowed_tools, risk_class, approval_policy,
  steps, deliverables, acceptance_criteria,
  eval_suite, hooks, author, source, integrity
}

RoleDefinition {
  id, division, identity, mission, communication_style,
  skill_ids, default_model_route, read_only, delegation_policy
}
```

`allowed_tools` и `risk_class` вычисляются Core и проверяются до каждого вызова; prompt не может расширить права. `deliverables` должны быть машиночитаемыми: план, diff, тестовый отчёт, review report, release evidence. Поля `source`, `version` и `integrity` позволяют отклонить изменённый или устаревший пакет. Содержимое skills хранится как локальные данные, но secrets, токены и полный чувствительный контекст в definition не включаются.

## Первый каталог native skills и ролей

Начальный набор ограничить coding-agent и Windows-native проектом:

1. `codebase-onboarding` / Codebase Onboarding — read-only карта репозитория, архитектуры и рисков.
2. `spec-driven-development` / Product Spec — превращает запрос в scope, non-goals, IPC/UI/Core impact и acceptance criteria.
3. `planning-and-task-breakdown` / Planner — атомарный roadmap с зависимостями и проверками.
4. `native-windows-implementation` / Native Windows Engineer — WinUI 3, Rust MSVC, named pipe, packaging.
5. `rust-test-driven-development` / Rust Test Engineer — red-green-refactor, unit/integration/compatibility tests.
6. `winui-ux-quality` / WinUI UX Reviewer — native layout, accessibility, truthful state и visual smoke.
7. `code-review-and-quality` / Code Reviewer — correctness, security, maintainability, scope и regression review.
8. `security-and-privacy-audit` / Security Engineer — secrets, path boundaries, prompt injection, approvals, DPAPI/Credential Manager.
9. `release-and-packaging` / Release Engineer — manifest, installer, SHA-256, rollback и clean-machine smoke.
10. `minimal-change` / Minimal Change Engineer — task-only diff, сохранение пользовательских изменений и проверка `git diff --check`.

Каждая роль должна ссылаться на skills, а не копировать их текст. Роли с write/terminal/commit получают risk classification и обязательный approval.

## Жизненный цикл и slash-команды

Core реализует состояния `defined`, `planned`, `building`, `verifying`, `reviewing`, `ready_to_ship`, `blocked`, `completed`.

| Команда | Результат | Ограничения |
| --- | --- | --- |
| `/spec` | spec/PRD и вопросы к scope | read-only |
| `/plan` | атомарный план и dependency graph | read-only |
| `/build` | выполнение одной утверждённой задачи | approval перед записью/опасным tool |
| `/build auto` | последовательное выполнение утверждённого плана | пауза на failure, scope change и risk |
| `/test` | свежие проверки и evidence | не маскировать failures |
| `/review` | review report с приоритетами | read-only |
| `/webperf` | не вводить web runtime; использовать для разрешённых browser/UI performance checks | только если задача реально касается browser tool |
| `/code-simplify` | минимальный refactor с regression tests | без изменения публичного поведения |
| `/ship` | delivery checklist, commit readiness и release evidence | commit/push только по политике пользователя |

Команды должны быть Core-командами через IPC, а не локальным парсером WinUI. История, текущий skill, approval и trace восстанавливаются после reconnect.

## Discovery, activation, install и update

1. При старте задачи Core строит список кандидатов по trigger, файлам, языку, lifecycle stage и проектным правилам.
2. Core отбрасывает несовместимые версии, отсутствующие references и skills с недопустимым risk profile.
3. UI показывает выбранную роль/skill, причины выбора, инструменты и риск; пользователь может закрепить или сменить кандидат.
4. Activation создаёт snapshot definition/version в task trace. Во время задачи skill immutable.
5. Установка принимает только локальный архив или HTTPS-источник с manifest, размером, hash и совместимостью protocol/runtime; install scripts запрещены по умолчанию.
6. Update staged, проверяется hash/signature, затем атомарно активируется; предыдущая версия остаётся для rollback. Удаление активного skill запрещается.

## Дочерние read-only роли

Родитель может делегировать ограниченные задачи: onboarding, поиск по коду, threat-model review, test-plan review и документацию. Child получает урезанный context и отдельный `child_task_id`, не имеет write, shell, commit, install или network mutation tools. Результат — структурированный report с источниками и confidence, который родитель обязан проверить перед включением в plan/build. UI показывает parent/child relation, status и cost/elapsed time.

## Evals, hooks, quality gates и telemetry

- Evals: fixtures для выбора skill, соблюдения tool allowlist, корректности diff, отказа от prompt injection, IPC compatibility, cancellation и восстановления после reconnect.
- Hooks: `before_context`, `before_tool`, `after_tool`, `before_commit`, `after_task`; hooks только наблюдают/отклоняют по policy и не получают секреты.
- Quality gates: spec completeness, plan approval, clean scope, targeted tests, `cargo fmt --all -- --check`, `cargo test`, WinUI tests, package smoke, `git diff --check` и визуальный smoke для UI.
- Telemetry: локальный JSONL/SQLite audit trail — выбранные skill/role versions, tool calls, approvals, durations, failures и test evidence. Содержимое секретов и чувствительных файлов редактируется до записи; telemetry export требует явного действия.

## Изменения в Core, IPC, SQLite и WinUI

### Core/Rust

- Добавить registry/parser/validator definitions, deterministic matcher и lifecycle coordinator.
- Вынести risk/allowlist enforcement в Core перед dispatch tool; добавить child-task coordinator и eval runner.
- Добавить package cache с hash/signature, staged update и rollback.
- Покрыть каждую новую функцию unit/integration tests; отдельные fixtures проверяют несовместимые manifest и path escape.

### IPC

- Расширить `crates/desktop-ipc/proto/evohime.desktop.proto` сообщениями `SkillCatalog`, `SkillSelection`, `LifecycleCommand`, `ChildTask`, `EvalReport`, `PackageUpdate` и соответствующими events.
- Сохранить major compatibility, sequence replay и bounded frame size; добавить Rust/C# compatibility fixtures и unknown-field tests.

### SQLite

- Таблицы `skill_definitions`, `role_definitions`, `task_skill_snapshots`, `lifecycle_steps`, `child_tasks`, `eval_runs`, `package_versions`.
- Миграция транзакционная, backup перед schema change, уникальность `(id, version)`, retention для trace и статус rollback.

### WinUI 3

- Catalog page с фильтрами, карточкой роли, risk/tools и source/version.
- В composer — lifecycle stage, slash-command suggestions, selected role/skill и approval state.
- Timeline — child tasks, evals, hooks, evidence и понятный blocked state.
- UI не читает workspace/SQLite и не запускает installer: всё запрашивается через IPC.

## Roadmap, зависимости и проверки

### Этап 1 — контракт и registry

Артефакты: protobuf messages, Rust definitions/validator, SQLite migration, десять seed definitions, compatibility fixtures. Зависит от текущих IPC/SQLite foundations. Проверки: `cargo fmt --all -- --check`, `cargo test -p desktop-ipc -p evohime-local-storage -p evohime-core`, WinUI IPC tests, migration backup/rollback. Выход: invalid risk/tool/package не проходит validator.

### Этап 2 — lifecycle и `/spec`/`/plan`/`/test`/`/review`

Артефакты: Core coordinator, persisted steps, command palette и timeline. Проверки: reconnect/replay, cancellation, read-only enforcement, exact acceptance criteria и свежие targeted tests. Выход: режимы plan/build не смешиваются, trace воспроизводим.

### Этап 3 — activation/discovery и native catalog

Артефакты: matcher, explanation of selection, WinUI catalog/detail view, pinning и per-project activation. Проверки: deterministic fixtures, user override, unknown skill, version conflict и визуальный smoke. Выход: UI честно показывает активную роль и не скрывает риск.

### Этап 4 — child roles, evals и hooks

Артефакты: child-task protocol, read-only sandbox, eval runner, hook API и local audit events. Проверки: child write/shell/commit denial, prompt-injection fixtures, timeout/cancel, bounded output, no secret leakage. Выход: дочерние отчёты не могут менять workspace.

### Этап 5 — packages, `/build auto`, `/code-simplify`, `/ship`

Артефакты: signed/hash-checked package cache, staged update/rollback, approved-plan executor и delivery checklist. Проверки: corrupted archive, rollback, approval pause, scope drift, task-only commit, native package smoke и clean Windows install. Выход: автоматизация не обходит gates.

## Критерии готовности

- Для запроса видны выбранные role/skill, версия, причины выбора, риск, tools и acceptance criteria.
- Один и тот же input с одинаковым registry/context выбирает одинаковый skill.
- Plan не может менять workspace; build не меняет файлы без approval.
- Child roles доказуемо read-only; invalid package и path escape отклоняются.
- Каждая стадия имеет persisted trace, eval evidence и понятный failure/blocked state.
- IPC compatibility, SQLite migration/backup, Rust/WinUI tests, package smoke и `git diff --check` проходят свежую проверку.
- Native installer/runtime остаются единственным пользовательским продуктом; секреты не попадают в prompts, trace или package metadata.

## Что не переносить

- Не копировать сотни внешних personas и бизнес-division (sales, paid media, social marketing) в core-релиз; их можно рассматривать только как будущие пользовательские packs.
- Не делать personality источником полномочий или причиной автоматического выбора опасных tools.
- Не импортировать чужой installer, shell scripts, multi-tool filesystem layout или desktop runtime.
- Не превращать Markdown skill в исполняемый код и не разрешать skill самовольно менять policy, IPC, secrets или approval.
- Не возвращать web UI/Vite и не переносить состояние из Rust Core в WinUI.
