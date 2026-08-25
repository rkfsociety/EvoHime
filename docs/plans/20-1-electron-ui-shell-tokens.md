# План 20.1 — токены и базовая desktop-оболочка

Статус: готов к реализации после ревью плана 20.0.

## Цель

Заложить единую визуальную основу Евы: тёмную палитру, типографику,
геометрию, поверхности, focus states, shell layout, topbar и statusbar. Этап
не меняет навигацию, API или поведение отдельных панелей.

## Зависимости

### Блокирующие

- `20-0-electron-ui-visual-redesign.md`;
- текущие `App.tsx` и `styles.css`;
- существующий Electron renderer.

### Опциональные

- единый набор локальных SVG-иконок;
- screenshot regression tooling.

При отсутствии icon tooling используются текущие локальные символы с теми же
размерами и accessible names.

## Изменяемые поверхности

- `desktop/evohime-electron/src/renderer/src/styles.css`;
- shell-часть `App.tsx`;
- при необходимости небольшие presentation-only классы компонентов.

Не изменяются `src/shared/api.ts`, preload/main adapter и Core.

## Работы

1. Ввести scoped design tokens из спецификации: фон, sidebar, surfaces,
   borders, text, muted, accent, success, warning и danger.
2. Убрать зависимость от нативного серого фона `<button>` и унифицировать
   primary, secondary, ghost, danger, disabled и pressed states.
3. Собрать shell из sidebar, topbar, main body и statusbar с базовыми размерами
   и desktop-адаптацией.
4. Сохранить существующий текст connection state и его fail-visible семантику.
5. Сохранить верхний Trace trigger, listening indicator и update indicator;
   поменять только визуальную иерархию.
6. Добавить видимый focus-ring, `prefers-reduced-motion` и системные состояния
   hover/active/disabled без изменения обработчиков команд.
7. Проверить отсутствие горизонтального overflow на целевых размерах окна.

## Критерии приёмки

- shell визуально соответствует токенам спецификации;
- sidebar, topbar, body и statusbar не перекрывают друг друга;
- все текущие connection/update/listening тексты доступны;
- native button background не появляется в продукте;
- keyboard focus виден на каждом контроле;
- нет новых IPC-вызовов и изменений shared types;
- `npm run typecheck` и `npm run build` проходят.

## Проверки

```powershell
cd desktop\evohime-electron
npm run typecheck
npm run build
```

Ручные screenshots: connected, reconnecting, degraded, fatal, update
indicator, listening unknown и trace closed/open на 1366×768 и 1024×720.
