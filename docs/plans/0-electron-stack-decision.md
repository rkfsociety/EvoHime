# Подплан 0, этап 0 — зафиксированный стек Electron shell

Статус: принято. Pinned-версии изменяются только отдельным review.

Этот документ хранит только решение по стеку и границам безопасности. Завершённые
результаты Gate 0–2 и исторические таблицы проверок удалены; текущая работа
ведётся в `docs/plans/0-electron-shell-migration.md`.

## Стек

| Компонент | Версия / решение |
| --- | --- |
| Electron | `43.4.0` |
| Node runtime | встроенный в Electron, внешний Node в продукт не входит |
| TypeScript | `5.9.3` |
| Renderer | React `19.2.8` + React DOM `19.2.8` |
| Bundler | Vite `7.3.6` + electron-vite `5.0.0` |
| Tests | Vitest `3.2.7` |
| Protobuf | protobufjs `8.7.2` + protobufjs-cli `2.6.2` |
| Package manager | npm 10.x, lockfile v3, `npm ci --ignore-scripts` |
| Packaging | electron-builder `26.15.3`, `dir` payload; install/update остаются Inno Setup и transaction worker |

Не используются Electron autoUpdater, Squirrel, HTTP-сервер, browser launcher
и второй update-канал.

## Профили и границы

- Dev допускает DevTools, hot reload и sourcemaps.
- Production отключает DevTools/menu/shortcuts и sourcemaps, отвергает debug-флаги.
- Renderer: `sandbox: true`, `contextIsolation: true`, `nodeIntegration: false`.
- Preload экспортирует только `window.evohime.v1` с typed `invoke`, `subscribe`,
  bounded clipboard write и allow-listed `openExternal`.
- Renderer не получает `ipcRenderer`, EventEmitter, MessagePort, `fs`, `shell`,
  `child_process`, environment или прямой доступ к pipe/workspace.
- Core остаётся единственным владельцем policy, capabilities, approvals и
  workspace validation.

## Supply chain и протокол

`.npmrc` отключает lifecycle-скрипты; `scripts/postinstall-allowlist.mjs`
разрешает только необходимые installers Electron/esbuild. Канонический IPC
источник — `crates/desktop-ipc/proto/evohime.desktop.proto`; generated bindings
проверяются `npm run check:protocol`.

Проверки выполняются на текущей Windows-машине. Другие ОС не являются частью
текущего цикла и проверяются отдельными задачами при работе с ними.

## Решение по bridge

Named-pipe adapter принят как transport layer main process; отдельный Rust IPC
bridge не добавляется. Core повторно проверяет каждую команду, а UI использует
только типизированный preload API.
