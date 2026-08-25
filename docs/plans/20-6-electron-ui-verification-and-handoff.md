# План 20.6 — финальная проверка и передача UI

Статус: готов к реализации после завершения 20.5.

## Цель

Доказать, что визуальная переработка соответствует спецификации, не потеряла
действующие функции и может быть передана пользователю без изменений Core или
IPC.

## Зависимости

### Блокирующие

- этапы `20-1`–`20-5` завершены;
- обновлённые component/contract tests;
- собранный Electron package для runtime-проверки.

### Опциональные

- реальный собранный Core для E2E;
- screenshot comparison tooling.

Без реального Core выполняются доступные renderer tests и фиксируется
пропущенный E2E gate; это не заменяет package smoke.

## Матрица проверки

Снять и проверить screenshots на 1366×768 и 1024×720 для:

- no project, project selected, project chooser, unavailable project;
- no chats, chat list, home, active chat, long response;
- approval pending/resolved/error;
- Core connected/reconnecting/state-gap/degraded/fatal;
- routing normal/fallback/pending decision;
- Overview, Review, Operations, Workflow, Listening;
- Settings по каждой вкладке;
- Trace empty/populated/export status;
- update indicator, update gate, each real phase;
- repair available/running/ready-to-commit/CI/failed;
- ambient confirmation dialogs;
- account menu opened upward and keyboard focus states.

## Автоматические проверки

```powershell
cd desktop\evohime-electron
npm run check:protocol
npm run typecheck
npm test -- --run
npm run build
npm run check:bundle
npm run package
```

Дополнительно запускаются package smoke и real-Core E2E по правилам `AGENTS.md`,
если доступен собранный Core.

## Функциональная сверка

Перед завершением составить diff-checklist по текущим UI surfaces:

- каждый renderer component из `src/renderer/src` учтён;
- каждый пользовательский action и disabled condition сохранён;
- `workspace.*`, `chat.*`, `provider.*`, `identity.get`, `repository.get` и
  Core commands не изменены;
- нет новых filesystem/network/shell возможностей в renderer;
- approval, idempotency, confirmation и fail-visible состояния сохранены;
- нет отдельной вкладки «Диалог» и нет дублирующей навигации в sidebar.

## Критерии завершения

- все обязательные команды и проверки проходят;
- screenshots просмотрены, в том числе overlays и destructive confirmations;
- `git diff --check` проходит;
- рабочая копия содержит только task-only изменения;
- после публикации контракты переносятся в `docs/architecture.md`,
  подтверждённое состояние — в `docs/current-state.md`, а временные планы
  `20-*` удаляются согласно правилам каталога.

## Rollback

До завершения этапа 20.6 каждый визуальный этап должен быть отдельным
task-only коммитом. Откат должен затрагивать только renderer presentation и
CSS; данные, Core, IPC и пользовательские workspace/chats не мигрируют и не
откатываются.
