# План 197.0 — Изолированные сессии чатов (провайдер, модель, права и worktree)

Статус: active implementation contract. План использует существующие
`TaskCoordinator`, `ModelGateway`, `PermissionEngine`, `EventJournal` и
`BoundedTaskGroup`; второго Core-процесса или второго планировщика не создаёт.

## Цель
Сделать каждый чат независимой логической песочницей внутри одного процесса
Core: выбор провайдера, профиля, модели и режима разрешений (`PermissionMode`)
принадлежит конкретному чату и фиксируется на уровне `AgentRun`, а не
глобально для всего Core. Переключение провайдера или модели в композере больше
не перезапускает Core и не влияет на задачи, уже выполняющиеся в других чатах.
Для пишущих задач в одном git-репозитории вторым контуром добавляется
опциональная изоляция файловой системы через `git worktree`.

## Основание checkout
В текущем checkout параллельное выполнение задач в разных чатах уже работает на
уровне планировщика (`core_coordinator_routing.rs` запускает `StartTask` через
`tokio::spawn` с лимитом `BoundedTaskGroup` на 16 задач, а UI `TaskTimeline.tsx`
и `chat-summary.ts` отслеживают `running` независимо по `chatId`), но три
ключевых параметра остаются глобальными синглтонами на процесс Core:

1. **Провайдер:** `desktop/evohime-electron/src/main/shell-bridge.ts` при
   `provider.select` и `provider.save` вызывает `restartCore()`, а Core при
   старте получает конфигурацию только одного активного провайдера.
2. **Модель:** `StartTask` в `evohime.desktop.proto` не содержит полей
   `provider`/`profile_id`/`model`. `SelectModel` меняет общий `SelectedModel`,
   а `core_agent_execution.rs` читает `self.selected_model.get()` на каждой
   итерации цикла, из-за чего смена модели в одном чате меняет модель на лету
   у всех остальных активных чатов.
3. **Права (`PermissionMode`):** `PermissionMode` в
   `ipc_bridge_ambient_tasks_children.rs` мутирует общий `PermissionEngine`
   Core, из-за чего переключение `read_only` / `ask` / `full` в одном чате
   затрагивает остальные.

Дополнительно `ModelCatalog` опрашивает только `default_route`, а параллельные
пишущие чаты в одном `workspace_path` делят общую рабочую директорию.

## Граница
- Один процесс `evohime-core.exe` и один `evohime-supervisor.exe` сохраняются;
  отдельный ОС-процесс Core на чат или сообщение не запускается.
- Изоляция сессии чата распространяется на: `(provider, profile_id, model,
  permission_mode, execution_kind, preferred_route_hint)` и (опционально в
  этапе 2) `git worktree` для пишущих задач.
- Переключение уже настроенного провайдера и модели в композере не вызывает
  `restartCore()`. Перезапуск Core сохраняется только при сохранении/изменении
  API-ключей в Settings, чтобы сохранить текущую схему передачи секретов через
  DPAPI и окружение supervisor.
- Общий rate-limit cooldown по паре `(provider, model)` (`rate_limit_signal.rs`)
  остаётся процессным для защиты от каскадных `429` на одном API-ключе.
- Лимит `BoundedTaskGroup` (`DEFAULT_CAPACITY = 16`) сохраняется общим на Core.
- Существующие команды `SelectModel` и `PermissionMode` сохраняются как
  defaults/legacy-фоллбэк для CLI (`eva`) и вызовов без per-chat полей.

## Изменяемые контракты
- `crates/desktop-ipc/proto/evohime.desktop.proto` и генерируемые TS/Rust
  типы: расширение `StartTask` (поля `provider`, `profile_id`, `model`,
  `permission_mode`, `isolation_mode`) и `ModelCatalog` (запрос каталога по
  конкретному `provider` / `profile_id`).
- `crates/model-gateway`: мульти-маршрутный реестр всех настроенных профилей
  провайдеров вместо единственного `default_route`.
- `crates/evohime-core`: `SessionBinding` / per-run `(route, model,
  permission_mode)` в `CoreCommand::StartTask` и `core_agent_execution.rs`;
  изолированная проверка разрешений на уровне run; lifecycle `git worktree`
  песочницы для пишущих сессий.
- `desktop/evohime-electron`: хранение настроек сессии в `chats.json`,
  передача всех настроенных профилей провайдеров при старте Core, отказ от
  `restartCore()` в `provider.select` / композере, per-chat состояние в
  `ChatProviderPicker.tsx` и `ModelPicker.tsx`.

## Зависимости
### Блокирующие
- `evohime-desktop-ipc` и `npm run check:protocol`;
- `evohime-model-gateway` (`LiteRouterConfig`, `ProviderRoute`, `fetch_model_catalog`);
- `evohime-core` (`TaskCoordinator`, `core_agent_execution.rs`, `PermissionEngine`);
- `desktop/evohime-electron` (`shell-bridge.ts`, `provider-store.ts`,
  `chat-store`, `TaskTimeline.tsx`, `ChatProviderPicker.tsx`, `ModelPicker.tsx`).

### Опциональные
- Задел `taskWorktreeIsolation` и `agent_git_change_sets.rs` для этапа 2
  (файловая изоляция через `git worktree`); этап 1 (изоляция провайдера,
  модели и прав) самодостаточен и работает без worktree.
- Headless CLI `eva` продолжает работать через default-конфигурацию без
  изменения UX.

## Recovery и rollback
- При отсутствии явных per-session полей в `StartTask` (например, из CLI или
  старых вызовов) Core детерминированно использует `default_route`, глобальный
  `SelectedModel` и текущий `PermissionMode` — обратная совместимость не
  нарушается.
- Если запрошенный в `StartTask` провайдер не сконфигурирован, Core немедленно
  возвращает типизированную ошибку запуска задачи до создания `agent_run`.
- Временные `git worktree` (этап 2) при отмене или сбое Core очищаются при
  recovery по сохранённым метаданным в `EventJournal`, не затрагивая основное
  рабочее дерево.
- Rollback возможен на уровне версии модуля/приложения; схема `chats.json`
  расширяется аддитивно (опциональные поля).

## Этапы
- [Этап 1 — Core-контракт: пул провайдеров и per-run изоляция модели и прав](./197-1-chat-session-isolation.md)
- [Этап 2 — Runtime-интеграция: ModelCatalog per-route и опциональный git worktree](./197-2-chat-session-isolation.md)
- [Этап 3 — IPC, Electron per-chat состояние и композер без рестарта Core](./197-3-chat-session-isolation.md)
- [Этап 4 — Verification, документация и release evidence](./197-4-chat-session-isolation.md)

## Критерии готовности
- [ ] В двух параллельных чатах одновременно выполняются задачи на разных
      провайдерах и разных моделях без перезапуска Core и без взаимного влияния.
- [ ] Разные чаты одновременно работают с разными `PermissionMode` (`read_only`
      в одном, `full` в другом) без перезаписи прав друг друга.
- [ ] Переключение провайдера или модели в композере не вызывает `restartCore()`.
- [ ] `ModelCatalog` в композере загружает список моделей выбранного в чате
      провайдера, не меняя состояние соседних чатов.
- [ ] Опциональный режим `git worktree` изолирует правки параллельных пишущих
      чатов в одном репозитории и даёт контролируемый review/apply.
