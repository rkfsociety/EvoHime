# EvoHime — текущее состояние

Обновлено: 2026-08-29. Планы 01–22 завершены и
удалены из каталога временных планов. План 19.0 добавил только пользовательский
self-repair/self-update цикл; автоматического ремонта, push или перезапуска нет.
Code signing явно исключён из текущего release scope.

План 23 выполняется последовательно: этапы 23.1 (versioned TaskCheckpoint
contract, provenance validation, immutable storage и schema v32) и 23.2
(Core runtime capture/recovery) реализованы и проверены; IPC/UI и полное
acceptance закрытие остаются этапами 23.3–23.4.

## Продукт

EvoHime — локальный Windows-клиент для coding-agent задач. Пользовательское имя агента — **Ева**. Пользовательские версионные релизы для текущего цикла не создаются; установленная сборка определяется коммитом и веткой в `evohime.build.json`.

Пользователь получает один `EvoHime-Setup.exe`. После установки на рабочем столе появляется один ярлык `EvoHime`, запускающий `EvoHime.exe`.

## Runtime

- `EvoHime.exe` — Electron main process с bundled renderer; native package и installer собирают Electron shell;
- `evohime-core.exe` — Rust agent loop, model gateway, tools, permissions, approvals и SQLite;
- `evohime-supervisor.exe` — single-instance mutex, Job Object, restart и диагностика;
- `evohime-transaction.exe` — скрытый transaction worker для backup, commit и rollback обновлений, включая post-restart health handshake и откат CI-установщика или локально пересобранного пакета;
- versioned protobuf over Windows named pipe — единственный UI/Core transport;
- `%LOCALAPPDATA%\EvoHime` — локальные данные и JSONL-логи; `source`, `update-staging`, `update-state` и `update.json` принадлежат обновлению.

Core и supervisor — внутренние компоненты установки, не отдельные пользовательские продукты.

## Готово

### Runtime и foundation

- Canonical Receipt v1: bounded JCS payload/envelope, UTF-16 key ordering,
  duplicate-key/UTF-8 checks, stable schema limits and error codes, Ed25519
  verification, result hash domain и единые Rust/Electron known-answer vectors;
  см. `docs/security/receipt-canonical-v1.md` и `contracts/receipts/v1/`;
- Receipt key lifecycle 01.2: Windows DPAPI CurrentUser и owner/SYSTEM DACL,
  защищённый active key, SQLite-authoritative transition/audit history,
  journaled rotation с crash recovery, explicit trust roots, scheduled/manual/
  compromise/recovery boundaries, signed checkpoint contract и offline
  `evohime-verify.exe`; Electron получает только status и public metadata;
- Core, SQLite, IPC, supervisor, event replay и diagnostics;
- TaskCheckpoint v1: bounded canonical contract с разделением Core-derived и
  model-proposed данных, SHA-256 content hash, immutable parent chain,
  idempotent insert, workspace/event-sequence/state-transition checks, typed
  errors, SQL/JSON metadata consistency и fallback на предыдущую валидную
  запись; storage schema — v32. Core сохраняет checkpoints перед lease,
  compaction/context projection и terminal состояниями, публикует только
  bounded metadata-событие, а recovery блокирует усечённый replay, неизвестный
  outcome и blind retry. Проверки 23.1–23.2: 10 focused storage tests,
  4 focused runtime tests, полный `evohime-local-storage` suite (196 tests),
  полный `evohime-core` suite (547 tests), `cargo check -p evohime-core`,
  `cargo fmt --all -- --check` и `git diff --check`;
