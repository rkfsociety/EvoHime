# План 202.0 — Микроядерная декомпозиция Core и независимые модули-сервисы

Статус: active implementation contract. Направление переводит `evohime-core` (315 файлов, ~8,25 МБ исходников) из монолитного runtime в тонкое микроядро с изолированными модулями-сервисами и внешним пакетом данных.

## Цель
Свести к минимуму время сборки в CI, время исправления ошибок и время доставки обновлений пользователю:
1. Любая правка в прикладной подсистеме пересобирает, тестирует и обновляет **только свой модуль**, не затрагивая `evohime-core.exe` и остальные сервисы.
2. Правки промптов, рецептов и декларативных шаблонов доставляются отдельным пакетом данных (`core-assets`) **без компиляции Rust**.
3. Обновление прикладного модуля выполняется точечно: `supervisor` / `updater` перезапускают только обновлённый сервис без разрыва UI и активных сессий ядра.

## Границы микроядра (что остаётся в `evohime-core`)
В `evohime-core.exe` остаётся только то, что обязано быть единым владельцем состояния и безопасности:
- главный цикл агента, управление сессиями и оркестрация (`core_agent*`, `core_coordinator*`, `core_lifecycle`);
- транспорт и контракт с оболочкой/CLI (`desktop-ipc`, `pipe_server`, `unix_pipe_server`, `core_protocol`, `ipc_bridge`);
- хранилище SQLite и журнал событий (`evohime-local-storage`, `core_journal*`, `evohime-receipts`);
- политики безопасности и разрешения (`permissions`, `policy_gate`, `sensitive_data_guardrails`, `output_guardrail_pipeline`);
- базовый маршрутизатор вызовов к моделям и инструментам (`model-gateway`, `tool-runtime`, `context-budget`);
- универсальный реестр и прокси внешних модулей-сервисов (`ModuleRegistry` + `ModuleProxy`).

## Единая архитектура модулей-сервисов
Все вынесенные подсистемы работают по единому стандарту:
1. **Единый SDK/контракт (`crates/evohime-service-contract`):**
   - единая аутентификация (HMAC-SHA256 по named pipe / Unix socket, аналогично `core` и `hive`);
   - стандартный lifecycle: `health`, `capabilities` (декларация инструментов/методов с JSON-схемами), `invoke`, `stream` и graceful shutdown;
   - ядро не содержит бизнес-логики модулей — оно лишь регистрирует их capabilities в `ToolRegistry`/`CapabilityRegistry` и проксирует вызовы с проверкой `permissions`.
2. **Единый реестр модулей для сборки и доставки:**
   - единый декларативный источник списка модулей (вместо ручного дублирования в `module-router.yml`, `compatible-manifest.yml`, `module-release.ps1`, `native-package.ps1` и `evohime-updater`).
3. **Точечный hot-restart:**
   - для прикладных модулей (`restart = "service"` вместо `"core"`/`"shell"`) `evohime-supervisor` умеет мягко остановить, заменить бинарник и перезапустить только один сервис.

## Целевой состав модулей

### А. Доперенос в существующие модули
- `analysis-worker` — статический анализ, `analysis_kernel*`, диагностика кода, снимки архитектуры (~105 КБ).
- `hive` — `hive_delegation`, `hive_context` и часть командной координации (~125 КБ).
- `verifier` — `plan_review`, `multi_reviewer_ensemble`, `verification_evidence_ledger` и review-lanes (~90 КБ).

### Б. Новые независимые модули-сервисы и пакет данных
- `core-assets` (`.zip` пакет данных, без Rust-сборки) — системные промпты, `capability_recipes`, `domain_workflow_recipes`, `approval_policy_profiles`, шаблоны.
- `evals` (на базе `crates/evohime-eval`, ~270 КБ) — бенчмарки, симуляции, калибровка моделей и железа.
- `media-voice` (~135 КБ) — `image_generation`, `vision_contract`, голосовые команды и голосовой вывод.
- `research` (~145 КБ) — веб-поиск, fetch, research pipeline, браузерные сессии и публичный GitHub.
- `remote-a2a` (на базе `crates/evohime-remote`, ~140 КБ) — внешние агенты (A2A), внешние coding-агенты и удалённые каналы.
- `ambient` (~115 КБ) — фоновая проактивность, временные сигналы и автономные эксперименты.
- `computer-use` (~15 КБ сейчас, высокий рост) — GUI-автоматизация, мобильная автоматизация, IDE-мост (убирает Win32-GUI зависимости из ядра).
- `workflow-engine` (>500 КБ) — движок сценариев, расписания, триггеры, фоновые задания.
- `rag-memory` (>450 КБ) — `workspace_rag*`, `memory_extraction*`, `offline_experience_consolidation`, `experience_replay_library`.
- `model-router` (>300 КБ, финальный этап) — локальный раннер/адаптация моделей и маршрутизация бесплатных провайдеров.

## Зависимости
### Блокирующие
- `crates/desktop-ipc`, `crates/evohime-supervisor`, `crates/evohime-updater`, `.github/workflows/module-router.yml`, `scripts/module-release.ps1`, `scripts/native-package.ps1`.
### Опциональные
- План 195 (декларативные capability descriptors) — используется для единого описания capabilities вынесенных модулей.
- План 194 (Background Jobs Control Center) — отображает статус фоновых задач модулей `workflow-engine` и `ambient`.

## Этапы
- [Этап 1 — единый контракт модулей, реестр сборки и `core-assets`](./202-1-core-microkernel-decomposition.md)
- [Этап 2 — доперенос в существующие модули и вынос первой волны (`evals`, `media-voice`, `research`)](./202-2-core-microkernel-decomposition.md)
- [Этап 3 — вынос `remote-a2a`, `ambient`, `computer-use`, `workflow-engine`, `rag-memory` и hot-restart в `supervisor`](./202-3-core-microkernel-decomposition.md)
- [Этап 4 — вынос `model-router` (при целесообразности), полная верификация и закрытие](./202-4-core-microkernel-decomposition.md)

## Критерии готовности
- [ ] Создан единый контракт `evohime-service-contract` и `ModuleRegistry` в `evohime-core`.
- [ ] Список модулей для CI, релизов и апдейтера генерируется/валидируется из единого источника.
- [ ] `core-assets` обновляется без компиляции Rust.
- [ ] Прикладные модули собираются и публикуются изолированными workflow без пересборки `core`.
- [ ] `supervisor` и `updater` поддерживают точечный перезапуск сервиса без перезапуска `EvoHime.exe` и `evohime-core.exe`.
