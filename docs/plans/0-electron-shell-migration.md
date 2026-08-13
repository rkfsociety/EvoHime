# Подплан 0 — замена desktop shell на Electron

Статус: план миграции; реализация не начата
Порядок: 0 из 6; prerequisite для UI-частей планов 1–5

## Цель и архитектурный invariant

Заменить WinUI 3 desktop shell на Electron, сохранив Rust Core, supervisor,
transaction worker, SQLite, права, секреты и versioned named-pipe IPC.
Electron — заменяемая оболочка; Core не должен зависеть от Electron-specific
semantics. Renderer не является web-панелью, не поднимает HTTP-сервер и не
использует внешний Node runtime.

## Целевая архитектура и ownership

```text
EvoHime.exe                 Electron main process
        │                   single-instance, tray, windows, preload, IPC adapter
        └─ evohime-supervisor.exe
               └─ evohime-core.exe
        evohime-transaction.exe  backup, update, rollback worker
        Electron renderer       bundled TypeScript UI, без Node API
```

`EvoHime.exe` владеет пользовательским окном, tray и Electron single-instance
lock. Supervisor остаётся единственным владельцем Core lifecycle: он владеет
mutex/Job Object, запускает, перезапускает и корректно завершает Core. Electron
не создаёт вторую lifecycle-цепочку и не владеет Job Object. Supervisor получает
от main parent-liveness token/heartbeat и завершает Core при потере владельца,
кроме явно поддержанного tray-состояния. При падении renderer окно
восстанавливается; при падении Electron supervisor завершает Core по штатной
политике, а новый запуск поднимает Core через supervisor. При закрытии окна
Core завершается только по явной политике quit; закрытие в tray не завершает
Core.

Renderer получает только типизированный allow-list API через `preload` и
`contextBridge`. Уровни доверия: renderer — недоверенный, preload — узкий
bridge, Electron main — transport/orchestration layer, Core — единственная
security authority. Core самостоятельно проверяет capabilities, policy,
approvals, paths, workspace scope, executable/argv и secret operations даже
если команда пришла от Electron main.

## Что не меняется

- Rust Core и его capability/policy engine остаются источником истины;
- `desktop-ipc-v1`, sequence replay, bounded frame size и `StopTask` сохраняют
  совместимость major-версии;
- supervisor, Job Object, recovery, SQLite migrations, event journal и
  transaction worker остаются native-компонентами;
- Credential Manager/DPAPI используются для секретов;
- plaintext secrets запрещены в renderer, preload, main, settings, stdout/stderr,
  crash reports, JSONL и IPC diagnostics;
- поддерживаются Windows 10 и Windows 11 x64;
- до acceptance сохраняются один пользовательский shortcut и WinUI fallback.

## Этапы реализации

### 0. Контракт, стек и migration spike

- зафиксировать конкретные Electron major/LTS, Node runtime Electron, TypeScript
  target, renderer framework, bundler, package manager и packaging tool;
- создать `desktop/evohime-electron` с main, preload и renderer слоями;
- зафиксировать production/dev security profiles: DevTools/hot reload допустимы
  только в dev, production использует строгий CSP и не принимает debug flags;
- проверить `sandbox: true` вместе с фактическим preload API, оставить только
  минимально необходимые Electron APIs и не ослаблять sandbox как workaround;
- провести spike named-pipe клиента: async I/O, reconnect, backpressure,
  bounded queue, timeout и crash/restart Core; pipe-логика живёт в одном thin
  adapter layer main process;
- определить `asar`/unpacked layout, code signing и package smoke;
- проверить single-instance, graceful close, tray/quit, DPI/scaling, dark theme
  и Windows 10/11 на реальных машинах или зафиксировать недоступную проверку;
- renderer не получает прямой доступ к workspace, pipe, shell или Core socket.

**Gate 0:** выбранный стек собирает подписываемый desktop package, пустое окно
запускается без консоли и браузера, sandbox/CSP проходят smoke, а pipe spike
подтверждает reconnect и bounded behavior либо документирует переход к bridge.

### 1. IPC adapter и контракт

- реализовать в main process клиент `desktop-ipc-v1` через Windows named pipe;
- сделать генерацию TypeScript envelope/types из канонического protobuf
  обязательной CI-проверкой; ручные типы допустимы только как временный
  проверяемый bootstrap;
