# Подплан 0, этап 0 — зафиксированный стек Electron shell

Статус: принято; изменение любой pinned-версии требует отдельного review
Область: `desktop/evohime-electron` (Electron main, preload, renderer)

## Выбранные версии

Все версии закреплены точно (`save-exact=true`, без `^`), lockfile хранится в
репозитории и проверяется в CI через `npm ci`.

| Компонент | Версия | Почему |
| --- | --- | --- |
| Electron | `43.4.0` | текущий stable major; входит в окно поддержки последних major-версий |
| Node runtime | встроенный в Electron 43 (`>= 22.12`) | внешний Node.js в продукт не входит |
| TypeScript | `5.9.3` | стабильная линия со совместимостью с Vite 7 и electron-vite 5 |
| Renderer framework | React `19.2.8` + React DOM `19.2.8` | UI-срезы плана 1–5 описаны в компонентной модели |
| Bundler | Vite `7.3.6` + electron-vite `5.0.0` | один инструмент на три слоя; electron-vite 5 поддерживает Vite ≤ 7 |
| React-плагин | `@vitejs/plugin-react` `5.2.0` | совместим с Vite 7 |
| Тесты | Vitest `3.2.7` | совместим с Vite 7, запускает и unit, и real-Core E2E |
| Protobuf | `protobufjs` `8.7.2` + `protobufjs-cli` `2.6.2` | генерация TS-биндингов без внешнего `protoc` |
| Package manager | npm `10.x`, lockfile v3 | `npm ci --ignore-scripts` + явный allow-list postinstall |
| Packaging | electron-builder `26.15.3`, target `dir` | только распакованный payload; install/update/rollback остаются за Inno Setup и `evohime-transaction.exe` |

Не используются: Electron `autoUpdater`, Squirrel, любой второй update-канал,
HTTP-сервер, browser launcher, внешний Node runtime.

## Профили сборки

- **dev** (`npm run dev`): dev-server renderer, DevTools и hot reload доступны,
  sourcemaps для main/preload включены;
- **production** (`npm run build` + `npm run package`): DevTools выключены,
  меню снято, DevTools-шорткаты перехвачены, sourcemaps не собираются,
  debug-флаги (`--remote-debugging-port`, `--inspect`, `--no-sandbox`, …)
  приводят к отказу запуска, а не к тихому игнорированию.

## Границы доверия

```text
renderer   недоверенный, sandbox: true, contextIsolation: true, nodeIntegration: false
preload    узкий bridge: window.evohime.v1 (invoke/subscribe/clipboard/openExternal)
main       transport/orchestration: named pipe, окно, tray, диагностика
Core       единственный security authority: capabilities, policy, approvals, paths
```

Renderer не получает `ipcRenderer`, EventEmitter, MessagePort, `fs`, `shell`,
`child_process`, environment или прямой доступ к pipe/workspace.

## Supply chain

- `.npmrc` отключает lifecycle-скрипты для всех зависимостей;
- `scripts/postinstall-allowlist.mjs` запускает только `electron` и `esbuild`
  installers, добавление записи требует security review;
- devDependencies не попадают в package (`files: out/**`, `package.json`);
- lockfile обязателен, CI использует `npm ci`.

## Генерация протокола

Канонический источник — `crates/desktop-ipc/proto/evohime.desktop.proto`.
`npm run generate:protocol` создаёт `src/main/ipc/generated/protocol.{js,d.ts}`,
`npm run check:protocol` в CI падает на устаревшем биндинге. Ручные типы
протокола не используются: bootstrap-этап с ручными типами не понадобился.

## Результаты Gate 0 spike (проверено на Windows 11 x64, 2026-08-13)

| Проверка | Результат |
| --- | --- |
| Сборка подписываемого package | `electron-builder --dir` → `release/win-unpacked/EvoHime.exe`, подписан signtool-шагом, ~359 МБ распакованно |
| Запуск без консоли и браузера | packaged `EvoHime.exe` стартует, окно открывается, консольного окна и внешнего браузера нет |
| sandbox + preload API | `sandbox: true` совместим с фактическим preload API; ослаблять sandbox не потребовалось |
| CSP | `default-src 'self'`, без `unsafe-eval`; для packaged renderer действует meta-CSP (file:// не отдаёт заголовки), для остальных ответов — заголовок сессии |
| Permission handlers | deny-by-default подтверждён логом: `media`, `geolocation`, `web-app-installation` отклонены |
| Named pipe: handshake | `core.ready`, negotiated protocol `1.0`, `coreVersion` получены от настоящего `evohime-core.exe` |
| Named pipe: reconnect | kill Core → `reconnecting` → рестарт Core → `connected`; bounded exponential backoff 250 мс → 10 с |
| Named pipe: bounded frames | announced length > 4 МиБ отвергается до буферизации, соединение пересоздаётся |
| Named pipe: replay/resync | пропуск sequence переводит в `state-gap` и отправляет `ResyncRequest`, а не считается успешным восстановлением |
| Backpressure/queue | bounded очередь команд (256 / 8 МиБ) отвечает `queue-full` вместо тихой потери |
| Redaction | пути, токены, argv и стек-трейсы скрыты в `shell-main.jsonl` |
| packaged shell + real Core | packaged `EvoHime.exe` подключается к запущенному Core без единого reconnect-предупреждения |

## Результаты Gate 1 (проверено на Windows 11 x64, 2026-08-13)

| Проверка | Результат |
| --- | --- |
| Generated types = canonical proto | `npm run check:protocol` проходит и падает на устаревшем биндинге |
| Owner-only DACL на pipe | Core создаёт endpoint с `D:P(A;;GA;;;<user SID>)`, лог `ipc.listening acl=owner-only` |
| Непредсказуемое имя pipe | supervisor генерирует `evohime-core-<16 байт hex>` на сессию |
| Одноразовый nonce с TTL | повтор, просрочка и подмена nonce отвергаются (unit-тесты `session.rs`) |
| HMAC-proof | один known-answer вектор проверяется в Rust, Electron и WinUI tests |
| Session/identity binding | user SID клиента читается у ОС через impersonation; чужая идентичность отвергается |
| Enforced handshake E2E | настоящий Core: корректный secret подключается, подделанный secret и неизвестная роль дают `fatal` без retry-цикла |
| Полная цепочка | supervisor → protected context → Core (`authenticated: true`) → Electron: `ipc.client_authenticated role=shell` без reconnect-предупреждений |
| WinUI fallback | компилируется, C# suite (24 теста) зелёная, роль `compatibility-shell` принимается настоящим Core |
| Запуск подписанного package | supervisor → Core → packaged `EvoHime.exe`: окно «EvoHime» открыто, `packaged: true`, `developerLaunch: false`, `ipc.client_authenticated role=shell`, ошибок и reconnect-предупреждений нет |

## Область проверки

Проверка выполняется на текущей машине разработки (Windows 11 x64) —
собранный package запускается и проходит полную цепочку supervisor → Core →
оболочка. Отдельный прогон на Windows 10 в рамках этих gate не выполняется;
поддержка Windows 10 остаётся требованием продукта и проверяется на
release acceptance, а не на каждом этапе.

Также не проверялись на этом этапе: UAC/elevation, смена пользовательской
сессии, memory pressure, DPI/scaling, dark theme и ручной запуск
WinUI-оболочки под supervisor (покрыт тестами и общим вектором proof).

## Решение по риску bridge

Named-pipe adapter достиг reliability-критериев spike, поэтому отдельный Rust
IPC bridge не вводится. Решение пересматривается, если ACL/challenge-работа
этапа 1 не проходит свой gate.
