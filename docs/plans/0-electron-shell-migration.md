# Подплан 0 — замена desktop shell на Electron

Статус: этапы 0, 1 и 2 закрыты; срезы 1–4 этапа 3 завершены.
Следующий срез — tray, notifications, settings/provider references.
Порядок: 0 из 6; prerequisite для UI-частей планов 1–5
Зафиксированный стек и результаты spike: `docs/plans/0-electron-stack-decision.md`

## Цель и архитектурный invariant

Заменить WinUI 3 desktop shell на Electron, сохранив Rust Core, supervisor,
transaction worker, SQLite, права, секреты и versioned named-pipe IPC.
Electron — заменяемая оболочка; Core не должен зависеть от Electron-specific
semantics. Renderer не является web-панелью, не поднимает HTTP-сервер и не
использует внешний Node runtime.

## Целевая архитектура и ownership

```text
native launch contract      starts/attaches the signed lifecycle owner
        ├─ evohime-supervisor.exe  mutex, Job Object, Core lifecycle
        │       └─ evohime-core.exe
        ├─ EvoHime.exe              Electron main, tray, windows, IPC adapter
        │       └─ Electron renderer bundled TypeScript UI, без Node API
        └─ evohime-transaction.exe  independent update/rollback worker
```

Подписанный native launch contract запускает supervisor как независимый
lifecycle owner (не как обычного дочернего процесса Electron и не внутри его
Job Object), а затем подключает или запускает `EvoHime.exe`. Supervisor владеет
single-instance/mutex, Job Object, запуском, рестартом и завершением Core.
Electron владеет только окном, tray и своим single-instance handoff; второй
запуск передаёт аргументы существующему main и фокусирует окно.

Для связи с Electron supervisor использует OS-owned liveness handle/event,
переданный через launch contract; heartbeat — только дополнительный health
signal. Закрытый handle считается crash/owner loss после короткого grace и
reconnect window; зависший main обнаруживается timeout heartbeat. В tray
Electron явно переводит session в keep-alive state, а Force Quit/закрытие
handle возвращает обычную quit policy. Новый Electron подключается к живому
supervisor и выполняет IPC resync, а не запускает конкурирующий Core.

`evohime-transaction.exe` не входит в Core Job Object. Core авторизует
операцию и формирует подписанный operation manifest, supervisor запускает
worker через отдельный authenticated operation channel и передаёт ему
статус/результат. После начала install/update/rollback worker владеет своей
transaction lifecycle независимо от Electron/Core; operation mutex запрещает
две одновременные операции. Worker не заменяет активные бинарники до
подтверждённого handoff и умеет восстановиться после reboot.

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
- оформить выбор стека отдельным reviewed commit; изменение pinned versions
  после Gate 0 требует повторного review;
- создать `desktop/evohime-electron` с main, preload и renderer слоями;
- зафиксировать production/dev security profiles: DevTools/hot reload допустимы
  только в dev, production использует строгий CSP и не принимает debug flags;
- проверить `sandbox: true` вместе с фактическим preload API, оставить только
  минимально необходимые Electron APIs и не ослаблять sandbox как workaround;
- провести spike named-pipe клиента: async I/O, reconnect, backpressure,
  bounded queue, timeout и crash/restart Core; pipe-логика живёт в одном thin
  adapter layer main process. Проверить stale pipe, concurrent reconnect,
  session change, UAC/elevation и memory pressure;
- определить `asar`/unpacked layout, code signing и package smoke;
- проверить single-instance, graceful close, tray/quit, DPI/scaling, dark theme
  и Windows 10/11 на реальных машинах или зафиксировать недоступную проверку;
- renderer не получает прямой доступ к workspace, pipe, shell или Core socket.

Gate 0 закрыт. Зафиксированный стек и текущие ограничения остаются в
`docs/plans/0-electron-stack-decision.md`; результаты проверок здесь не
дублируются.

**Gate 0:** выбранный стек собирает подписываемый desktop package, пустое окно
запускается без консоли и браузера, sandbox/CSP проходят smoke, launch contract
подтверждает independent supervisor/liveness behavior, а pipe spike имеет
зафиксированные reconnect latency, replay/resync outcome, streaming/backpressure,
stale-pipe/concurrent-reconnect и ACL/challenge результаты. При провале любого
критерия документируется переход к bridge до начала UI-срезов.

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
  challenge и имя pipe передаются от supervisor к Core через защищённый launch
  context (не через непроверенные renderer arguments), а session context включает
  ожидаемые user SID и Windows logon session/LUID. Challenge одноразовый,
  ограничен временем и не принимается повторно;
  Core отвергает несовместимую версию, malformed identity и неверный challenge.
  PID/path/signature могут использоваться как дополнительные проверки, но не
  считаются единственной аутентификацией. Защита от процесса другого
  пользователя обеспечивается ACL; модель угроз для полностью доверенного
  текущего пользователя документируется отдельно;
