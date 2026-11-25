# План 201.4: Удаление зависимости в `Cargo.toml`, guard-тест в CI и закрытие

> **Зависимости:** [201-3-core-hive-module-decoupling.md](201-3-core-hive-module-decoupling.md) (blocking). Optional: нет.

---

## 1. Цель этапа

Удалить последнюю формальную связь `evohime-core -> evohime-hive` в `Cargo.toml`, настроить триггеры CI и добавить автоматическую проверку, которая не позволит случайно вернуть зависимость в будущем.

## 2. Изменения

1. **Удаление зависимости и реэкспортов:**
   - Удалить строку `evohime-hive = { path = "../evohime-hive" }` из `crates/evohime-core/Cargo.toml`.
   - Удалить все 7 реэкспортов `evohime_hive::*` (`eva_pulse`, `eva_pulse_boundary`, `hive_observation`, `hive_settings`, `hive_state`, `hive_permission_session`, `hive_worktree`) из `crates/evohime-core/src/lib.rs`.
2. **Обновление CI workflows:**
   - В `.github/workflows/core.yml` добавить `crates/evohime-hive-contract/**` в `pull_request.paths` (так как `core` зависит от контракта).
   - Проверить `.github/workflows/hive.yml`, `.github/workflows/core.yml` и `.github/workflows/module-router.yml`, чтобы изменения внутри `crates/evohime-hive/**` никогда не требовали сборки `core module`.
3. **Автоматический guard-тест границ:**
   - В `scripts/module-router.tests.ps1` (или отдельный лёгкий check-скрипт без компиляции) добавить проверку: `crates/evohime-core/Cargo.toml` не должен содержать зависимость `evohime-hive`.
4. **Документация и закрытие плана:**
   - Зафиксировать инвариант в `AGENTS.md` и `docs/architecture.md` (или профильном файле `docs/architecture/`): `evohime-core` не имеет права зависеть от `evohime-hive`; изменения Улья никогда не должны требовать правок в `crates/evohime-core/**` и поднятия `release-versions/core.txt`.
   - Зафиксировать результаты проверок в `docs/release-evidence.md` и `docs/current-state.md`, затем закрыть план `201`.

## 3. Verification, Rollback и Release evidence

- **Verification:**
  - `cargo tree -p evohime-core` (в CI) не содержит `evohime-hive`.
  - Полный прогон CI: `core module`, `hive module`, `ui-bundle module`, `rustdoc`, `module-router`.
  - Тестовый сценарий: правка файла в `crates/evohime-hive/**` + `release-versions/hive.txt` запускает в `module-router` только `hive module`.
- **Rollback:** не требуется после прохождения CI; при сбое релизной упаковки до публикации — откат коммита этапа 201.4.
- **Release evidence:** ссылки на зелёные прогоны `core.yml`, `hive.yml`, `ui-bundle.yml`, `rustdoc.yml` и `module-router.yml` в `docs/release-evidence.md`.