- pipe создаёт только Core; supervisor управляет его lifecycle, но не подменяет
  endpoint. Применяются Windows security descriptor с DACL для ожидаемого
  пользователя/session и непредсказуемое имя endpoint; Core не полагается
  только на секретность имени;
- handshake фиксирует major/minor, nonce/challenge, client role и capabilities;
  challenge связывается с контролируемым supervisor launch/session context.
  Core отвергает несовместимую версию, malformed identity и неверный challenge.
  PID/path/signature могут использоваться как дополнительные проверки, но не
  считаются единственной аутентификацией. Защита от процесса другого
  пользователя обеспечивается ACL; модель угроз для полностью доверенного
  текущего пользователя документируется отдельно;
- adapter владеет handshake/reconnect/retry, sequence replay, duplicate/replay
  behavior, stale-pipe cleanup, timeouts, bounded frames, max queue и
  backpressure. Утраченный sequence не считается успешно восстановленным;
- добавить contract tests normal/malformed/oversized/replay/duplicate/
  disconnect/timeout cases и E2E tests с настоящим собранным Core, включая
  kill/restart Core;
- до удаления WinUI сохранять C# IPC tests как compatibility oracle.

**Gate 1:** generated types совпадают с protobuf, adapter проходит contract и
real-Core E2E tests, reconnect/replay не теряют state, лимиты и ACL проверены,
а Electron main не содержит Core business/security logic.

### 2. Базовый security foundation

- реализовать узкий preload allow-list с `contextIsolation: true`,
  `sandbox: true`, `nodeIntegration: false`;
- production CSP: `default-src 'self'`, без `unsafe-eval`, без remote content,
  inline scripts и произвольной навигации; `unsafe-inline` для стилей не
  разрешать без обоснования и отдельного security review;
- `will-navigate`, `window.open`, новые окна, `webviewTag`, внешние схемы и
  `file://` вне packaged renderer запрещены по умолчанию; external URL
  открывается только через проверенный allow-list и явный shell-open path;
- в production отключить DevTools, menu и shortcuts, отфильтровать
  `--remote-debugging-port` и debug flags; source maps в production package не
  включать;
- redaction выполнять до записи diagnostics в main/Core; не кэшировать secret
  values и не пересылать их в crash reports, stdout/stderr или settings;
- dependency lockfile хранить в репозитории, использовать frozen-lockfile в CI,
  pin Electron/Node, выполнять dependency audit, явно allow-list-ить
  postinstall scripts и исключать devDependencies из production package;
- покрыть preload API, redaction, navigation policy и неразрешённые команды
  regression tests.

**Gate 2:** security tests и production static checks проходят; ни renderer, ни
main не могут выполнить secret/workspace/shell operation вне Core policy.

### 3. Вертикальные UI-срезы

Переносить экраны по одному и допускать следующий срез только после его gate:

1. workspace picker, persisted selection, startup/reconnect и failure states;
2. task timeline, streaming, cancellation, approval и recovery;
3. Files, Editor и Git;
4. policy/permission panel, backup/restore progress и diagnostics;
5. tray, notifications, settings/provider references;
6. bounded Terminal — последним среди shell-like функций.

Каждый срез использует только IPC-команды/события Core. UI-тесты проверяют
отображение и переходы, а Core повторно выполняет security validation.

Обязательные UI-состояния: Core/supervisor не запустился, pipe disconnected,
reconnecting, replaying, Core killed during task, disk full, read-only или
locked workspace, provider outage, degraded и fatal recovery screen. Ошибка
preload логируется redacted-событием в main и даёт reload renderer без полного
перезапуска приложения, если это безопасно.

Для Terminal renderer не управляет PTY напрямую: команды идут через Core
policy/approval. Вывод ограничивает scrollback и фильтрует опасные escape/OSC;
file/exec links не кликабельны без явного подтверждения.

**Gate каждого среза:** focused UI tests, real-Core IPC scenario, reconnect/
failure-state check и проверка отсутствия прямых filesystem/shell calls.

### 4. Packaging, lifecycle и diagnostics