- streamed task timeline, cancellation и approval round-trip;
- Windows package smoke tests и Windows CI;
- единый Inno Setup installer с одним desktop shortcut; установленный клиент сам поднимает supervisor, а supervisor — Core;
- фоновое обновление из постоянного GitHub Release: клиент сверяет зелёный commit, скачивает `EvoHime-Setup.exe` только при совпадении манифеста и SHA-256, а затем отдаёт его `evohime-transaction.exe --installer`. Запуск приложения не блокируется скачиванием; после фоновой загрузки UI показывает баннер с предложением перезапуска. Production использует CI installer, локальная пересборка через `launchPolicy: "build"` сохранена только для разработки;
- ambient listener 04.3 реализован: supervisor запускает `evohime-listener.exe` в отдельном bounded Job Object с независимым restart budget; Core и listener используют разные owner-only pipe endpoints и роль `listener` с nonce/HMAC. Аудио-крейт содержит cpal shared capture, bounded in-memory ring, deterministic 32/48→16 kHz decimation, energy VAD и fixture segmentation; privacy-gate запрещает filesystem I/O в аудио-крейте и включён в `rust-native` CI;
- движок распознавания 04.4 реализован: whisper.cpp грузится через `libloading` из каталога инструментов, выбранного резолвером `EVOHIME_LISTENER_TOOLS_DIR` → `EVOHIME_TOOLS_DIR` → `%LOCALAPPDATA%\EvoHime\tools\listener`, каждый файл сверяется с SHA-256 манифеста, необъявленная DLL рядом блокирует загрузку, раскладка ABI проверяется до первого вызова. Подпись требуется только у `onnxruntime.dll`; неподписанный собственный `whisper.dll` — штатное состояние, закреплённое тестом. Листенер открывает микрофон, сегментирует речь, дедуплицирует повторы (NFKC-нормализация, окно 60 с, near-dup ≥ 0.9) и спускается по лестнице `small → base → tiny` при RTF > 0.5 пять раз подряд, после чего останавливается с причиной `engine_degraded`. Транскрипты доходят до `ambient_utterances` с реальными языком, длительностью и порядковым номером, а Electron скачивает и проверяет набор рантайма (`listener-runtime.ts`) с показом состояния на вкладке «Распознавание речи»;
- контроль и UI слушания 04.5 реализованы: девять additive-команд протокола (теги 107–115) — включение/пауза/выбор устройства, статус, список эпизодов, текст одного эпизода, удаление транскриптов, «забыть окно», чтение и сохранение политики, решение по предложению. Состояние живёт в одном месте — `AmbientListeningRegistry` в `evohime-core`; трей, глобальный хоткей `Ctrl+Alt+M` и вкладка «Слух» отправляют одну и ту же команду и обновляются только событием `ambient.state`. Занятая комбинация хоткея объявляется недоступной, а не молча отключается; при отсутствии ответа за 5 с индикатор показывает «проверка состояния», а не «выключено». Удаление требует подтверждения и в модальном диалоге, и в ядре (`confirmed=true`). Текст высказываний отдаётся только `GetAmbientEpisode` по явному клику. Листенер перечисляет устройства, переключает их без перезапуска и подписан на `WM_DEVICECHANGE`; тихие часы политики закрывают поток по локальным часам. Отдельное разрешение `microphone_listen` переключается в новой панели безопасности и не затрагивается сменой общего режима доступа;
- мост ambient в память 04.6 реализован: `SourceTrust::Ambient` — пятое значение доверия со строго более жёсткой policy; `evaluate` возвращает `pending` с причиной `ambient_never_auto_confirms` раньше любых порогов, поэтому услышанное не может стать активной памятью без клика пользователя. Триггер — закрытие эпизода (начало следующего, минута тишины, разрыв связи с листенером), вход отдельный от диалогового, чтобы не подделывать `user_asserted`. Свой гейт запуска и свои бюджеты: `EVOHIME_AMBIENT_MEMORY` (`off` | `pending`, мусор — `off`), 6 кандидатов и 12 эпизодов в час, отдельный лимит токенов, причины `ambient_candidate_limit`/`ambient_episode_limit`; общий выключатель `EVOHIME_MEMORY_EXTRACTION=disabled` старше частного. Из речи принимаются только `preference`, `entity` и `lesson`; утверждение не в первом лице поднимает `privacy_class` до `sensitive` и скрывает тело. `provenance_source_id` кандидата — это `episode_id`, поэтому удаление эпизода отклоняет его кандидатов причиной `source_deleted`. В `OperationsPanel` — бейдж «услышано», подпись «говорящий не подтверждён» и фильтр по источнику; диалоговый `check_can_extract` не ослаблен ни в одной ветке;
- ограниченная проактивность 04.7 реализована, планом 04 закрыт: по услышанному Ева производит ровно два эффекта — карточку-предложение и неисполняемое напоминание, а `StartTask`, `ToolCall`, `FileWrite` и `NetworkRequest` отклоняются `authorize_proactive` до любого эффекта и покрыты негативными тестами. Источник предложений — те самые `constraint` и `decision`, которые 04.6 отказывается делать памятью. Дедупликация идёт по `proposal_key` (вид + тема + округлённый час) под `UNIQUE`, постоянный mute — по `mute_key` без времени, поэтому он переживает и смену временной корзины, и рестарт Core. Потолок из 04.1 (3 в час, 10 в сутки, не чаще одного раз в 10 минут, плюс пауза и тихие часы) неизменяем, а счётчики персистятся строкой схемы v26; превышение отбрасывает предложение, а не копит очередь, и дубликат бюджета не тратит. Схема v26 additive поверх v25: `ambient_proposals`, `ambient_proposal_mutes`, `ambient_proactivity_counters`. Удаление эпизода переводит его предложения в `expired` с причиной `source_deleted` **до** удаления строки эпизода. Durable-событие `ambient.proposal` несёт только `proposal_id`, `episode_id`, `kind`, bounded `subject_key` и состояние и уходит вместе с эпизодом; человекочитаемый текст карточки приходит командой `ListAmbientProposals` (тег 116). Решение несёт обязательный `idempotency_key`, принятое создаёт `work_items` в статусе `backlog` с `source_ref = episode_id`, а 24 часа молчания переводят карточку в `expired`;
- голосовые команды в режиме прослушки реализованы: обращение по имени плюс глагол («Ева, открой хром») разбирается детерминированно, без модели, в `voice_command.rs`; фраза без обращения командой не является. Открывается только запись каталога приложений (`tool-runtime/src/app_catalog.rs`): встроенные системные приложения, `App Paths` реестра и пользовательский `app-catalog.json`, с русскими синонимами, категориями («браузер») и подменой пути MSIX-пакета на alias в `WindowsApps`. Неизвестное или неоднозначное название не запускает ничего. По умолчанию услышанное становится карточкой на 5 минут и открывается кликом; автозапуск включается явно полями политики `voice_commands` / `voice_commands_autorun` (`optional bool`, поэтому старый клиент не выключает их молчанием). Шестое ambient-событие `ambient.voice_command` несёт только `command_id`, `kind`, `app_id` и состояние; заголовок приложения читается командой `ListVoiceCommands` (127), решение — `ResolveVoiceCommand` (128). Тот же каталог доступен агенту инструментами `app.open` и `app.list`. Job object супервизора получил `JOB_OBJECT_LIMIT_BREAKAWAY_OK`, поэтому открытое приложение переживает перезапуск Core, а дерево самого Core по-прежнему умирает вместе с job;
- upgrade smoke в CI, автоматический rollback, post-restart health handshake и recovery незавершённой транзакции перед запуском Core;
- один постоянный релиз `installer` с описанием из `installer/release-notes.md`: `EvoHime-Setup.exe` и `EvoHime-Setup.json` в нём перезаписываются после успешного CI на `main`, новых релизов и версионных тегов не создаётся. Установщик нужен для первой установки и фоновых обновлений клиента;
- второй постоянный релиз `listener-runtime` с описанием из `installer/listener-runtime-notes.md` — набор движка распознавания: `whisper.dll` с зависимостями, ступени `ggml-small/base/tiny.bin` и манифест `listener-runtime.json`. Собирается воспроизводимо `scripts/build-listener-runtime.ps1`: коммит whisper.cpp закреплён, раскладка `whisper_full_params` сверяется на сборке пробником против тех же заголовков, из которых собрана DLL, модели проверяются по SHA-1 апстрима, а готовый каталог проверяется примером `verify-runtime` — тем же кодом `tools_dir::load`, которым его проверит листенер. DLL собирается без нативных оптимизаций и со статическим CRT, поэтому не требует VC++ Redistributable. Публикация — ручной запуск `.github/workflows/listener-runtime.yml`; автоматически на push и PR идёт только гейт сборки со ступенью `tiny`, потому что заливать ~700 МБ на каждый коммит незачем;
- имя агента «Ева» передаётся в system context Core;
- Core-owned build policy и её хранение;
- durable recovery foundation для длительных запусков и reconciliation;
- `run_policy` — неизменяемый snapshot бюджета запуска (итерации, wall clock, tool calls, токены, стоимость); Core проверяет его перед каждым эффектом, renderer может только показать значения;
- `pulse` в supervisor — контракт локального digest расписаний: dead-letter даёт `Failed`, пропуски и ошибки — `Degraded`, успех не подменяет отказ. Модуль пока не подключён к supervisor loop; пользовательский статус Pulse выводится в `OperationsPanel` из событий расписаний.

