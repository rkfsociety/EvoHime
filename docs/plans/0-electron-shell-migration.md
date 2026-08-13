# Подплан 0 — замена desktop shell на Electron

Статус: базовая миграция Electron и этапы 0–3 частично завершены; этапы 4–6
остаются в работе. Проверка выполняется на текущей Windows-машине.
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
- Перенесены workspace picker, task timeline, read-only Files/Git,
  policy/diagnostics, tray/notifications/settings и bounded Terminal.
- Editor-срез реализован через Core-owned `PrepareBuild`/`ApplyApprovedBuild`:
  descriptor form, bounded diff preview, explicit apply и failure states.
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
- Fault smoke: forced Core exit восстановлен supervisor; forced supervisor exit
  не оставляет Core — успешно.
- `git diff --check` — чисто.

Не выполнены: проверка Windows 10, второй физической машины, code signing,
полный Inno Setup install/upgrade/uninstall smoke, fault-injection matrix,
startup/IPC/package budgets и soak/low-memory проверки.

## Остаток реализации

1. Завершить supervisor liveness contract: записывать liveness identity в
   launch context и отслеживать owner loss в Electron main; heartbeat остаётся
   только дополнительным health signal.
2. Добавить локальный package/lifecycle smoke без зависимости от WinUI и
   проверить, что release version действительно попадает в Electron package.
3. Расширить fault/acceptance scripts сценариями read-only и locked workspace,
   provider outage, bounded reconnect и diagnostics export; зафиксировать
   измеренные startup/IPC/package baselines. Core/supervisor restart уже
   покрыт `scripts/electron-fault.tests.ps1`.
4. После двух одинаковых acceptance cycles отдельным task-only коммитом
   убрать WinUI runtime из production CI/package, сохранив compatibility oracle
   только если он ещё нужен.

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
