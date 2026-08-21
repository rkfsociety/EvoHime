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
- дополнительные package metadata для MCP. Уже существующий
  `WorkflowRegistry` остаётся единственным источником server identity,
  endpoint, transport и tool allowlist; до появления catalog metadata внешняя
  запись видна как `unavailable` и не попадает в executable loadout.

## Что уже есть в коде

- проверка sha256 файлов runtime по manifest уже реализована для listener
  (`tools_dir.rs`) и для пакета обновления (`evohime.manifest.json`);
- `crates/evohime-core/src/capability_registry.rs` уже валидирует подписанные
  manifests ролей/skills: trusted signing key, content hash, allowed
  tools/domains, protected paths, risk и запрет install scripts;
- `crates/evohime-local-storage/src/capability_store.rs` и таблица
  `capability_manifests` уже дают bounded SQLite catalog с install/update,
  list и delete IPC. Он хранит только текущую запись по id и не является
  toolkit lifecycle: в нём нет версионной истории, enabled/disabled/
  quarantined статусов или rollback audit;
- SQLite storage с миграциями и audit-таблицами существует
  (`crates/evohime-local-storage`), включая `capability_store.rs`;
- read-only IPC projection и rate limit уже применяются для других панелей.

Нет toolkit-level catalog entry поверх этого capability catalog: отсутствуют
таблица версий toolkit-а, license/source/package metadata, статусы,
enable/disable history и rollback audit. Хеш-утилиты сейчас привязаны к
конкретным потребителям и не обобщены до toolkit package; существующие
`capability_manifests` нельзя бездумно использовать как историю, потому что
upsert заменяет текущую строку.

## Изменения

1. Расширить существующий Core-owned capability/catalog trust boundary, не
   создавая второго источника подписей: добавить toolkit entry, manifest
   version, source, package hash, license, status (`available`, `enabled`,
   `disabled`, `quarantined`, `unavailable`) и audit history. Связь с
   `CapabilityManifest` и его `allowed_tools`/domains должна быть явной;
   обновление не должно превращать role/skill manifest в executable tool
   permission само по себе.
2. Реализовать lifecycle:
   discover → validate metadata → verify hash → explicit enable → use in
   loadout → disable/quarantine → rollback to previous version.
3. Запретить автоматическое выполнение install hooks, archive paths, scripts,
   arbitrary commands и model-provided URLs. Путь пакета канонизируется и
   проверяется до чтения; существующий `CapabilityManifest` с
   `allow_install_scripts` остаётся fail-closed.
4. Сопоставить catalog entry с manifest hash и active run snapshot. Уже
   начатый run не должен внезапно перейти на другую версию инструмента.
5. Добавить bounded diagnostics для отсутствующего runtime, hash mismatch,
   incompatible Core/protocol и disabled capability.
6. Подготовить read-only IPC projection каталога и команды enable/disable,
   подчинённые audit, rate limit и approval policy.
7. Для MCP ссылаться на существующий `WorkflowRegistry` по `server_id`, а не
   дублировать endpoint/transport/allowlist в toolkit catalog.

## Проверки

- migration/rollback и restart recovery;
- hash mismatch, path escape, undeclared file и stale version tests;
- совместимость с `capability_registry`/`capability_store`: trusted manifest
  остаётся валидным, а toolkit lifecycle не выдаёт ему новых grants;
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