### Безопасность и данные

- Core-first SQLite backup/restore: Online Backup API, WAL checkpoint, DPAPI payload protection, checksum, preview, approval, progress, safety backup, rollback и redacted audit; долгая операция отменяется командой `CancelDatabaseOperation`;
- diagnostic bundle `v1` собирается только main-процессом и ограничен четырьмя логами, хвостом 64 КБ каждого, 120 строками и 512 КБ сериализованного результата; recovery projection индексирует terminal tasks за один проход и показывает только bounded allowlisted details;
- filesystem.search исключает hard-default secret/auth paths, не следует symlink/reparse-обходам и не требует POSIX shell;
- shell blocklist расширен для Windows launcher/LOLBin семейств; recovery timeline различает `RECOVERING`, `BLOCKED`, `WAITING_APPROVAL` и `FAILED`;
- ключ провайдера хранится main-процессом Electron: значение шифруется OS (`safeStorage`, DPAPI на Windows) и лежит в `%LOCALAPPDATA%\EvoHime\shell\provider.json` с режимом `600`; размер и формат сохранённого ciphertext проверяются, запись идёт через flush/atomic rename с очисткой временного файла при ошибке. Профили LiteRouter, OpenAI Compatible и Responses API раздельны: renderer получает только summary «ключ задан/не задан» для выбранного профиля, а Core — переменные окружения только выбранного провайдера через supervisor. Смена ключа перезапускает supervisor и Core;
- панель `ChatGPT + Codex CLI` показывает лимит и модель локального Codex app-server без API-ключа. Если CLI отсутствует, пользовательская кнопка устанавливает точный пакет `OpenAI.Codex` через winget; кнопка `Войти через ChatGPT` открывает интерактивный `codex login`, а `Обновить` проверяет результат через app-server;
- в композере выбран единый режим выполнения задачи: LiteRouter, настроенный OpenAI API или Codex CLI; рядом показывается каталог моделей выбранного режима, а Codex передаётся как явный coding intent через additive IPC-поле. Отдельный coding-чекбокс удалён, обычный диалог не переключается молча на другой backend;
- каталог моделей отдаёт не только идентификаторы, но и лимиты (`context_length`, `max_completion_tokens`), которые Core сохраняет в таблицу `model_context_limits` (схема 20). Планировщик контекста берёт из неё реальное окно модели: пока провайдер не спрошен, действует встроенный профиль, а расхождение решается в пользу провайдера;
- вкладка «Ревью планов» принимает Markdown до 512 КБ — одним файлом или несколькими сразу (мультивыбор в диалоге и drag&drop в панель, файлы склеиваются в нумерованные разделы), запускает 2–8 последовательных read-only reviewer calls (по одному запросу за раз, чтобы не упираться в лимиты провайдера) и отдельную synthesis call; ответ каждого рецензента ограничен 256 КБ. Состав и порядок рецензентов сохраняются при неудачном обновлении каталога моделей, итог копируется в буфер или экспортируется в Markdown, а `ClearPlanReviewHistory` очищает историю и в Core, и в UI сразу;
- кнопка «Исправить план по ревью» в той же вкладке переписывает план по замечаниям одним вызовом synthesis-модели (`RevisePlan`) и показывает результат целиком; сохранение — отдельное действие (`SaveRevisedPlan`, только `.md`), замена исходного файла подтверждается вторым нажатием, а ответ короче половины исходного помечается как вероятный обрыв генерации. Правка доступна, когда в списке ровно один файл и это тот же файл, по которому сделано ревью. Рецензент и редактор видят соседние планы, на которые проверяемый ссылается: ядро читает их само по пути исходного файла, промпт правки требует минимальных изменений и запрещает ослаблять инварианты приложенных планов, а карточка показывает, с чем сверялась правка, и предупреждает не только об обрыве ответа, но и о вдвое раздутом плане. Перевод строки исправленного файла приводится к исходному, чтобы правка не выглядела в git полной перезаписью;
- base URL провайдера принимается только по `https`, либо `http` на loopback, чтобы ключ не ушёл на произвольный хост.
- approval.required передаёт bounded structured preview для команд, записи файлов и unified diff; Electron показывает его в `TaskTimeline`, а Core сохраняет exact-call hash и повторную policy-проверку перед выполнением;
- approval-токены для tool runtime одноразовые и атомарно погашаются перед выполнением; hard-deny policy проверяет канонический путь, включая вызовы через относительные алиасы;
- Runtime receipts 01.3 подключены к Core-owned execution path: durable signed pre/post/refusal, UUIDv7 approval intent с monotonic TTL, exact-call recheck, signed refusal для expired/stale/call_changed/policy_denied, bounded parent approval reference, recovery/quarantine/reconciliation и audit call hash; JCS numeric edge cases покрыты shared Rust tests;
- План 09 добавил Core-owned `CapabilitySnapshotV1` с canonical domain-separated hash, parent/child subset, bounded budgets и redacted summary. `PolicyGate` выполняет preflight и recheck непосредственно перед effect для ToolAgent, terminal IPC и workflow adapters; action/approval durable linkage хранит session, snapshot, policy и hook versions. Typed policy outcomes сохраняются в additive `receipt_policy_decisions`, поэтому старые receipt CHECK-значения остаются совместимыми;
- Production approval claim идёт только через durable `claim_approval_checked` с session/snapshot/policy binding. Явной отказ переводит intent в `denied`, action — в `refused`; повторный execute не создаёт side effect. Bounded Core hooks видят hashes и typed outcome metadata, но не raw input или секреты;
- Receipt chain storage и export 01.4: durable `receipts_v1` в SQLite с `previous_receipt_hash`-цепочкой, подписанные checkpoints и retention/compaction (`verified_pruned` вместо тихого удаления), chain-aware проверка ключевых границ и pre/terminal-пар, атомарный JSONL export bundle с манифестом и chain-aware offline `evohime-verify.exe`. Core отдаёт это командами `ListReceipts`, `VerifyReceipts` и `ExportReceipts`, Electron main-процесс проксирует их (`core.listReceipts`, `core.verifyReceipts`, `core.exportReceipts`); пользовательской поверхности в renderer нет — панель безопасности из UI убрана, поэтому цепочка доступна только через IPC и offline-верификатор. Схемы и векторы: `contracts/receipts/v1/`;

