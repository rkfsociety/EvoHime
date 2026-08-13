# Подплан 0 — замена desktop shell на Electron

Статус: локальная реализация миграции завершена и проверена на текущей Windows-машине.
Закрыты этапы 0–5; WinUI оставлен только как временный compatibility oracle.
Проверки других ОС и отдельных чистых машин отложены до работы на них.
Зафиксированный стек: `docs/plans/0-electron-stack-decision.md`.

## Цель и границы

Заменить WinUI 3 desktop shell на Electron, сохранив Rust Core, supervisor,
transaction worker, SQLite, права, секреты и versioned named-pipe IPC.
Electron — заменяемая оболочка: renderer не получает прямой доступ к workspace,
pipe, shell, SQLite или model provider.

```text
EvoHime.exe (Electron main + renderer)
        │ desktop-ipc-v1 через named pipe
evohime-core.exe (state, policy, tools, SQLite)
        ▲
evohime-supervisor.exe (mutex, Job Object, lifecycle, recovery)
        │
evohime-transaction.exe (install/update/rollback)
```

Core остаётся единственной security authority. Main владеет только shell,
tray, окнами и transport orchestration; preload экспортирует узкий typed API.
Секреты не попадают в renderer, настройки, логи или diagnostics.

## Выполненная реализация

- Зафиксированы Electron 43, TypeScript, React, Vite, npm lockfile,
  `contextIsolation`, sandbox, CSP, navigation policy и production bundle checks.
- Реализован main/preload IPC adapter с handshake, HMAC session proof,
  bounded frames, reconnect, replay/resync и real-Core E2E.
- Перенесены workspace picker, task timeline, Files/Git, policy/diagnostics,
  tray/notifications/settings и bounded Terminal.
- Terminal работает только через Core approval/policy, ограничивает command,
  argv, cwd, timeout и output; renderer не запускает shell или PTY.
- Electron main умеет поднять supervisor из packaged payload, дождаться launch
  context и подключиться к Core; второй lifecycle chain не создаётся.
- Windows package собирается через Electron Builder, native binaries входят в
  переносимый payload, Inno Setup остаётся install/upgrade/rollback path.
- Workflow, manifest, installer shortcut и install smoke обновлены под
  `resources/app.asar` и native-компоненты Electron package.

Закрытые результаты и промежуточные таблицы намеренно не дублируются здесь:
источником истины остаются код, тесты и git history.

## Локальная acceptance-проверка

Проверено на текущей Windows-машине:

- Electron: `npm test -- --run` — 96 тестов; `npm run typecheck`, `npm run build`,
  `npm run check:bundle` — успешно.
- Real-Core Electron E2E: handshake, restart/reconnect и authenticated
  rejection сценарии — успешно.
- Rust: `cargo test --locked --workspace` — все тесты успешно.
- WinUI compatibility oracle: `EvoHime.Tests` — 30 тестов, `EvoHime.IpcTests`
  — 24 теста, успешно.
- Packaging: native package, version и workflow smoke — успешно; реальный
  unpacked package собран, `EvoHime.exe` запущен и оставался живым после 5 секунд.
- `git diff --check` — чисто.

Не выполнялись и не блокируют текущую локальную работу: проверка Windows 10,
второй физической машины, подписания сертификатом и полноценного Inno Setup
install/upgrade/uninstall smoke, если соответствующий внешний компонент
отсутствует в текущем окружении.

## Остаточный compatibility policy

WinUI project и C# tests сохраняются только как временный compatibility набор.
Они не входят в Electron production payload. Удаление compatibility oracle —
отдельная задача после двух согласованных acceptance cycles; новые Electron
изменения не должны возвращать WinUI runtime в production package.

## Rollback и инварианты

- При IPC major mismatch приложение показывает recovery state, без silent
  fallback на неизвестную схему.
- При сбое Core supervisor отвечает за restart/recovery и Job Object.
- При ошибке package/update transaction worker восстанавливает предыдущий
  согласованный payload; Electron autoUpdater и Squirrel не используются.
- Изменение IPC требует обновления Rust, Electron и compatibility tests.
