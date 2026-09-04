# План 144.4 — Модульные релизы: verification, release evidence и закрытие

Статус: этап 4 для [плана 144.0](./144-0-modular-release-and-component-update.md).

## Зависимости

### Блокирующие

- Реализованные contract, transaction/recovery и build/UI surfaces этапов
  [144.1](./144-1-component-manifest-and-compatibility.md),
  [144.2](./144-2-selective-transaction-and-recovery.md) и
  [144.3](./144-3-build-pipeline-and-shell-ui-package.md).
- Existing Rust, Electron, native-package, installer rollback and Windows
  acceptance gates из текущего release workflow.
- Redacted evidence policy: tests не публикуют credentials, raw prompts,
  transcripts, absolute paths или пользовательские данные.

### Опциональные

- Реальный worker/data-pack rollout; без него evidence должен явно показывать
  unsupported/deferred status, а не считать все компоненты independently
  updateable.
- Authenticode и production CDN; они не нужны для закрытия текущего scope.

## Реализация

1. Добавить focused tests для manifest canonicalization, bounds, hashes, missing
   artifacts, unknown required components, dependency cycles, downgrade,
   protocol/ABI mismatch, path traversal, symlink/reparse, archive limits и
   old-marker compatibility.
2. Добавить updater tests для component-set planning, selective backup,
   untouched-file preservation, atomic pointer/file replacement, lock retry,
   partial apply, power-loss journal recovery, health timeout, failed launch,
   rollback failure reporting и full-installer fallback.
3. Добавить Electron tests для renderer externalization, active-version
   selection, host fallback, preload/security invariants, truthful update
   status и UI-only download accounting. Проверить, что новый UI не вызывает
   неподдерживаемый IPC command без compatibility gate.
4. Добавить isolated Windows E2E в temporary install directories:
   full-package baseline → UI-only update; UI+Core coordinated update;
   unsupported manifest → full installer; interruption before/after apply;
   health timeout; restart/recovery; preservation of local state and extra
   files. Установленный пользовательский клиент не запускается и не меняется.
5. Добавить CI/release evidence matrix: path-to-component selection,
   reproducible artifact hashes, manifest-to-assets check, native package smoke,
   installer smoke, rollback/health acceptance, full existing Rust/Electron
   regression for contract changes and `git diff --check`.
6. После фактического green evidence перенести подтверждённый contract и
   state в `docs/architecture.md`, `docs/current-state.md`,
   `docs/release-evidence.md` и при необходимости `docs/development-plan.md`.
   Только после этого удалить полный комплект `144-0` … `144-4` по правилам
   каталога планов.

## Критерии выхода

- [ ] UI-only update доказан по byte/artifact inventory: скачан и заменён
  только UI bundle, а остальные компоненты не изменились.
- [ ] Component-set и full-installer paths имеют воспроизводимые green/failed
  evidence, включая rollback и recovery после каждой опасной границы.
- [ ] CI не утверждает выборочную сборку, если тест фактически собрал полный
  пакет; output и manifest показывают реальный набор компонентов.
- [ ] Документы описывают только подтверждённое checkout-состояние, а deferred
  workers/data/signing явно остаются deferred.
- [ ] Полный release audit проходит, рабочее дерево не содержит посторонних
  изменений, `git diff --check` проходит, а task-only изменения зафиксированы
  отдельным коммитом.

## Не входит

Не входят публикация нового пользовательского релиза в рамках планирования,
изменение установленной Евы, ручная подпись артефактов и закрытие плана без
фактически собранного и проверенного selective update path.
