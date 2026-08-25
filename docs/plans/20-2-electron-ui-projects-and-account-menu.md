# План 20.2 — проекты, чаты и меню пользователя

Статус: готов к реализации после завершения 20.1.

## Цель

Переработать левую панель так, чтобы проект и его чаты были постоянно
понятны, а глобальные разделы и настройки открывались из меню пользователя,
которое раскрывается вверх.

## Зависимости

### Блокирующие

- `20-0-electron-ui-visual-redesign.md`;
- `20-1-electron-ui-shell-tokens.md`;
- действующие `ProjectSidebar.tsx`, `App.tsx`, `SettingsModal.tsx`.

### Опциональные

- отдельный reusable popover primitive. Без него используется локальный
  presentation-компонент с теми же focus/escape правилами.

## Работы

1. Переработать карточку проекта: имя, workspace path, доступность, active
   state и «Сменить проект».
2. Сделать project chooser с известными проектами, недоступной папкой,
   «Выбрать папку…», выбором и «Забыть проект».
3. Сохранить вызовы `workspace.list`, `workspace.pick`, `workspace.select` и
   `workspace.forget` без изменения payload/result.
4. Переработать список чатов проекта: active state, «+ Новый чат», empty,
   busy/error и отдельная кнопка удаления.
5. Сохранить `chat.list`, `chat.create`, `chat.open`/selection и `chat.remove`;
   не добавлять подтверждение удаления, если его нет в текущем поведении.
6. Убрать постоянную копию глобальных tool tabs из sidebar.
7. Перенести Обзор, Ревью планов, Память и Pulse, Составные задачи, Слух и
   Настройки в account menu.
8. Реализовать открытие меню вверх, закрытие по Escape/outside/repeat click,
   focus return и `aria-expanded`/`aria-current`/`aria-selected`.
9. Сохранить UpdateIndicator в account area и его действующие действия.

## Критерии приёмки

- проект можно выбрать и сменить без потери чатов предыдущего workspace;
- отмена native picker не меняет выбранный проект;
- недоступный проект виден как недоступный, а не исчезает молча;
- чаты не смешиваются между проектами;
- глобальные разделы присутствуют только в меню пользователя;
- отдельного navigation item «Диалог» нет;
- меню доступно мышью и клавиатурой, фокус возвращается на trigger;
- IPC-команды и обработчики остаются прежними.

## Проверки

- component tests project list, empty/error/unavailable project, chat list и
  account menu;
- keyboard pass: Tab, Enter, Escape, outside click;
- `npm run typecheck`;
- `npm test -- --run`.
