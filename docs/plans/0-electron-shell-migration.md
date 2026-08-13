# Подплан 0 — замена desktop shell на Electron

Статус: реализация плана завершена для текущей Windows-машины; cross-OS
проверки и production code signing отложены до отдельного цикла.
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

- Electron: полный `npm test -- --run`, `npm run typecheck`, `npm run build`,
  `npm run check:bundle` — успешно.
- Real-Core Electron E2E: handshake, restart/reconnect и authenticated
  rejection сценарии — успешно.
- Rust: `cargo test --locked --workspace` — все тесты успешно.
- WinUI compatibility oracle: `EvoHime.Tests` — 30 тестов, `EvoHime.IpcTests`
  — 24 теста, успешно.
- Packaging: native package, release version, workflow smoke и два acceptance
  cycle — успешно; `resources/app.asar/package.json` содержит release version,
  package startup baseline измерен, Inno Setup package собран.
- Fault smoke: forced Core exit восстановлен supervisor; forced supervisor exit
  не оставляет Core — успешно.
- Installer smoke: install → upgrade `1.2.5` → `1.2.6` → uninstall — успешно.
- `git diff --check` — чисто.

Отложены вне текущего цикла: проверка Windows 10 и другой физической машины,
code signing с release-сертификатом и длительный soak/low-memory прогон с
утверждёнными продуктовым порогами.

## Остаток вне текущего цикла

Рабочих пунктов реализации не осталось. Cross-OS matrix, release code signing
и длительный soak запускаются отдельными задачами, когда появятся целевые ОС,
сертификат и согласованные performance thresholds. WinUI runtime уже не входит
в Electron production payload; C# suite оставлен как compatibility oracle.

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
