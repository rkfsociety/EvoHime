# План 199.4 — Verification, release evidence и закрытие

Статус: active implementation contract. Этап следует после 199.3.

## Scope и изменяемые контракты
1. Полная проверка контракта и независимости модулей:
   - подтвердить отсутствие изменений в `crates/evohime-core/`, `crates/desktop-ipc/proto/` и `release-versions/core.txt`;
   - подтвердить прохождение CI gates для `evohime-hive` (unit/integration tests, clippy с `deny(unwrap_used, expect_used, panic)`, `cargo doc --workspace --lib --no-deps --locked` с `-D missing_docs`) и `evohime-electron` (`typecheck`, `test`, `check:protocol`, `build`, `check:bundle`).
2. Подготовка релизных версий публикуемых модулей (по правилам 14 и 16 `AGENTS.md`, только при изменении исходников соответствующих модулей):
   - повысить патч-версию в `release-versions/hive.txt`, `release-versions/shell-host.txt` и `release-versions/ui-bundle.txt` (и не трогать `release-versions/core.txt`).
3. Синхронизация канонической документации при закрытии направления:
   - перенести итоговый контракт `HiveCityMapSnapshot`, `GetCityMap` на control pipe Улья и клиентскую проекцию архипелага в [docs/architecture/hive.md](../architecture/hive.md) и [docs/hive-architecture.md](../hive-architecture.md);
   - обновить [docs/current-state.md](../current-state.md) и [docs/release-evidence.md](../release-evidence.md);
   - удалить временные файлы `docs/plans/199-*.md` и убрать `199` из активного каталога [docs/plans/README.md](./README.md), перенеся `199` в список закрытых направлений.

## Зависимости
### Блокирующие
- Завершение и верификация этапов 199.1, 199.2 и 199.3.

### Опциональные
- Нет.

## Recovery и rollback
- При сбое CI или регрессии на установленной версии откат выполняется штатным модульным rollback для `hive`, `shell-host` и `ui-bundle` без влияния на `core` и SQLite.

## Verification
- CI: Rust test + clippy + rustdoc gates; Electron typecheck + unit/integration tests + `check:bundle`.
- Проверка на реальном сценарии (после публикации модулей и обновления): открытие вкладки «Улей» → отображение Столицы Евы → добавление проекта в «Проекты под наблюдением» → появление и «строительство» города-спутника → клик по зданию и городу → удаление проекта → сворачивание города-спутника.

## Release evidence
- Ссылки на зелёные CI runs (Rust, rustdoc, Electron, module releases `hive` / `shell-host` / `ui-bundle`).
- Запись в `docs/release-evidence.md` с подтверждением независимости `evohime-core`.

## Критерии выхода
- [ ] Все проверки CI зелёные, `evohime-core` не затронут.
- [ ] Каноническая документация обновлена, временные файлы плана 199 удалены.
- [ ] Зафиксированы release evidence и версии модулей `hive`, `shell-host`, `ui-bundle`.
