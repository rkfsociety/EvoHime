# План 06-3 — IPC, Electron projection и workflow-рецепты

## Цель

Дать Electron минимальную пользовательскую поверхность для запуска и
наблюдения workflow, не перенося в renderer планирование или выполнение.

## Зависимости

### Блокирующие

- [06-2](06-2-workflow-runtime.md);
- versioned protobuf contract и generated TypeScript protocol;
- существующие task timeline, approval, cancellation и OperationsPanel.

### Опциональные

- визуальный редактор графа. До его появления workflow запускается из
  Core-owned шаблона или read-only JSON/Markdown preview;
- drag-and-drop layout. До его появления UI использует стабильную раскладку
  по topological order.

## Изменения

1. Добавить additive IPC-команды:
   `ListWorkflowTemplates`, `GetWorkflowDefinition`, `StartWorkflow`,
   `GetWorkflowRun`, `CancelWorkflow`, `ResolveWorkflowApproval` и
   `ListWorkflowEvents`.
2. Добавить typed events: run/node started, waiting approval, progress,
   child report accepted/rejected, degraded, failed, cancelled и completed.
   События должны поддерживать replay и bounded payloads.
3. В Electron main/preload проксировать только Core projection: node IDs,
   role/status, progress, bounded error, approval preview и references.
   Prompt, raw child output, secrets и unrestricted context в renderer не
   отдавать.
4. Добавить экран/панель workflow с графом, текущими состояниями узлов,
   зависимостями, попытками, approval и ссылками на task timeline.
5. Добавить стартовые шаблоны по CAMEL-подобным ролям:
   `Исследование репозитория`, `План → реализация → ревью`,
   `Параллельное security review`. Шаблоны являются Core-owned versioned
   definitions, а не динамически загружаемыми Python agents.
6. Обеспечить старому Electron-клиенту graceful handling неизвестных
   additive events и состояния `unknown_state`/`core_unavailable`.

## Проверки

- `npm run check:protocol`;
- protocol serialization tests для всех новых сообщений;
- main/preload security tests на отсутствие raw prompt/output/secrets;
- UI tests для replay, reconnect, cancel, approval и Core unavailable;
- real-Core E2E на запуске одного шаблона без внешнего web runtime;
- `npm run typecheck` и `npm test`.

## Готово, когда

Пользователь может запустить шаблон, видеть полный bounded граф и принять или
отклонить approval, а renderer ни разу не вычисляет зависимости и не запускает
узлы самостоятельно.