### Model gateway и routing

- В `crates/model-gateway` существуют bounded `RouteCandidate`, детерминированный
  selector и runtime-режимы `LocalFirst`/`Offline`; provider contract также
  содержит типы capability metadata, policy snapshot, health overlay, retry и
  trace.
- Этот код подключён к `ToolAgent` и desktop IPC. Snapshot/overlay selector
  выполняет закрытые проверки privacy, offline, classification, capability,
  context, cost, latency, evaluation gate и health/circuit в воспроизводимом
  порядке. Fallback сохраняет policy hash, snapshot hash и `now_ms` попытки.
- Local provider ограничен loopback; supervisor выдаёт TTL-grant, запускает
  adapter только из supervisor-owned configuration, держит его в отдельном
  Job Object и обслуживает authenticated Core→supervisor pipe с bounded
  `launch`/`stop`/`probe` командами.
- Core имеет реальный pending approval registry с явным resolve, timeout,
  cancellation и лимитом reroute; renderer показывает только typed redacted
  trace и отдельные `unknown_state`/`core_unavailable` состояния.
- Evaluation catalog проверяется по подписи до загрузки, поставляется как
  runtime resource и обновляется через validated temporary file + atomic
  replacement. Этот раздел является подтверждённым состоянием checkout.

### Desktop shell (Electron)

