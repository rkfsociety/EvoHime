# План 203.1 — Волна 1: Неиспользуемые контракты, `computer-use`, `ambient`, `media-voice` и `research`

## Цель этапа
Убрать из `crates/evohime-core/src` первые 4 прикладные области с минимальной связностью с главным циклом агента и перевести их вызовы на внешние сервисы.

## Состав переноса и удаления из `evohime-core`
1. **Неиспользуемые модули-заготовки (немедленное удаление после переноса в сервисы):**
   - В `evohime-computer-use`: `native_computer_use_runtime.rs`, `mobile_device_automation_runtime.rs`, `ide_companion_bridge.rs`, `cross_modal_ui_grounding.rs`.
   - В `evohime-ambient`: `autonomous_metric_experiment_runtime.rs`, `temporal_signal_intelligence.rs`.
   - Действие: перенести логику/валидацию и тесты в соответствующие сервисы, удалить файлы и `pub mod` из `crates/evohime-core/src/lib.rs`.
2. **`ambient` → `crates/evohime-ambient` (`evohime-ambient.exe`):**
   - Перенос `ambient.rs` (74.5 KB) и `ambient_proactivity.rs`.
   - Переключение вызовов в `core_agent_memory.rs` и `ipc_bridge_ambient_*` на `ModuleRegistry` (`ambient.*` capabilities), перенос тестов из `ipc_bridge_tests_ambient.rs`.
3. **`media-voice` → `crates/evohime-media-voice` (`evohime-media-voice.exe`):**
   - Перенос `image_generation.rs` (73.8 KB), `vision_contract.rs`, `voice_command.rs`, `voice_input_dictation.rs`, `optional_voice_output_adapter.rs`.
   - Переключение tool/IPC вызовов генерации изображений и голосового ввода/вывода в `evohime-core` на `ModuleRegistry`.
4. **`research` → `crates/evohime-research` (`evohime-research.exe`):**
   - Перенос `research.rs`, `research_search.rs`, `research_fetch.rs`, `research_gate.rs`, `research_pipeline.rs`.
   - Переключение вызовов веб-поиска/fetch/pipeline на `ModuleRegistry` и перенос профильных тестов из `ipc_bridge_tests_workspace_research.rs`.

## Проверка этапа
- `cargo test --locked -p evohime-computer-use -p evohime-ambient -p evohime-media-voice -p evohime-research` (в CI) и узкие тесты `evohime-core`.
- Контрактные тесты в `evohime-core` подтверждают делегирование в `ModuleRegistry` и корректный `Unavailable` fallback при отсутствии бинарника сервиса.

## Статус проверки (2026-11-28, чтение кода + узкие локальные тесты)
- `computer-use` — ✅ полностью: все 4 файла перенесены в `evohime-computer-use`, из ядра удалены; тесты зелёные.
- `research` — ✅ полностью: 5 файлов перенесены (контракты — в `research_types.rs` и `external_source_acquisition_runtime.rs`), из ядра удалены; `research.fetch_url`, `research.capture_evidence`, `research.grounded_session`, `research.validate_grounded` идут через `invoke_required_service`, персист в SQLite — через persistence-порт ядра; контрактные тесты с mock-сервисом остались в `ipc_bridge_tests_workspace_research.rs`; тесты зелёные.
- `media-voice` — ⚠️ частично: `vision_contract`, `voice_command`, `voice_input_dictation`, `optional_voice_output_adapter` перенесены; `voice.command_parse`, `voice.dictation_*`, `media.validate_image` идут через `ModuleRegistry`; **`image_generation.rs` (76 КБ) остаётся в ядре** как оркестрация над SQLite (job-слоты, durable-восстановление) — вынос требует persistence-порта; локальная валидация `validate_output` осталась только как `#[cfg(test)]`-хелпер.
- `ambient` — ⚠️ частично: `ambient_proactivity.rs`, `autonomous_metric_experiment_runtime.rs`, `temporal_signal_intelligence.rs` перенесены; `ambient.proactivity_decide` и `voice.command_parse` идут через `invoke_if_ready`; контрактные тесты с `MockAmbientServiceTransport` в ядре; **`ambient.rs` (85 КБ, ~1900 строк) остаётся в ядре** — политика/контроль/реестры слушания живут в данных ядра и SQLite.
- Узкие локальные тесты: computer-use 5/5, ambient 16/16, media-voice 17/17, research 22/22 — зелёные. Полный CI-прогон подтверждает push.
