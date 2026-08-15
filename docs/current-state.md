# EvoHime — текущее состояние

Обновлено: 2026-08-14.

## Продукт

EvoHime — локальный Windows-клиент для coding-agent задач. Пользовательское имя агента — **Ева**. Текущая версия клиента — `0.0.000033`.

Пользователь получает один `EvoHime-Setup.exe`. После установки на рабочем столе появляется один ярлык `EvoHime`, запускающий `EvoHime.exe`.

## Runtime

- `EvoHime.exe` — Electron main process с bundled renderer; native package и installer собирают Electron shell;
- `evohime-core.exe` — Rust agent loop, model gateway, tools, permissions, approvals и SQLite;
- `evohime-supervisor.exe` — single-instance mutex, Job Object, restart и диагностика;
- `evohime-transaction.exe` — скрытый transaction worker для backup, commit и rollback обновлений, включая подмену локально пересобранного пакета;
- versioned protobuf over Windows named pipe — единственный UI/Core transport;
- `%LOCALAPPDATA%\EvoHime` — локальные данные и JSONL-логи; `source`, `update-staging`, `update-state` и `update.json` принадлежат обновлению.

Core и supervisor — внутренние компоненты установки, не отдельные пользовательские продукты.

## Готово

### Runtime и foundation

- Core, SQLite, IPC, supervisor, event replay и diagnostics;
- streamed task timeline, cancellation и approval round-trip;
- Windows package smoke tests и Windows CI;
- единый Inno Setup installer с одним desktop shortcut; установленный клиент сам поднимает supervisor, а supervisor — Core;
- обновление из исходников без релизов: клиент сверяет `evohime.build.json` с вершиной ветки, ставит недостающие инструменты сборки через winget, обновляет свой git checkout, пересобирает продукт локально и отдаёт готовый пакет `evohime-transaction.exe --apply-staging`. При запуске идёт блокирующий gate с шагами пересборки и кнопкой «Пропустить», у запущенного клиента — фоновая сборка и баннер с перезапуском;
- upgrade smoke в CI, автоматический rollback и recovery незавершённой транзакции перед запуском Core;
- один постоянный релиз `installer` с описанием из `installer/release-notes.md`: `EvoHime-Setup.exe` в нём перезаписывается по ручному запуску workflow, новых релизов и версионных тегов не создаётся. Постоянная ссылка — `releases/latest/download/EvoHime-Setup.exe`. Установщик нужен только для первой установки, дальше клиент обновляет себя сам;
- имя агента «Ева» передаётся в system context Core;
- Core-owned build policy и её хранение;
- durable recovery foundation для длительных запусков и reconciliation;
- `run_policy` — неизменяемый snapshot бюджета запуска (итерации, wall clock, tool calls, токены, стоимость); Core проверяет его перед каждым эффектом, renderer может только показать значения;
- `pulse` в supervisor — честный локальный digest расписаний: dead-letter даёт `Failed`, пропуски и ошибки — `Degraded`, успех не подменяет отказ.

### Безопасность и данные

- Core-first SQLite backup/restore: Online Backup API, WAL checkpoint, DPAPI payload protection, checksum, preview, approval, progress, safety backup, rollback и redacted audit; долгая операция отменяется командой `CancelDatabaseOperation`;
- filesystem.search исключает hard-default secret/auth paths, не следует symlink/reparse-обходам и не требует POSIX shell;
- shell blocklist расширен для Windows launcher/LOLBin семейств; recovery timeline различает `RECOVERING`, `BLOCKED`, `WAITING_APPROVAL` и `FAILED`;
- ключ провайдера хранится main-процессом Electron: значение шифруется OS (`safeStorage`, DPAPI на Windows) и лежит в `%LOCALAPPDATA%\EvoHime\shell\provider.json` с режимом `600`. Renderer получает только summary «ключ задан/не задан», а Core — переменные окружения выбранного провайдера через supervisor. Смена ключа перезапускает supervisor и Core;
- вкладка «Ревью планов» принимает Markdown до 512 КБ, запускает 2–8 последовательных read-only reviewer calls (по одному запросу за раз, чтобы не упираться в лимиты провайдера) и отдельную synthesis call; результаты доступны через Core history и экспортируются в Markdown;
- base URL провайдера принимается только по `https`, либо `http` на loopback, чтобы ключ не ушёл на произвольный хост.
- approval.required передаёт bounded structured preview для команд, записи файлов и unified diff; Electron показывает его в TaskTimeline и Terminal, а Core сохраняет exact-call hash и повторную policy-проверку перед выполнением;
- approval-токены для tool runtime одноразовые и атомарно погашаются перед выполнением; hard-deny policy проверяет канонический путь, включая вызовы через относительные алиасы;

