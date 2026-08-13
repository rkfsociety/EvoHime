# Подплан 0 — переход с WinUI на Electron

Статус: утверждённый план миграции; реализация не начата
Порядок: 0 из 6; prerequisite для UI-частей планов 1–5

## Цель

Заменить нестабильную WinUI 3 оболочку на Electron desktop shell, сохранив
Rust Core, supervisor, transaction worker, SQLite, права, секреты и
versioned named-pipe IPC. Приложение должно быть единым Windows desktop
продуктом: Electron renderer не является отдельной web-панелью и не запускает
HTTP-сервер, браузер или внешний Node runtime.

## Целевая архитектура

```text
EvoHime.exe                 Electron main process
        │ preload/contextBridge, typed commands/events
        ▼
Electron renderer           bundled TypeScript UI, без nodeIntegration
        │ desktop-ipc-v1 / Windows named pipe через main process
        ▼
evohime-core.exe            Rust agent loop, tools, policies, secrets, SQLite
        ▲
evohime-supervisor.exe     mutex, Job Object, restart, JSONL diagnostics
        │
evohime-transaction.exe    backup, update and rollback worker
```

Renderer получает только типизированный allow-list API через `preload` и
`contextBridge`. Он не имеет доступа к файловой системе, shell, environment,
SQLite, Credential Manager, DPAPI или model provider. Core остаётся единственным
владельцем состояния и security boundary. Electron main process отвечает за
окно, single-instance, tray, lifecycle и транспорт IPC, но не переносит в себя
бизнес-логику Core.

## Что не меняется

- Rust Core и его capability/policy engine остаются источником истины;
- `desktop-ipc-v1`, sequence replay, bounded frame size и `StopTask` сохраняют
  совместимость major-версии;
- supervisor, Job Object, recovery, SQLite migrations, event journal и
  transaction worker остаются native-компонентами;
- Credential Manager/DPAPI используются для секретов, plaintext secrets в
  renderer, Electron logs и settings запрещены;
- поддерживаются Windows 10 и Windows 11 x64, без привязки к одной версии
  Windows 11;
- установочный сценарий и один desktop shortcut сохраняются, пока новый
  Electron runtime не пройдёт полный acceptance.

## Этапы реализации

### 0. Контракт и migration spike

- зафиксировать Electron и Node версии, TypeScript target, renderer framework
  и способ упаковки;
- создать desktop-only проект `desktop/evohime-electron` с main, preload и
  renderer слоями;
- запустить пустое окно из `start-dev.ps1` рядом с текущим WinUI smoke;
- проверить single-instance, graceful close, DPI/scaling, темную тему и
  запуск на Windows 10/11;
- не подключать renderer напрямую к workspace или Core socket.

### 1. IPC adapter

- реализовать в main process клиент `desktop-ipc-v1` поверх Windows named pipe;
- сгенерировать TypeScript envelope/types из канонического protobuf-контракта
  либо добавить проверяемый ручной adapter без расхождения схем;
- поддержать handshake, major/minor compatibility, bounded frames,
  sequence replay, reconnect и `StopTask`;
- добавить Rust↔Electron compatibility tests для нормальных, malformed,
  oversized, replay и disconnect cases;
- до удаления WinUI временно сохранить C# IPC tests как cross-client oracle.

### 2. Вертикальные UI-срезы

Переносить экраны по одному, не копируя бизнес-логику в renderer:

1. workspace picker, persisted selection и startup/reconnect state;
2. task timeline, streaming, cancellation, approval и recovery states;
3. Files, Editor, Git и bounded Terminal;
4. policy/permission panel, backup/restore progress и diagnostics;
5. settings/provider references, tray, notifications и safe error states.

Каждый срез должен использовать только IPC-команды и события Core. UI-тесты
проверяют отображение и пользовательские переходы, а security-решения и
валидацию повторно проверяются Core.

### 3. Secrets и trusted boundary

- перенести оставшиеся provider settings/secret operations из UI-специфичного
  C# слоя в Core IPC;
- оставить в Electron только logical credential reference и redacted status;
- запретить plaintext secret в renderer state, DevTools output, crash reports,
  JSONL и IPC diagnostics;
- добавить regression tests на preload API, redaction и попытки renderer
  вызвать неразрешённые команды.

### 4. Безопасность Electron

Обязательная конфигурация production window:

- `contextIsolation: true`;
- `sandbox: true`, где это совместимо с preload;
- `nodeIntegration: false`;
- строгий CSP без `unsafe-eval` и без произвольной навигации;
- запрет remote content, открытых DevTools и непроверенных external links;
- allow-list для IPC методов, схем URL и window lifecycle;
- renderer не принимает пути, executable, argv или policy decisions как
  доверенные значения.

Проверить dependency lockfile, Electron code signing/update artifacts, secret
search и отсутствие старого browser launcher/HTTP server.

### 5. Packaging и lifecycle

- Electron runtime встраивается в переносимый Windows package;
- текущий Inno Setup и transaction worker сохраняются как outer installer,
  пока install/upgrade/rollback не будут подтверждены на новом package;
- `EvoHime.exe` остаётся пользовательской точкой запуска;
- supervisor по-прежнему запускает и контролирует Core, а Electron не создаёт
  вторую конкурирующую lifecycle-цепочку;
- проверить clean install, upgrade, interrupted upgrade, rollback, uninstall,
  single-instance и запуск без консоли/браузера.

### 6. Удаление WinUI и release acceptance

После прохождения всех gates удалить WinUI runtime из production package,
launcher и CI. Сначала оставить проект и тесты как временный compatibility
набор, затем удалить их отдельным task-only коммитом после двух успешных
Windows acceptance cycles.

## Acceptance criteria

- приложение запускается на чистой Windows 10 и Windows 11 x64 без установки
  Node.js, .NET SDK, Rust, браузера или WSL;
- один `EvoHime.exe` открывает Electron UI, не создаёт консоль и не поднимает
  HTTP server;
- Core restart, reconnect, replay, cancellation, approval и recovery работают
  через тот же versioned IPC contract;
- Files/Git/Editor/Terminal не получают прямой доступ к workspace и shell из
  renderer;
- secret values отсутствуют в renderer logs, diagnostics, crash dumps,
  settings и exported events;
- install, upgrade, rollback, uninstall и single-instance проходят на Windows
  10 и Windows 11;
- `cargo test --workspace`, Electron typecheck/unit tests, IPC compatibility,
  package smoke и Windows UI acceptance проходят свежим прогоном;
- `git diff --check` чист, package содержит только необходимые runtime
  компоненты, а WinUI больше не является обязательной зависимостью запуска.

## Риски и rollback

- Если Electron main process не сможет стабильно работать с named pipe,
  добавляется отдельный Rust IPC bridge с тем же контрактом; прямой доступ
  renderer к pipe не допускается.
- Если sandbox preload несовместим с нужным API, сначала сужается API и
  обновляются tests; отключение `contextIsolation` или включение
  `nodeIntegration` не является допустимым workaround.
- До полного acceptance сохраняется запускаемый WinUI fallback в исходниках и
  CI compatibility path. Production installer переключается на Electron только
  после install/upgrade/rollback gate.
