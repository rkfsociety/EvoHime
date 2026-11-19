# План 197.1 — Core-контракт: пул провайдеров и per-run изоляция модели и прав

Статус: active implementation contract. Этап следует после overview 197.0.

## Scope и изменяемые контракты
1. **Пул провайдеров в `model-gateway` и Core (`ModelGateway`):**
   - Расширить конфигурацию запуска Core так, чтобы в Core при старте
     передавались все настроенные профили провайдеров (ключ, `base_url`,
     `provider`, `profile_id`, `tier`, `request_pacing`, `model_rate_limits`),
     а не только один `default_route`.
   - Хранить в `ModelGateway` карту инстансов провайдеров по стабильному
     ключу маршрута `ProviderRouteKey { provider, profile_id }` с fallback на
     `default_route`.
   - Сохранить общий `rate_limit_signal` по паре `(provider, model)` для
     корректной защиты от `429` при параллельных чатах на одном ключе.
2. **Неизменяемая привязка сессии на уровне задачи (`SessionBinding`):**
   - Добавить в `CoreCommand::StartTask` и контекст запуска структуру
     `SessionBinding`: `provider: Option<String>`, `profile_id: Option<String>`,
     `model: Option<String>`, `permission_mode: Option<PermissionMode>`.
   - В `core_agent_execution.rs` убрать чтение глобального
     `self.selected_model.get()` внутри цикла итераций: задача один раз при
     старте фиксирует свой `effective_route` и `effective_model` (с fallback
     на `default_route` и `SelectedModel`, только если в `StartTask` они не
     заданы).
3. **Per-run изоляция `PermissionMode`:**
   - Вместо мутации единого `PermissionEngine` при каждом переключении в UI,
     передавать `permission_mode` конкретной задачи в контекст выполнения
     инструментов / approval-проверку этого `AgentRun`.
   - Глобальный `PermissionEngine` остаётся владельцем общих пользовательских
     правил (`permissions.json`) и дефолтного режима для вызовов без
     `SessionBinding`.
4. **In-crate unit-тесты:**
   - Параллельный запуск двух `AgentRun` с разными `(provider, model)` и
     проверка, что запросы уходят в разные маршруты с разными `model` без
     взаимного переключения.
   - Параллельный запуск двух `AgentRun` с разными `PermissionMode` (`read_only`
     и `full`) и проверка, что режим одного не ослабляет и не блокирует
     второй.

## Зависимости
### Блокирующие
Overview 197.0; `evohime-model-gateway`; `evohime-permissions`;
`crates/evohime-core/src/core_coordinator_routing.rs` и
`crates/evohime-core/src/core_agent_execution.rs`.

### Опциональные
Этап 197.2 (`git worktree`) и этап 197.3 (IPC/UI) — до их подключения Core
сохраняет прежнее поведение при пустом `SessionBinding`.

## Recovery и rollback
- При передаче неизвестного или ненастроенного `ProviderRouteKey` задача
  отклоняется сразу с типизированной ошибкой `provider_not_configured`, не
  создавая `agent_run` в `EventJournal`.
- Rollback — откат до чтения `default_route` и `SelectedModel`; схема SQLite на
  этом этапе не меняется.

## Verification
- Узкие unit-тесты `evohime-model-gateway` и `evohime-core` на мульти-маршрутный
  выбор, неизменяемость `(route, model)` внутри цикла агента и per-run
  `PermissionMode`.
- Проверка `cargo doc` gates (без `missing_docs` на новых публичных типах).