- визуальная переработка Electron UI по направлению 20 завершена: тёмная
  палитра и токены вынесены в renderer stylesheet, sidebar показывает проекты
  и чаты, глобальные разделы и настройки доступны из меню пользователя,
  открывающегося вверх; добавлены клавиатурное закрытие по Escape, закрытие по
  клику вне меню, active/hover/focus-состояния и адаптивные отступы. Изменения
  не затрагивают IPC, Core-owned state, project/chat stores, approvals,
  recovery, updates или настройки компонентов;
- migration acceptance закрыта на Windows: UI-срезы, authenticated Core IPC, package startup, fault recovery, install/upgrade/rollback и acceptance matrix;
- левая панель — проекты и чаты (`ProjectSidebar`); меню пользователя внизу открывает глобальные разделы и настройки вверх. Имя пользователя берётся из GitHub CLI, `git config user.name` или учётной записи Windows и подписывается источником;
- главный экран (`HomeScreen`) вместо заглушки: чат создаётся сам при первом запросе;
- ход задачи свёрнут в читаемую ленту (`ActivityLine`, `transcript.ts`), инструменты подписаны по-русски (`tool-names.ts`), ответы агента отображаются как Markdown (`MarkdownMessage`);
- строка репозитория над композером (`RepositoryBar`): ветка и счётчики изменений;
- выбор модели в чате (`ModelPicker`) с разделением каталога на free/paid; выбор применяется без перезапуска Core через IPC `SelectModelRequest`;
- настройки API-провайдера и Codex собраны в один блок с внутренними вкладками; отдельный `WorkspacePicker` убран — папка выбирается из панели проектов;
- `RecoveryBanner` показывает подтверждённое Core состояние восстановления;
- после перезапуска Core durable build-effect и обычные agent-run проходят lease heartbeat; подтверждённый terminal event приходит в UI как `RESUMABLE`, а неподтверждённый результат остаётся `BLOCKED`; текущая общая storage schema — v32;
- `OperationsPanel` («Память и Pulse») — очередь подтверждения памяти и конфликты (только metadata, с действиями «сохранить»/«отклонить»/«заменить»), typed read-only projection child workflow (timeline, role/state/revision/budget, lease и dead-letter) и schedule-событий, а также управление локальным индексом workspace: status, update/rebuild/cancel, optional embeddings и bounded search;
- пользовательский self-repair: после трёх ошибок задач `OperationsPanel` показывает bounded digest и кнопку «Починить». Main сохраняет FSM в `shell\\repair.json`, создаёт изолированную копию канонического GitHub-репозитория, запускает через Core отдельные diagnose/patch, commit и push операции, показывает diff/tests/CI и не запускает эти шаги без клика пользователя;
- repair защищает `.codex`, `AGENTS.md`, workflows, updater, supervisor, receipt, security и `.env*`; transaction worker держит backup до authenticated Core startup новой версии и откатывает установку при health timeout;
- self-repair хранит bounded projection в `%LOCALAPPDATA%\EvoHime\shell\repair.json`, checkout — в `%LOCALAPPDATA%\EvoHime\repair\<repair-id>`. После закрытия Евы активный run переводится в recoverable failure; накопленный digest не запускает работу сам;
- specialized child workflows реализованы в Core: versioned typed request/report, correlation и atomic per-parent sequence, schema/size/provenance/grant revalidation, coordinator state machine, monotonic lease recovery, bounded transport retry/revision, durable checkpoint/dead-letter retention и deterministic fan-in. Context allowlist и ArtifactStore offload/read policy не допускают произвольный child context или raw Sensitive/Secret output; audit rejection и trace projection разделены;
- Context Budget Manager: перед каждым model call Core собирает контекст под bounded budget вместо отправки всего накопленного диалога. Профиль модели, обязательный минимум, конечная лестница сокращения, детерминированный `content_hash` и immutable `context_ledger` с hash реализованы в `crates/context-budget`; ledger, scratchpad, content-addressed artifact store и RAG generation storage живут в SQLite (schema v19). Отказ сборки доходит до UI как `BudgetUnavailable` с кодом и стадией, а не как молчаливый обрыв. Сжатие истории выполняет отдельный bounded вызов model gateway с deterministic fallback, tool schemas ограничены loadout детерминированного intent router, а вызов вне loadout отклоняется до эффекта. Событие `ModelContext` получило additive projection состава и причин сокращения; добавлены команды просмотра ledger и scratchpad, `summarize now`, очистки scratchpad, `pin/unpin item` и чтения артефакта. См. `docs/architecture.md`;
- Local Agentic RAG: Core выполняет bounded canonical workspace scan, versioned chunking, atomic generation publication и incremental reuse в SQLite FTS5. Deterministic planner/checker loop возвращает validated evidence с score explanation и uncertainty; optional локальные embeddings публикуются отдельным поколением и дают RRF `k=60` с автоматическим FTS5 fallback. Citations version 1 проходят первичную и финальную re-read проверку, metadata-only `rag_context_ledger` не хранит text/raw output, а stale evidence не может подтвердить финальный ответ или Memory Extraction candidate. IPC и UI поддерживают index/rebuild/cancel/search/status; evaluation fixtures лежат в `tests/evals/fixtures/workspace-rag/`. См. `docs/architecture.md`;
- Memory Extraction: Core извлекает кандидатов в память из диалога после явного триггера пользователя (`strict` по умолчанию, `EVOHIME_MEMORY_EXTRACTION`), прогоняет их через версионируемый policy gate и сохраняет как `pending_confirmation`, пока пользователь не подтвердит. Активной памятью без approval может стать только low-risk предпочтение, произнесённое пользователем; секреты не сохраняются, конфликт не разрешается автоматически. См. `docs/architecture.md`;
- чаты shell хранятся в `%LOCALAPPDATA%\EvoHime\shell\chats.json` с ограничениями (100 чатов на workspace, 500 сообщений на чат). Это UI-группировка, а не состояние агента: Core остаётся владельцем задач и заново проверяет каждую команду.

