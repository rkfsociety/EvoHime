# План 202.4 — Финализация микроядра, `model-router` и release evidence

## Цель этапа
Завершить очистку `evohime-core` до тонкого микроядра, оценить вынос `model-router`, провести полную верификацию и закрыть направление.

## Содержание работ
1. **Оценка и вынос `model-router` (при подтверждении профита):**
   - `local_model_adaptation.rs`, `local_model_runtime_manager.rs`, `local_model_compatibility_gateway.rs`, `local_inference_scheduler.rs`, `free_access_probe.rs`, `free_provider_reliability_routing*.rs`.
   - Если замер латентности IPC на каждом шаге диалога укладывается в бюджет (до нескольких миллисекунд), вынести в `evohime-model-router.exe`; иначе — выделить локальный раннер/пробы в отдельный сервис, оставив быструю таблицу маршрутизации в `model-gateway`.
2. **Очистка `evohime-core`:**
   - Удаление неиспользуемых зависимостей из `crates/evohime-core/Cargo.toml` (в т.ч. тяжёлых Win32-GUI и лишних фич).
   - Проверка `cargo doc` gates, `clippy`, `rustfmt` по всему workspace.
3. **Сравнительная проверка времени CI и обновлений:**
   - Фиксация времени сборки/тестов изолированных модулей vs `core.yml`.
   - Проверка совместимости `compatible-manifest.yml` и полного bootstrap-сценария.
4. **Закрытие плана:**
   - Обновление `docs/architecture.md`, `docs/architecture/*.md`, `docs/current-state.md`, `docs/release-evidence.md` и удаление временных файлов `202-*` после полного завершения.

## Зависимости
- Блокирующие: Этапы 202.1, 202.2, 202.3.

## Критерии готовности
- [ ] Все запланированные прикладные модули работают изолированно через `evohime-service-contract`.
- [ ] `evohime-core` содержит только микроядро и не пересобирается при правках прикладных модулей.
- [ ] Полный прогон CI (`windows.yml`, `core.yml`, все `<module>.yml`, `compatible-manifest.yml`) зелёный.
- [ ] Каноническая документация обновлена, release evidence зафиксировано.