- Electron runtime встраивается в переносимый Windows package; проверить
  `asar`/unpacked native assets, подпись всех исполняемых компонентов и
  отсутствие лишних Chromium/Node artifacts;
- текущие Inno Setup и transaction worker остаются единственным install,
  upgrade, rollback и update path. Electron `autoUpdater`, Squirrel и второй
  update механизм запрещены;
- transaction tests покрывают clean install, upgrade, interrupted update,
  reboot during update, rollback, uninstall, orphan cleanup, signature и
  отсутствие console window/browser launcher;
- Electron main/renderer events пишутся redacted JSONL в контролируемый каталог
  diagnostics с разделением потоков, rotation и max-size policy; Core journal
  остаётся authoritative для agent events. Экспорт diagnostics удаляет secrets;
- проверить tray, quit, single-instance Electron и supervisor mutex/Job Object
  совместно, включая coexistence WinUI fallback без одинаковых mutex/shortcut
  conflicts.

**Gate 4:** подписанный package проходит install/upgrade/rollback/uninstall и
diagnostics smoke на Windows 10/11 без второй lifecycle/update цепочки.

### 5. Нефункциональные и fault-injection проверки

Зафиксировать baseline и budget для startup, idle/soak memory, CPU, IPC latency,
queue growth и package size; конкретные лимиты утверждаются после spike, а не
берутся произвольно. CI/acceptance fault injection включает kill Core, kill
supervisor, Core restart, long-running task, provider/network outage, disk full,
read-only/locked workspace, low-memory pressure и reboot во время обновления.

**Gate 5:** budget не нарушен, leak/soak не выявлен, fault matrix имеет
ожидаемые UI/recovery outcomes и redacted diagnostics.

### 6. Удаление WinUI и release acceptance

После Gate 0–5 удалить WinUI runtime из production package, launcher и CI.
Проект и тесты остаются только как явно помеченный временный compatibility
набор до отдельного task-only коммита после двух одинаково определённых
acceptance cycles. В cycle входят clean install, upgrade, forced Core и
supervisor crash/recovery, forced Electron/renderer recovery, interrupted
update, rollback, uninstall и проверки Windows 10/11. Отдельная ветка для
исторического кода не создаётся.

## Acceptance criteria

- чистая Windows 10/11 x64 запускает приложение без Node.js, .NET SDK, Rust,
  браузера или WSL;
- один `EvoHime.exe` открывает Electron UI, не создаёт консоль/HTTP server и
  не запускает конкурирующий lifecycle;
- supervisor владеет Core restart/Job Object, Electron main владеет только
  shell/transport orchestration, Core остаётся security authority;
- pipe ACL/handshake, version negotiation, bounded frames, replay,
  reconnect, cancellation, approval и recovery проходят tests;
- Files/Git/Editor/Terminal не имеют прямого доступа к workspace и shell из
  renderer или PTY;
- secret values отсутствуют в renderer/main logs, diagnostics, crash dumps,
  settings и exported events;
- fault-injection matrix и install/upgrade/rollback/uninstall проходят на
  Windows 10 и Windows 11;
- `cargo test --workspace`, generated IPC contract tests, Electron
  typecheck/unit/E2E tests, package smoke и Windows UI acceptance проходят
  свежим прогоном;
- package содержит только необходимые runtime components, подписан, без
  browser launcher, console window и production source maps; `git diff --check`
  чист.

## Риски и rollback

- Если named-pipe adapter не достигает подтверждённых reliability/security
  gates, до UI-срезов выбирается отдельный Rust IPC bridge с тем же контрактом;
  renderer никогда не получает прямой доступ к pipe.
- Если sandbox несовместим с нужным preload API, API сужается и проверяется
  повторно. Отключение `contextIsolation`, `sandbox` или включение
  `nodeIntegration` не является локальным workaround и требует отдельного
  security decision.
- До двух acceptance cycles сохраняется запускаемый WinUI fallback и
  compatibility tests. Production installer переключается на Electron только
  после install/upgrade/rollback/fault gates; rollback возвращает предыдущий
  подписанный package через transaction worker.
- При несовместимости IPC major/minor handshake приложение показывает safe
  recovery state и предлагает обновить согласованный package; silent fallback
  на неизвестную схему запрещён.