### Инструменты агента

Read-only Git loadout расширен операциями git.log, git.show, git.blame и git.changed_files. Они зарегистрированы в Core ToolRegistry, входят в модельные схемы и read-only resilience/policy-контуры; история ограничена 100 записями, blame — 500 строками, а revision и пути проходят валидацию до запуска Git.

### Model request provenance

- canonical request envelope v1, JCS/domain-separated hashes, retry lineage и
  известный test vector находятся в `contracts/model-request/v1/` и
  `crates/evohime-model-provenance`;
- перед каждым dispatch Core сохраняет immutable ledger, opaque block refs,
  request receipt, shadow metadata и dispatch marker; ответ и tool intent
  привязаны к конкретному request attempt;
- startup recovery классифицирует crash до marker как `interrupted`, а после
  marker — как `unknown_outcome`, без blind retry; retention запускается при
  старте и далее bounded worker-ом, redaction оставляет typed tombstone;
- export создаёт атомарный `evohime-provenance-export-v1` с ledger, route/policy,
  responses, intents, evidence, shadow и tombstone секциями. Offline verifier
  проверяет canonical manifest, размеры, hashes, detached Ed25519 signature и
  внешний trust key. Намеренно неполные `redacted`, `retention_pruned`,
  `legacy_hash_only` и `metadata_hash_only` не считаются полной реконструкцией.

