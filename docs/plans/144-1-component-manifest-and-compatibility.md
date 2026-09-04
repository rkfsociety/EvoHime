# План 144.1 — Модульные релизы: манифест компонентов и совместимость

Статус: этап 1 для [плана 144.0](./144-0-modular-release-and-component-update.md).

## Зависимости

### Блокирующие

- План 144.0 и текущие package/update contracts из `architecture.md`,
  `current-state.md` и `release-evidence.md`.
- `scripts/native-package.ps1`, `scripts/build-windows-native.ps1`,
  `desktop/evohime-electron/src/main/update/release-installer.ts` и
  `crates/evohime-updater` после evidence freeze.
- IPC major/sequence rules, Supervisor launch contract и listener-runtime
  manifest/hash contract.

### Опциональные

- Независимые worker/data packs; при их отсутствии manifest v1 описывает только
  безопасный initial component set и оставляет остальные в full-installer scope.
- Будущая подпись бинарников; текущая проверка использует существующий release
  trust/hash boundary и не расширяет code-signing scope.

## Реализация

1. Зафиксировать inventory установки и ownership для `shell-host`, `ui-bundle`,
   `core`, `supervisor`, workers, `transaction`, `cli`, `verifier` и runtime/data
   packs. Отдельно устранить ложное представление, что весь `EvoHime.exe`
   одновременно является независимым UI и browser backend.
2. Определить bounded `EvoHime Component Manifest v1` с canonical JSON:
   product/release identity, OS/architecture, release commit, component ID,
   component version/build identity, artifact name/kind/path, byte size,
   SHA-256, dependency IDs, required/optional flag, protocol/ABI range,
   restart class, migration class и rollback policy. Секреты, absolute paths,
   произвольные URLs и executable commands в manifest не допускаются.
3. Определить отдельные product и component identities: один пользовательский
   release manifest может менять только `ui-bundle`, сохраняя версии/hash
   остальных компонентов. Commit-based development marker сохраняется для
   source rebuild compatibility.
4. Ввести правила selection/compatibility: текущий host должен поддерживать
   component schema; required unknown, missing dependency, downgrade,
   protocol/ABI mismatch, duplicate ID, cycle, size/hash/path violation и
   manifest drift дают typed отказ до staging. Неизвестный optional component
   не устанавливается молча.
5. Определить локальный installed inventory и план операции в `update-state`,
   не добавляя updater authority в Core SQLite. Existing
   `evohime.manifest.json` и full installer остаются совместимым bootstrap
   fallback до завершения миграции.
6. Зафиксировать compatibility matrix для минимальных независимых сценариев:
   `ui-bundle` требует совместимый `shell-host` и additive supported IPC;
   `core` требует совместимый Supervisor launch/runtime contract;
   schema-changing Core release требует backup-aware coordinated transaction;
   `supervisor` и `transaction` обновляются только специальным restart path.

## Критерии выхода

- [ ] Inventory отражает фактические файлы native package и ресурсы, включая
  listener/analysis/transaction/verifier, а не только три главных процесса.
- [ ] Manifest parser/validator bounded, canonical и не принимает path escape,
  symlink/reparse payload, hash mismatch, oversized artifact или unsafe graph.
- [ ] Compatibility matrix различает UI-only, component-set и full-installer
  операции; неизвестные данные не превращаются в success.
- [ ] Product release ID и component versions не требуют отдельных релизов для
  пользователя и не смешиваются с Core Declarative Component Registry.
- [ ] Есть focused contract tests и redacted fixtures для valid/invalid,
  stale/downgrade, dependency cycle, unknown required и protocol mismatch.

## Не входит

Не входят скачивание и применение артефактов, изменение Electron loader,
transaction worker implementation, CI matrix и пользовательская панель; они
выполняются следующими этапами.