- adapter владеет handshake/reconnect/retry, sequence replay, duplicate/replay
  behavior, stale-pipe cleanup, timeouts, bounded frames, max queue и
  backpressure. Command queue не теряет команды: при переполнении отправитель
  получает controlled reject/block; потоковые события могут coalesce/drop по
  documented policy. Утраченный sequence не считается успешно восстановленным;
- policy: major mismatch отклоняется; одинаковый major совместим при
  поддерживаемом minor/capability intersection; неизвестные optional fields
  игнорируются, unknown required capability отклоняет только операцию,
  неизвестная command/event даёт protocol error без silent state mutation;
- resync contract: `ReplayAvailable` replay-ит события; `ReplayUnavailable`
  требует `GetSnapshot/ResyncState`, после чего UI применяет атомарный snapshot.
  При невозможности snapshot UI показывает explicit recovery и не скрывает
  partial replay/state gap;
- добавить contract tests normal/malformed/oversized/replay/duplicate/
  disconnect/timeout cases и E2E tests с настоящим собранным Core, включая
  kill/restart Core;
- до удаления WinUI сохранять C# IPC tests как compatibility oracle.

Gate 1 закрыт; transport и типизированный IPC готовы для UI-срезов.

**Gate 1:** generated types совпадают с protobuf, adapter проходит contract и
real-Core E2E tests, reconnect/replay либо детерминированно восстанавливают
state, либо показывают explicit resync/recovery без silent loss; проверены
лимиты, ACL, session binding и concurrent reconnect, а Electron main не
содержит Core business/security logic. Временный ручной bootstrap types имеет
ticket и expiry date.

### 2. Базовый security foundation

- реализовать узкий versioned product API, например `window.evohime.v1`, с
  typed `invoke/subscribe` и только нужными `clipboard`/validated external-link
  operations. Не экспортировать `ipcRenderer`, EventEmitter, MessagePort или
  Electron primitives напрямую; `fs`, `remote`, `child_process`, shell/exec и
  environment access запрещены;
- реализовать этот API с `contextIsolation: true`, `sandbox: true`,
  `nodeIntegration: false`; любое расширение surface требует security review;
- production CSP: `default-src 'self'`, без `unsafe-eval`, без remote content,
  inline scripts и произвольной навигации; `unsafe-inline` для стилей не
  разрешать без обоснования и отдельного security review;
- `will-navigate`, `window.open`, новые окна, `webviewTag`, внешние схемы и
  `file://` вне packaged renderer запрещены по умолчанию; external URL
  открывается только через проверенный allow-list и явный shell-open path;
- в production отключить DevTools, menu и shortcuts, отфильтровать
  `--remote-debugging-port` и debug flags; source maps в production package не
  включать;
- redaction выполнять общим проверяемым layer до записи diagnostics в main/Core;
  покрыть secrets, paths, tokens, provider keys, partial task payloads,
  environment, command-line arguments и stack traces. Core redacts its own
  journal, main redacts shell events, а regression tests проверяют fixtures и
  exported diagnostics;
- до Gate 1 документировать threat model fully trusted current-user malware:
  ACL/session binding защищают от другого user/session, но не обещают защиту
  от malware, уже действующего с теми же правами пользователя;
- настроить Chromium permission handlers deny-by-default для media,
  geolocation, notifications, clipboard read, MIDI, serial, USB, Bluetooth и
  screen capture;
- dependency lockfile хранить в репозитории, использовать frozen-lockfile в CI,
  pin Electron/Node, выполнять dependency audit, явно allow-list-ить
  postinstall scripts и исключать devDependencies из production package;
- покрыть preload API, redaction, navigation policy и неразрешённые команды
  regression tests.

Gate 2 закрыт; renderer и main используют только утверждённый bridge/Core
контракт.

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
reconnecting, replaying, partial reconnect/state gap, IPC version mismatch,
capability/policy rejection, Core killed during task, disk full, read-only или
locked workspace, provider outage, degraded и fatal recovery screen. Ошибка
preload логируется redacted-событием в main и даёт reload renderer без полного
перезапуска приложения, если это безопасно; reload восстанавливает session и
текущие task subscriptions через resync.

Automatic renderer/preload reload ограничен N failures за T времени. После
порога automatic reload прекращается и показывается minimal recovery window с
Export Diagnostics/Restart; бесконечный crash loop запрещён. Временные ошибки
могут retry-иться только по Core policy с bounded exponential backoff и
видимым состоянием retry.