### Workflow orchestration

- контракт `workflow/v1` (`crates/evohime-core/src/workflow.rs`): immutable
  граф, action profiles `child`/`mcp_tool`/`context_provider` рядом с прежними
  `research`/`transform`/`tool`/`condition`/`approval`/`subgraph`/`loop`,
  block identity с версией, acceptance contract, allowlist маршрутов, явные
  failure-ветви, canonical JSON и SHA-256 hash. Идентичности ограничены
  charset `[a-z0-9._:-]`, поэтому URL, путь или команда в них не помещаются;
- Core-owned реестр (`workflow_registry.rs`): каталог блоков с test fixtures,
  MCP-серверы с транспортом/endpoint/allowlist, read-only контекстные
  провайдеры, допущенные инструменты и Core-owned подграфы. Отклоняются
  неизвестный блок и сервер, несовпадение версии или схемы блока, инструмент
  вне allowlist, `transport_unavailable`, host вне
  `EVOHIME_MCP_ALLOWED_HOSTS`, превышение бюджета провайдера и эскалация
  grants/бюджета/контекста child;
- библиотека шаблонов (`workflow_templates.rs`): `repository-research`,
  `plan-implement-review`, `parallel-security-review`. Подстановка входов идёт
  только в свободный текст; шаблон с обязательным approval помечен
  `schedule_eligibility = unavailable`, остальные — `interval_only`;
- durable runtime (`workflow_runtime.rs`, storage schema 29): таблицы
  `workflow_runs`, `workflow_run_nodes`, `workflow_node_attempts`,
  `workflow_run_events`, lease, dispatch marker до эффекта, восстановление в
  `unknown_outcome`/`interrupted` без слепого повтора, bounded fan-out/fan-in,
  явные failure-ветви, `dead_letter` для исчерпанных повторов, `degraded` для
  недоступного источника и монотонные durable-события;
- адаптеры (`workflow_adapters.rs`) ведут узлы в существующие контуры Core:
  typed child request, `ToolRegistry` (включая Core-owned `mcp.call`),
  read-only контекстные источники и deterministic-операции;
- IPC: additive `ListWorkflowTemplates`, `GetWorkflowDefinition`,
  `StartWorkflow`, `GetWorkflowRun`, `CancelWorkflow`, `ListWorkflowEvents`.
  Approval узла решается существующей `ResolveApproval`;
- Electron: раздел «Составные задачи» (`WorkflowPanel`) показывает шаблоны,
  входы, состояния узлов, зависимости, попытки и события. Renderer не считает
  зависимости и не запускает узлы; prompt, цель child и сырой вывод в него не
  приходят;
- матрица приёмки 06-4 проверена deterministic evals в
  `crates/evohime-core/src/evals.rs` (12 новых случаев в существующих
  категориях), unit-тестами runtime/хранилища и real-Core E2E, запускающим один
  шаблон против собранного `evohime-core.exe`.

### Automation contract

- Core владеет versioned `automation/v1` definition (`automation.rs`):
  immutable definition/revision/owner scope, bounded activity references,
  capability and approval policy, schedule/manual trigger policy, concurrency,
  retry and retention limits. Unknown contract versions and unsafe limits fail
  closed before persistence; the definition hash is stable for the same
  serialized contract.
- `TriggerRequestV1` requires bounded correlation and idempotency identity,
  scheduled slot and input payload. `AutomationRunV1` binds the immutable
  definition hash, permission snapshot, approval snapshot and generation to a
  typed lifecycle; `ActivityEventV1` and `AutomationHealthV1` are bounded
  projection contracts and contain diagnostics rather than provider output.
- `automation_store` is installed on the shared SQLite connection. Definitions
  are keyed by `(definition_id, revision, owner_scope)`, while runs enforce
  `(owner_scope, definition_id, revision, idempotency_key)` uniqueness. Repeated
  delivery with the same payload returns the first run; a different payload
  returns typed `IdempotencyConflict` without creating a second run.
- The durable contract, scheduler, queue ownership, lease/fencing and simulation
  guards are implemented in the Core-owned runtime. Renderer has no scheduler
  or execution authority.

### Automation runtime

- `automation_runtime.rs` owns the automation FSM, bounded command queue
  (256), coalesced progress map (1024), fencing generation, 30-second lease,
  operation lock and fail-closed effect revalidation. Terminal states cannot be
  transitioned, and stale generations cannot publish transitions.
