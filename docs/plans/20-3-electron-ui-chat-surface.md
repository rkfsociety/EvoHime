# План 20.3 — home, чат, timeline и composer

Статус: готов к реализации после завершения 20.2.

## Цель

Переработать основную рабочую поверхность Евы: home выбранного проекта,
историю сообщений, activity, approval, recovery, routing, repository status и
composer. Логика выполнения задач остаётся в текущих компонентах и API.

## Зависимости

### Блокирующие

- `20-1-electron-ui-shell-tokens.md`;
- `20-2-electron-ui-projects-and-account-menu.md`;
- `TaskTimeline.tsx`, `HomeScreen.tsx`, `ActivityLine.tsx`,
  `MarkdownMessage.tsx` и существующие child components.

### Опциональные

- screenshot fixtures для длинного Markdown и многострочного composer.

## Работы

1. Переработать home: приветствие, активный проект, стартовые prompts,
   empty-state без проекта и unavailable Core.
2. Сохранить открытие/создание чата из первого prompt и действующие
   `chat.open`, `chat.create`, `chat.appendPrompt`.
3. Переработать user/agent message bubbles, timestamps, copy actions,
   Markdown headings/lists/tables/code и длинные ответы.
4. Переработать `ActivityLine` и working state так, чтобы технический прогресс
   был компактным и не выглядел ответом агента.
5. Переработать approval card: tool, permission, scope, preview, command,
   cwd/path/details, truncation, allow/reject/cancel, pending/resolved/error.
6. Переработать `RecoveryBanner` и `RoutingStatus`: connection failure,
   resync, preferred route, fallback, pending routing approval и expiry.
7. Переработать `RepositoryBar` без изменения repository IPC.
8. Переработать composer: textarea, отправка, остановка, модель, permission
   mode, Coding-задача (Codex CLI), ContextUsage и ошибки.
9. Сохранить ограничения: composer disabled без workspace/Core, stop только
   для активной задачи, approval идемпотентен, ошибки остаются видимыми.

## Критерии приёмки

- новый проект показывает понятный home, а выбранный чат — полноценный
  timeline;
- первый prompt корректно создаёт чат как сейчас;
- approval не теряется при reconnect и не подтверждается двойным кликом;
- routing decision и resync имеют отдельные понятные состояния;
- composer не перекрывает timeline и не выходит за границы окна;
- Coding toggle, модель, доступ, маршрут и контекст видимы;
- нет новых Core/IPC решений в renderer.

Проверяются все текущие `ConnectionState`: `starting`, `connecting`,
`connected`, `reconnecting`, `replaying`, `resyncing`, `state-gap`,
`version-mismatch`, `degraded` и `fatal`.

## Проверки

- tests для chat open/create, prompt, stop, approval и recovery;
- tests для routing pending/resolve/resync;
- длинный Markdown и composer с Shift+Enter;
- `npm test -- --run`, `npm run typecheck`, `npm run build`.
