# План 20.5 — настройки, trace и системные overlays

Статус: готов к реализации после завершения 20.4.

## Цель

Переработать modal и overlay-поверхности, которые должны выглядеть частью
одной оболочки: Settings, Trace, Update, Recovery и Repair status. Состояние и
разрешения остаются у действующих владельцев.

## Зависимости

### Блокирующие

- `20-1-electron-ui-shell-tokens.md`;
- `20-2-electron-ui-projects-and-account-menu.md`;
- `20-4-electron-ui-tool-panels.md`;
- `SettingsModal.tsx`, `ProviderForm.tsx`, `CodexPanel.tsx`,
  `ListenerRuntimeSection.tsx`, `SafetyPanel.tsx`, `TracePanel.tsx`,
  `UpdateGate.tsx`, `UpdateIndicator.tsx`.

### Опциональные

- reusable focus-trap helper. Без него допускается локальная реализация modal
  keyboard handling.

## Работы

1. Переработать Settings modal: header, close, backdrop, focus, tabs и
   responsive layout.
2. Сохранить вкладки provider/models, Codex CLI, workspace, speech, appearance
   и security.
3. Сохранить provider get/save/clear, restart indication и отсутствие ключа в
   renderer; Codex install/login/refresh/model/limits; speech runtime;
   microphone permission.
4. Переработать Trace side panel: current-chat filter, summary, events,
   payload, empty/loading/error и `Сохранить .md`.
5. Сохранить Escape/close/focus return для Settings и Trace.
6. Переработать UpdateIndicator/UpdateGate по точному `UpdatePhase`:
   disabled, idle, checking, up-to-date, available, preparing, ready,
   applying, failed.
7. Показать update message/detail/steps, progress, commit/branch, blocking,
   restartRequired, skip и restart actions.
8. Проверить stacking: blocking update gate не маскирует critical recovery,
   trace не ломает modal и не перекрывает важные действия.

## Критерии приёмки

- Settings открывается из account menu и закрывается кнопкой, Escape и backdrop;
- все пять settings tabs доступны и не меняют текущие API;
- секреты и provider values не появляются в визуальном trace или logs;
- trace показывает только события выбранного чата;
- update phases визуально различимы и не обещают неподтверждённый результат;
- overlays не создают двойных backdrop/focus traps;
- narrow window не обрезает modal controls.

## Проверки

- settings provider/Codex/speech/security tests;
- trace empty/populated/export/error tests;
- update indicator/gate phase tests;
- keyboard-only modal pass;
- `npm test -- --run`, `npm run typecheck`, `npm run build`.