- `automation_store` persists `automation_run_events` and `automation_leases`.
  A guarded state update and event append share one SQLite transaction;
  takeover is possible only after expiry and uses a higher generation. Provider
  operations have a 120-second deadline, cooperative cancellation and bounded
  retry classification.

### Automation snapshots and simulation

- `AutomationSnapshotV1` is a bounded schema-1 record with checksum,
  definition revision, fencing generation, event sequence, policy/approval
  snapshots and provenance. Validation rejects corrupt, oversized, stale and
  incompatible snapshots; `automation_snapshots` keeps them separate from
  authoritative active state and event history.
- `ReplayInputV1` produces a deterministic replay hash from frozen clock, RNG
  seed, normalized inputs, ordered events, provider fixtures and capability /
  policy snapshots. Simulation admits only the fake-provider effect and
  rejects filesystem, network, process, shell, registry, clipboard and
  production IPC effects; export redaction strips bearer markers and absolute
  Windows paths.

### Automation acceptance

- Deterministic A01–A08 fixtures in `automation_acceptance.rs` verify bounded
  trigger/queue behavior, stale lease fencing, cancellation and retry typing,
  snapshot/replay equality, simulation redaction, history limits and
  effect-boundary revalidation.
- The acceptance fixtures report Core/storage contract evidence. Scheduler
  timezone/missed-tick behavior is wired through the durable cursor poller,
  additive automation IPC is projected by Electron, and both are covered by
  the automation boundary/release evidence gates; optional adapters 13–15
  continue to fail closed as unsupported.

### Release decisions

`docs/decision-register.md` фиксирует dependency graph, владельцев schema/IPC,
resource budgets и закрытые release decisions. В частности,
оно фиксирует закрытые scheduler/IPC, archive/restore и license inventory
решения; code signing принят как внеобъёмное решение текущего цикла.
Automation boundary gate (`scripts/automation-release-gate.tests.ps1`) проверяет
наличие contract modules, отсутствие filesystem/network/process imports в них,
locked Core/storage acceptance tests и подключён к Windows Rust CI.
Release evidence (`docs/release-evidence.md`) фиксирует rollback/disable,
retention, redaction, privacy/egress и license ownership; local
`scripts/release-evidence.tests.ps1` проверяет документы, backup/restore и
automation evidence без публикации credentials.
Финальный audit (`docs/release-evidence.md`) оставляет checkout в статусе
`TECHNICAL_GATES_PASS / RELEASE_GREEN`; manifest/hash остаётся trust root.

### Разработка

- `.env.example` описывает переменные провайдера для локального запуска; `start-dev.ps1` читает `.env` по allow-list и передаёт значения только дочерним native-процессам.
- deterministic evaluation catalog и security smoke gates находятся в `tests/evals/` и запускаются из `scripts/eval-gate.tests.ps1` и `scripts/security-eval-gate.tests.ps1`; redacted CI summary сохраняется в `artifacts/eval-gate/summary.jsonl`.

## Последняя проверка checkout

26 августа 2026 года `scripts/final-release-audit.tests.ps1` и полный локальный
прогон подтвердили `TECHNICAL_GATES_PASS / RELEASE_GREEN`:

- workspace Rust tests и строгий `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` прошли;
- Electron `npm test`: 457 passed, 2 штатно skipped; protocol, typecheck,
  production build и bundle checks прошли;
- C# compatibility tests, native-package smoke, automation/release evidence и
  Windows release gates прошли в доступной проверке;
- `cargo fmt --all -- --check` и `git diff --check` прошли.

Package startup, installer, upgrade/rollback и Windows compatibility остаются
отдельными CI gates из `.github/workflows/windows.yml`. Source-update E2E
запускается только с `EVOHIME_UPDATE_E2E=1`, поскольку выполняет реальную
пересборку.

## Следующие направления

План 22 закрыт: diagnostics/recovery hardening, защита credential persistence и
documentation/release gate реализованы и перенесены в этот документ и
`architecture.md`. Дальнейшие изменения оформляются новым временным планом
только после отдельного evidence review; до этого поддерживаются текущие
Windows compatibility, package/installer и release gates.

## Граница продукта

Пользовательский продукт ограничен `EvoHime-Setup.exe`, `EvoHime.exe`, локальным Core, supervisor и данными в профиле Windows. Исследовательские и экспериментальные каталоги не входят в установочный runtime.

Legacy web UI, HTTP server, browser launcher и PostgreSQL migrations удалены из репозитория. Electron UI и authenticated versioned named-pipe IPC — текущая пользовательская оболочка и transport boundary; WinUI остаётся временным compatibility runtime для совместимости и тестов.
