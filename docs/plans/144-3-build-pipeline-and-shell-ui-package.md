# План 144.3 — Модульные релизы: build pipeline, Electron host и UI bundle

Статус: этап 3 для [плана 144.0](./144-0-modular-release-and-component-update.md).

## Зависимости

### Блокирующие

- Manifest/compatibility contract этапа
  [144.1](./144-1-component-manifest-and-compatibility.md) и apply/recovery
  contract этапа [144.2](./144-2-selective-transaction-and-recovery.md).
- Текущие `electron-builder`, native package scripts, Inno Setup, GitHub
  Actions и Electron security boundary (`contextIsolation`, sandbox,
  `nodeIntegration=false`).
- Канонический desktop IPC protocol и typed preload/main adapter.

### Опциональные

- Независимая сборка workers/data packs; initial implementation может оставить
  их в полном native package и всё равно выпустить UI-only path.
- Дельта-архивы и content-addressed dedup; initial artifacts могут быть
  полными bounded component archives.

## Реализация

1. Разделить build outputs по component IDs: renderer build выдаёт отдельный
   `ui-bundle`, Electron main/preload и обязательные native resources —
   `shell-host`, Rust targets — соответствующие native components. Не делать
   каждый crate отдельным пользовательским компонентом.
2. Изменить package manifest/generator так, чтобы `ui` больше не маскировал
   весь `EvoHime.exe`: manifest должен различать shell host, UI bundle и
   browser backend только если у backend действительно есть отдельный
   deployable contract.
3. Добавить selective build graph и cache policy: по изменённым путям
   определять affected components, переиспользовать неизменившиеся artifacts,
   не запускать полный Electron/Rust package для UI-only change, но переводить
   изменения protocol, updater, installer, supervisor contract или shared
   resources в coordinated/full gate.
4. Обновить release workflow с одним существующим release channel: публиковать
   component artifacts и единый manifest, сохраняя full `EvoHime-Setup.exe`
   как bootstrap/fallback. Manifest должен содержать exact asset names,
   sizes/hashes и selected compatibility ranges; публикация идёт только после
   соответствующих green checks.
5. Расширить `evohime.build.json`/installed marker до bounded inventory
   component versions/hashes, не сохраняя secrets или абсолютные пути. Старый
   marker должен приводить к безопасному full-package detection path.
6. Сохранить Electron ownership: main process проверяет manifest, запускает
   update service и выбирает validated UI version; renderer получает только
   projection. Расширить typed local update status выбранными components,
   bytes/progress, compatibility reason и restart requirement без переноса
   update authority в renderer или Core.
7. Добавить UI host fallback: отсутствующий/повреждённый active UI bundle не
   превращается в blank screen — загружается последняя healthy version или
   включается понятный recovery/full-installer state.

## Критерии выхода

- [ ] Изменение только `desktop/.../renderer` создаёт UI artifact и не требует
  пересборки native binaries для selective path.
- [ ] Shell host валидирует active UI bundle до загрузки и сохраняет Electron
  sandbox/context isolation/preload boundary.
- [ ] CI различает UI-only, native component, protocol/contract и full release
  changes; обязательные release gates остаются на месте.
- [ ] Один release manifest может публиковать UI-only patch, component-set или
  полный installer без второго канала и без отдельных пользовательских
  релизов каждого модуля.
- [ ] Update surface правдиво показывает выбранный набор, download size,
  restart и fallback/error; business logic не появляется в renderer.

## Не входит

Не входят новый дизайн интерфейса, изменение Core features, Electron
autoUpdater/Squirrel, публичный HTTP server, внешний Node/Python runtime или
добавление arbitrary dynamic renderer code.