### Desktop shell (Electron)

- migration acceptance закрыта на Windows: UI-срезы, authenticated Core IPC, package startup, fault recovery, install/upgrade/rollback и acceptance matrix;
- левая панель — проекты и чаты (`ProjectSidebar`); аккаунт с шестернёй настроек внизу. Имя пользователя берётся из GitHub CLI, `git config user.name` или учётной записи Windows и подписывается источником;
- главный экран (`HomeScreen`) вместо заглушки: чат создаётся сам при первом запросе;
- ход задачи свёрнут в читаемую ленту (`ActivityLine`, `transcript.ts`), инструменты подписаны по-русски (`tool-names.ts`), ответы агента отображаются как Markdown (`MarkdownMessage`);
- строка репозитория над композером (`RepositoryBar`): ветка и счётчики изменений;
- выбор модели в чате (`ModelPicker`) с разделением каталога на free/paid; выбор применяется без перезапуска Core через IPC `SelectModelRequest`;
- настройки провайдера собраны в один блок (`ProviderForm`) вместо прежнего `SettingsPanel`; отдельный `WorkspacePicker` убран — папка выбирается из панели проектов;
- `RecoveryBanner` показывает подтверждённое Core состояние восстановления;
- после перезапуска Core durable build-effect и обычные agent-run проходят lease heartbeat; подтверждённый terminal event приходит в UI как `RESUMABLE`, а неподтверждённый результат остаётся `BLOCKED`; storage schema — v16;
- `OperationsPanel` («Память и Pulse») — очередь подтверждения памяти и конфликты (только metadata, с действиями «сохранить»/«отклонить»/«заменить»), плюс read-only проекция child- и schedule-событий;
- Memory Extraction: Core извлекает кандидатов в память из диалога после явного триггера пользователя (`strict` по умолчанию, `EVOHIME_MEMORY_EXTRACTION`), прогоняет их через версионируемый policy gate и сохраняет как `pending_confirmation`, пока пользователь не подтвердит. Активной памятью без approval может стать только low-risk предпочтение, произнесённое пользователем; секреты не сохраняются, конфликт не разрешается автоматически. См. `docs/architecture.md`;
- чаты shell хранятся в `%LOCALAPPDATA%\EvoHime\shell\chats.json` с ограничениями (100 чатов на workspace, 500 сообщений на чат). Это UI-группировка, а не состояние агента: Core остаётся владельцем задач и заново проверяет каждую команду.

### Разработка

- `.env.example` описывает переменные провайдера для локального запуска; `start-dev.ps1` читает `.env` по allow-list и передаёт значения только дочерним native-процессам.
- deterministic evaluation catalog и security smoke gates находятся в `tests/evals/` и запускаются из `scripts/eval-gate.tests.ps1` и `scripts/security-eval-gate.tests.ps1`; redacted CI summary сохраняется в `artifacts/eval-gate/summary.jsonl`.

## Последняя проверка checkout

14 августа 2026 года пройдены Rust, Electron, C#/WinUI compatibility, protocol,
bundle, native package, evaluation и security smoke checks. Source-update E2E
остаётся штатно пропущенным без `EVOHIME_UPDATE_E2E=1`, поскольку он выполняет
реальную пересборку и занимает значительно больше времени. Публикация
установщика разрешена только после полного Windows CI и release smoke из
[`../SECURITY.md`](../SECURITY.md).

## Следующие направления

1. hardening credentials, recovery и diagnostics;
2. поддерживать Windows 10/11 CI и compatibility suite, не возвращая web runtime;
3. informative ARM64/Insider compatibility runs.

## Граница продукта

Пользовательский продукт ограничен `EvoHime-Setup.exe`, `EvoHime.exe`, локальным Core, supervisor и данными в профиле Windows. Исследовательские и экспериментальные каталоги не входят в установочный runtime.

Legacy web UI, HTTP server, browser launcher и PostgreSQL migrations удалены из репозитория. Electron UI и authenticated versioned named-pipe IPC — текущая пользовательская оболочка и transport boundary; WinUI остаётся временным compatibility runtime для совместимости и тестов.