Для Terminal renderer не управляет PTY напрямую: команды идут через Core
policy/approval. Вывод имеет зафиксированный после baseline scrollback limit,
фильтрует ANSI/OSC, включая OSC 8, а file/exec links запускаются только после
explicit approval через Core, не после одного UI-confirm.

Срез 1 закрыт: workspace picker готов и передаёт выбор только через main/Core.

**Gate каждого среза:** focused UI tests, real-Core IPC scenario, reconnect/
failure-state check и проверка отсутствия прямых filesystem/shell calls.

### 4. Packaging, lifecycle и diagnostics

- Electron runtime встраивается в переносимый Windows package; проверить
  `asar`/unpacked native assets, подпись всех исполняемых компонентов и
  отсутствие лишних Chromium/Node artifacts. Renderer загружается только из
  signed/packaged resources; runtime load JS/modules из writable directories,
  `%APPDATA%`, workspace и plugin discovery запрещены;
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
  совместно: второй запуск делает focus/handoff существующему main, не
  создаёт Core; WinUI и Electron используют разные mutex/pipe/data identifiers,
  но сохраняют один shortcut/protocol handler, а uninstall одного варианта не
  ломает поддерживаемый fallback.

**Gate 4:** подписанный package проходит install/upgrade/rollback/uninstall и
diagnostics smoke на Windows 10/11 без второй lifecycle/update цепочки.

### 5. Нефункциональные и fault-injection проверки

Зафиксировать baseline и budget для startup, idle/soak memory, CPU, IPC latency,
queue growth и package size; ориентировочные цели можно использовать только как
spike hypotheses, а конкретные лимиты утверждаются после spike, а не
берутся произвольно. CI/acceptance fault injection включает kill Core, kill
supervisor, Core restart, long-running task, provider/network outage, disk full,
read-only/locked workspace, low-memory pressure и reboot во время обновления.

**Gate 5:** budget не нарушен, leak/soak не выявлен, fault matrix имеет
ожидаемые UI/recovery outcomes и redacted diagnostics.

### 6. Удаление WinUI и release acceptance

После Gate 0–5 удалить WinUI runtime из production package, launcher и CI.
Проект и тесты остаются только как явно помеченный временный compatibility
набор до отдельного task-only коммита после двух одинаково определённых
acceptance cycles с checklist и recorded sign-off. В cycle входят clean machine,
clean user profile, existing/upgrade/corrupted profile, forced Core и supervisor
crash/recovery, forced Electron/renderer recovery, interrupted update, rollback,
uninstall и проверки Windows 10/11. Отдельная ветка для исторического кода не
создаётся. Пока WinUI fallback поддерживается, SQLite migrations/settings
сохраняют backward compatibility либо transaction worker восстанавливает
совместимый data snapshot при rollback.

## Acceptance criteria

- чистая Windows 10/11 x64 запускает приложение без Node.js, .NET SDK, Rust,
  браузера или WSL;
- один `EvoHime.exe` открывает Electron UI, не создаёт консоль/HTTP server и
  не запускает конкурирующий lifecycle;
- supervisor владеет Core restart/Job Object, Electron main владеет только
  shell/transport orchestration, Core остаётся security authority;
- launch contract, supervisor liveness, Electron single-instance handoff и
  tray/quit policy не оставляют orphan Core или вторую lifecycle chain;
- pipe ACL/handshake, version negotiation, bounded frames, replay,
  reconnect, resync, cancellation, approval и recovery проходят tests;
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
  browser launcher, console window, production source maps, remote debugging
  port и DevTools menu; `git diff --check` чист.

## Риски и rollback

- Если named-pipe adapter не достигает подтверждённых reliability/security
  gates, до UI-срезов выбирается отдельный Rust IPC bridge с тем же контрактом;
  bridge заранее проектируется на том же `desktop-ipc-v1`, handshake, resync и
  contract tests; renderer никогда не получает прямой доступ к pipe.
- Если sandbox несовместим с нужным preload API, API сужается и проверяется
  повторно. Отключение `contextIsolation`, `sandbox` или включение
  `nodeIntegration` не является локальным workaround и требует отдельного
  security decision.
- До двух acceptance cycles сохраняется запускаемый WinUI fallback и
  compatibility tests. Production installer переключается на Electron только
  после install/upgrade/rollback/fault gates; rollback возвращает предыдущий
  подписанный package через transaction worker и проверяет data/schema
  compatibility.
- При несовместимости IPC major/minor handshake приложение показывает safe
  recovery state и предлагает обновить согласованный package; silent fallback
  на неизвестную схему запрещён.
