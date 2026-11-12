# EvoHime desktop UI — дизайн и поведение

Обновлено: 2026-11-12.

## Продуктовая модель

Основная поверхность строится вокруг трёх постоянных уровней:

1. **Глобальная навигация** — чат, расписание, обзор, ревью планов, память и
   Pulse, workflow, продолжения, анализ, речь и задачи для человека. Она
   представлена компактной вертикальной панелью значков с доступными
   accessibility labels и tooltip.
2. **Рабочий контекст** — выбор workspace, создание диалога и список чатов этой
   папки. Эта колонка не смешивается с глобальными инструментами.
3. **Рабочая область** — выбранный раздел; в чате сохраняются история, решения
   по разрешениям, выбранный provider/model и быстрые дополнительные панели.

Профиль и настройки находятся в нижней части левой области. Глобальные
разделы не спрятаны внутри меню профиля.

## Основной shell

- App.tsx владеет только UI-навигацией и presentation state.
- ProjectSidebar.tsx отображает рабочую папку и её диалоги, используя
  существующие workspace.list, workspace.select, workspace.pick,
  chat.list, chat.remove.
- TaskTimeline.tsx отображает transcript и composer. Выбор workspace
  перенесён из composer в context rail; composer явно показывает текущую папку
  и сохраняет permission, provider, model, context usage, attachments,
  web-search и start/stop actions.
- SettingsModal.tsx использует tabs/tabpanel ARIA contract, переключение
  вкладок клавишами стрелок и Escape для закрытия.
- TracePanel, WorkbenchPanel и browser session остаются вспомогательными
  поверхностями и не владеют состоянием Core.

## Навигация и клавиатура

- Sidebar разделён на icon rail и contextual workspace/chat column.
- У каждого значка есть доступное имя и title; активный раздел получает
  отличимое, но сдержанное выделение.
- Ctrl+K / Cmd+K создаёт новый диалог и передаёт фокус в composer.
- Ctrl+, / Cmd+, открывает настройки.
- Escape закрывает открытые меню/диалоги согласно локальному владельцу.

## Composer

Composer визуально группирует область проекта и branch metadata, permission
mode, provider/model и context usage над полем ввода. Поле запроса занимает
основную ширину; attach, web search и run/stop остаются отдельными явными
действиями. Enter отправляет, Shift+Enter добавляет строку. Отсутствие
workspace не скрывает composer и не отключает обычный диалог; project-only
attachments корректно остаются недоступны без выбранной папки.

## Визуальные токены

- Dark baseline: глубокий slate фон, слегка отличимый surface для sidebar,
  основного содержимого и cards; light theme использует нейтральный светлый
  canvas.
- Accent используется для active navigation, focus и основных действий; success,
  warning и danger показывают состояние, а не декор.
- Радиусы 8/12/18 px; отступы основаны на шаге 4 px; интерактивные элементы
  имеют target примерно 36–42 px.
- Контент не растягивается на всю ширину: сообщения и composer имеют
  читаемую максимальную ширину.
- Тени используются только для overlay/dialog и основных интерактивных
  поверхностей; обычные панели различаются прежде всего поверхностью и
  границей.

## Адаптивное поведение

- Корень использует размер реального viewport; CSS не задаёт минимальную
  ширину, превышающую фактическую область окна.
- При ширине до 1180 px Trace становится overlay.
- При ширине до 980 px Workbench/Browser становятся bounded drawer.
- При малой высоте уменьшаются вертикальные интервалы и навигационные hit
  areas остаются не менее 37 px.
- Sidebar может сворачиваться до icon rail; в свёрнутом состоянии сохраняются
  глобальные destinations, профиль и настройки.
- Settings на узкой области перестраивают navigation tabs горизонтально.

## Границы и честность UX

- Renderer не придумывает runtime-status и не выводит решение об approval.
- Status labels должны отражать значения, пришедшие из Shell/Core; неизвестное
  состояние не представляется как «выключено» или «успешно».
- Projects/chat history ограничены существующими shell APIs; интерфейс не
  симулирует возможности, которых нет в Core.
- Никакие изменения в этом UI pass не меняют Core command semantics,
  permission checks, durable state или IPC schema.

## Проверки

Обязательные автоматические gates:
- npm run check:protocol
- npm run typecheck
- npm test
- npm run build
- npm run check:bundle
- module-router and compatible-manifest CI

Для завершённого визуального acceptance отдельно проверяются реальные
Electron screenshots на Windows: 1360×860, 1100×760, 960×640; dark/light
themes; collapsed/expanded rail; Trace + Workbench + Browser combinations;
long prompts and long tool output; settings dialog; keyboard navigation.
Static CSS contract tests не заменяют этот runtime screenshot audit.
