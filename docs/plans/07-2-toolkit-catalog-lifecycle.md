# План 07-2 — Toolkit catalog, provenance и безопасный lifecycle

## Цель

Добавить локальный каталог toolkit-ов с понятными версиями, provenance,
allowlist и rollback semantics. Каталог должен помогать пользователю находить
инструменты, но не превращаться в механизм скрытой загрузки и выполнения
непроверенного кода.

## Зависимости

### Блокирующие

- [07-1](07-1-tool-manifest-contract.md);
- существующие sha256-проверки артефактов рядом с runtime
  (`crates/evohime-listener/src/tools_dir.rs`, `evohime.manifest.json` в
  `crates/evohime-updater`), SQLite migrations в `crates/evohime-local-storage`
  и permission/audit storage;
- authenticated Core↔Electron IPC.

### Опциональные

- remote signed catalog. До его появления каталог работает только с builtin
  entries и локально импортированными manifest metadata;
- Core-owned MCP registry entry из 06-1. До его появления external entries
  остаются `unavailable`, не появляются в executable loadout, а существующий
  `mcp.call` виден как обычный builtin-инструмент.

## Что уже есть в коде

- проверка sha256 файлов runtime по manifest уже реализована для listener
  (`tools_dir.rs`) и для пакета обновления (`evohime.manifest.json`);
- SQLite storage с миграциями и audit-таблицами существует
  (`crates/evohime-local-storage`), включая `capability_store.rs`;
- read-only IPC projection и rate limit уже применяются для других панелей.

Нет самой сущности catalog entry: нет таблицы версий toolkit-а, статусов,
license/source metadata, истории enable/disable и rollback. Хеш-утилиты сейчас
привязаны к конкретным потребителям и не обобщены до каталога.

## Изменения

1. Ввести Core-owned tables/records для catalog entry, manifest version,
   source, hash, license, status (`available`, `enabled`, `disabled`,
   `quarantined`, `unavailable`) и audit history.
2. Реализовать lifecycle:
   discover → validate metadata → verify hash → explicit enable → use in
   loadout → disable/quarantine → rollback to previous version.
3. Запретить автоматическое выполнение install hooks, archive paths, scripts,
   arbitrary commands и model-provided URLs. Путь пакета канонизируется и
   проверяется до чтения.
4. Сопоставить catalog entry с manifest hash и active run snapshot. Уже
   начатый run не должен внезапно перейти на другую версию инструмента.
5. Добавить bounded diagnostics для отсутствующего runtime, hash mismatch,
   incompatible Core/protocol и disabled capability.
6. Подготовить read-only IPC projection каталога и команды enable/disable,
   подчинённые audit, rate limit и approval policy.

## Проверки

- migration/rollback и restart recovery;
- hash mismatch, path escape, undeclared file и stale version tests;
- проверка, что disabled/quarantined entry не попадает в loadout;
- проверка snapshot stability во время активного run;
- отсутствие install-time code execution в smoke tests;
- совместимость с существующими правилами hash manifest для listener и
  updater: их проверки не ослабляются и продолжают проходить;
- `cargo fmt --check` и targeted `cargo test -p evohime-core -p evohime-local-storage`.

## Готово, когда

Ева может показать и безопасно включить версионированный toolkit, но ни один
catalog entry не получает capability только от факта установки или ответа
модели.
