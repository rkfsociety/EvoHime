# EvoHime — текущее состояние

Обновлено: 2026-08-22 (планы 04 и 05 завершены; план 06 реализован в текущем
checkout: контракт workflow/v1, Core-owned реестр и шаблоны, durable runtime
со схемой 29, additive IPC и раздел «Составные задачи» в Electron).

## Продукт

EvoHime — локальный Windows-клиент для coding-agent задач. Пользовательское имя агента — **Ева**. Пользовательские версионные релизы для текущего цикла не создаются; установленная сборка определяется коммитом и веткой в `evohime.build.json`.

Пользователь получает один `EvoHime-Setup.exe`. После установки на рабочем столе появляется один ярлык `EvoHime`, запускающий `EvoHime.exe`.

## Runtime

- `EvoHime.exe` — Electron main process с bundled renderer; native package и installer собирают Electron shell;
- `evohime-core.exe` — Rust agent loop, model gateway, tools, permissions, approvals и SQLite;
- `evohime-supervisor.exe` — single-instance mutex, Job Object, restart и диагностика;
- `evohime-transaction.exe` — скрытый transaction worker для backup, commit и rollback обновлений, включая подмену CI-установщика или локально пересобранного пакета;
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
- streamed task timeline, cancellation и approval round-trip;
- Windows package smoke tests и Windows CI;
- единый Inno Setup installer с одним desktop shortcut; установленный клиент сам поднимает supervisor, а supervisor — Core;
- фоновое обновление из постоянного GitHub Release: клиент сверяет зелёный commit, скачивает `EvoHime-Setup.exe` только при совпадении манифеста и SHA-256, а затем отдаёт его `evohime-transaction.exe --installer`. Запуск приложения не блокируется скачиванием; после фоновой загрузки UI показывает баннер с предложением перезапуска. Локальная пересборка через `launchPolicy: "build"` сохранена для разработки;
- ambient listener 04.3 реализован: supervisor запускает `evohime-listener.exe` в отдельном bounded Job Object с независимым restart budget; Core и listener используют разные owner-only pipe endpoints и роль `listener` с nonce/HMAC. Аудио-крейт содержит cpal shared capture, bounded in-memory ring, deterministic 32/48→16 kHz decimation, energy VAD и fixture segmentation; privacy-gate запрещает filesystem I/O в аудио-крейте и включён в `rust-native` CI;
- движок распознавания 04.4 реализован: whisper.cpp грузится через `libloading` из каталога инструментов, выбранного резолвером `EVOHIME_LISTENER_TOOLS_DIR` → `EVOHIME_TOOLS_DIR` → `%LOCALAPPDATA%\EvoHime\tools\listener`, каждый файл сверяется с SHA-256 манифеста, необъявленная DLL рядом блокирует загрузку, раскладка ABI проверяется до первого вызова. Подпись требуется только у `onnxruntime.dll`; неподписанный собственный `whisper.dll` — штатное состояние, закреплённое тестом. Листенер открывает микрофон, сегментирует речь, дедуплицирует повторы (NFKC-нормализация, окно 60 с, near-dup ≥ 0.9) и спускается по лестнице `small → base → tiny` при RTF > 0.5 пять раз подряд, после чего останавливается с причиной `engine_degraded`. Транскрипты доходят до `ambient_utterances` с реальными языком, длительностью и порядковым номером, а Electron скачивает и проверяет набор рантайма (`listener-runtime.ts`) с показом состояния на вкладке «Распознавание речи»;
- контроль и UI слушания 04.5 реализованы: девять additive-команд протокола (теги 107–115) — включение/пауза/выбор устройства, статус, список эпизодов, текст одного эпизода, удаление транскриптов, «забыть окно», чтение и сохранение политики, решение по предложению. Состояние живёт в одном месте — `AmbientListeningRegistry` в `evohime-core`; трей, глобальный хоткей `Ctrl+Alt+M` и вкладка «Слух» отправляют одну и ту же команду и обновляются только событием `ambient.state`. Занятая комбинация хоткея объявляется недоступной, а не молча отключается; при отсутствии ответа за 5 с индикатор показывает «проверка состояния», а не «выключено». Удаление требует подтверждения и в модальном диалоге, и в ядре (`confirmed=true`). Текст высказываний отдаётся только `GetAmbientEpisode` по явному клику. Листенер перечисляет устройства, переключает их без перезапуска и подписан на `WM_DEVICECHANGE`; тихие часы политики закрывают поток по локальным часам. Отдельное разрешение `microphone_listen` переключается в новой панели безопасности и не затрагивается сменой общего режима доступа;
- мост ambient в память 04.6 реализован: `SourceTrust::Ambient` — пятое значение доверия со строго более жёсткой policy; `evaluate` возвращает `pending` с причиной `ambient_never_auto_confirms` раньше любых порогов, поэтому услышанное не может стать активной памятью без клика пользователя. Триггер — закрытие эпизода (начало следующего, минута тишины, разрыв связи с листенером), вход отдельный от диалогового, чтобы не подделывать `user_asserted`. Свой гейт запуска и свои бюджеты: `EVOHIME_AMBIENT_MEMORY` (`off` | `pending`, мусор — `off`), 6 кандидатов и 12 эпизодов в час, отдельный лимит токенов, причины `ambient_candidate_limit`/`ambient_episode_limit`; общий выключатель `EVOHIME_MEMORY_EXTRACTION=disabled` старше частного. Из речи принимаются только `preference`, `entity` и `lesson`; утверждение не в первом лице поднимает `privacy_class` до `sensitive` и скрывает тело. `provenance_source_id` кандидата — это `episode_id`, поэтому удаление эпизода отклоняет его кандидатов причиной `source_deleted`. В `OperationsPanel` — бейдж «услышано», подпись «говорящий не подтверждён» и фильтр по источнику; диалоговый `check_can_extract` не ослаблен ни в одной ветке;
- ограниченная проактивность 04.7 реализована, планом 04 закрыт: по услышанному Ева производит ровно два эффекта — карточку-предложение и неисполняемое напоминание, а `StartTask`, `ToolCall`, `FileWrite` и `NetworkRequest` отклоняются `authorize_proactive` до любого эффекта и покрыты негативными тестами. Источник предложений — те самые `constraint` и `decision`, которые 04.6 отказывается делать памятью. Дедупликация идёт по `proposal_key` (вид + тема + округлённый час) под `UNIQUE`, постоянный mute — по `mute_key` без времени, поэтому он переживает и смену временной корзины, и рестарт Core. Потолок из 04.1 (3 в час, 10 в сутки, не чаще одного раз в 10 минут, плюс пауза и тихие часы) неизменяем, а счётчики персистятся строкой схемы v26; превышение отбрасывает предложение, а не копит очередь, и дубликат бюджета не тратит. Схема v26 additive поверх v25: `ambient_proposals`, `ambient_proposal_mutes`, `ambient_proactivity_counters`. Удаление эпизода переводит его предложения в `expired` с причиной `source_deleted` **до** удаления строки эпизода. Durable-событие `ambient.proposal` несёт только `proposal_id`, `episode_id`, `kind`, bounded `subject_key` и состояние и уходит вместе с эпизодом; человекочитаемый текст карточки приходит командой `ListAmbientProposals` (тег 116). Решение несёт обязательный `idempotency_key`, принятое создаёт `work_items` в статусе `backlog` с `source_ref = episode_id`, а 24 часа молчания переводят карточку в `expired`;
- upgrade smoke в CI, автоматический rollback и recovery незавершённой транзакции перед запуском Core;
- один постоянный релиз `installer` с описанием из `installer/release-notes.md`: `EvoHime-Setup.exe` и `EvoHime-Setup.json` в нём перезаписываются после успешного CI на `main`, новых релизов и версионных тегов не создаётся. Установщик нужен для первой установки и фоновых обновлений клиента;
- второй постоянный релиз `listener-runtime` с описанием из `installer/listener-runtime-notes.md` — набор движка распознавания: `whisper.dll` с зависимостями, ступени `ggml-small/base/tiny.bin` и манифест `listener-runtime.json`. Собирается воспроизводимо `scripts/build-listener-runtime.ps1`: коммит whisper.cpp закреплён, раскладка `whisper_full_params` сверяется на сборке пробником против тех же заголовков, из которых собрана DLL, модели проверяются по SHA-1 апстрима, а готовый каталог проверяется примером `verify-runtime` — тем же кодом `tools_dir::load`, которым его проверит листенер. DLL собирается без нативных оптимизаций и со статическим CRT, поэтому не требует VC++ Redistributable. Публикация — ручной запуск `.github/workflows/listener-runtime.yml`; автоматически на push и PR идёт только гейт сборки со ступенью `tiny`, потому что заливать ~700 МБ на каждый коммит незачем;
- имя агента «Ева» передаётся в system context Core;
- Core-owned build policy и её хранение;
- durable recovery foundation для длительных запусков и reconciliation;
- `run_policy` — неизменяемый snapshot бюджета запуска (итерации, wall clock, tool calls, токены, стоимость); Core проверяет его перед каждым эффектом, renderer может только показать значения;
- `pulse` в supervisor — контракт локального digest расписаний: dead-letter даёт `Failed`, пропуски и ошибки — `Degraded`, успех не подменяет отказ. Модуль пока не подключён к supervisor loop; пользовательский статус Pulse выводится в `OperationsPanel` из событий расписаний.

### Безопасность и данные

- Core-first SQLite backup/restore: Online Backup API, WAL checkpoint, DPAPI payload protection, checksum, preview, approval, progress, safety backup, rollback и redacted audit; долгая операция отменяется командой `CancelDatabaseOperation`;
- filesystem.search исключает hard-default secret/auth paths, не следует symlink/reparse-обходам и не требует POSIX shell;
- shell blocklist расширен для Windows launcher/LOLBin семейств; recovery timeline различает `RECOVERING`, `BLOCKED`, `WAITING_APPROVAL` и `FAILED`;
- ключ провайдера хранится main-процессом Electron: значение шифруется OS (`safeStorage`, DPAPI на Windows) и лежит в `%LOCALAPPDATA%\EvoHime\shell\provider.json` с режимом `600`. Renderer получает только summary «ключ задан/не задан», а Core — переменные окружения выбранного провайдера через supervisor. Смена ключа перезапускает supervisor и Core;
- каталог моделей отдаёт не только идентификаторы, но и лимиты (`context_length`, `max_completion_tokens`), которые Core сохраняет в таблицу `model_context_limits` (схема 20). Планировщик контекста берёт из неё реальное окно модели: пока провайдер не спрошен, действует встроенный профиль, а расхождение решается в пользу провайдера;
- вкладка «Ревью планов» принимает Markdown до 512 КБ — одним файлом или несколькими сразу (мультивыбор в диалоге и drag&drop в панель, файлы склеиваются в нумерованные разделы), запускает 2–8 последовательных read-only reviewer calls (по одному запросу за раз, чтобы не упираться в лимиты провайдера) и отдельную synthesis call; ответ каждого рецензента ограничен 256 КБ. Состав и порядок рецензентов сохраняются при неудачном обновлении каталога моделей, итог копируется в буфер или экспортируется в Markdown, а `ClearPlanReviewHistory` очищает историю и в Core, и в UI сразу;
- кнопка «Исправить план по ревью» в той же вкладке переписывает план по замечаниям одним вызовом synthesis-модели (`RevisePlan`) и показывает результат целиком; сохранение — отдельное действие (`SaveRevisedPlan`, только `.md`), замена исходного файла подтверждается вторым нажатием, а ответ короче половины исходного помечается как вероятный обрыв генерации. Правка доступна, когда в списке ровно один файл и это тот же файл, по которому сделано ревью. Рецензент и редактор видят соседние планы, на которые проверяемый ссылается: ядро читает их само по пути исходного файла, промпт правки требует минимальных изменений и запрещает ослаблять инварианты приложенных планов, а карточка показывает, с чем сверялась правка, и предупреждает не только об обрыве ответа, но и о вдвое раздутом плане. Перевод строки исправленного файла приводится к исходному, чтобы правка не выглядела в git полной перезаписью;
- base URL провайдера принимается только по `https`, либо `http` на loopback, чтобы ключ не ушёл на произвольный хост.
- approval.required передаёт bounded structured preview для команд, записи файлов и unified diff; Electron показывает его в `TaskTimeline`, а Core сохраняет exact-call hash и повторную policy-проверку перед выполнением;
- approval-токены для tool runtime одноразовые и атомарно погашаются перед выполнением; hard-deny policy проверяет канонический путь, включая вызовы через относительные алиасы;
- Runtime receipts 01.3 подключены к Core-owned execution path: durable signed pre/post/refusal, UUIDv7 approval intent с monotonic TTL, exact-call recheck, signed refusal для expired/stale/call_changed/policy_denied, bounded parent approval reference, recovery/quarantine/reconciliation и audit call hash; JCS numeric edge cases покрыты shared Rust tests;
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
  replacement. Все планы 02 реализованы; их временные файлы удалены, а этот
  раздел является подтверждённым состоянием.

### Desktop shell (Electron)

- migration acceptance закрыта на Windows: UI-срезы, authenticated Core IPC, package startup, fault recovery, install/upgrade/rollback и acceptance matrix;
- левая панель — проекты и чаты (`ProjectSidebar`); аккаунт с шестернёй настроек внизу. Имя пользователя берётся из GitHub CLI, `git config user.name` или учётной записи Windows и подписывается источником;
- главный экран (`HomeScreen`) вместо заглушки: чат создаётся сам при первом запросе;
- ход задачи свёрнут в читаемую ленту (`ActivityLine`, `transcript.ts`), инструменты подписаны по-русски (`tool-names.ts`), ответы агента отображаются как Markdown (`MarkdownMessage`);
- строка репозитория над композером (`RepositoryBar`): ветка и счётчики изменений;
- выбор модели в чате (`ModelPicker`) с разделением каталога на free/paid; выбор применяется без перезапуска Core через IPC `SelectModelRequest`;
- настройки провайдера собраны в один блок (`ProviderForm`) вместо прежнего `SettingsPanel`; отдельный `WorkspacePicker` убран — папка выбирается из панели проектов;
- `RecoveryBanner` показывает подтверждённое Core состояние восстановления;
- после перезапуска Core durable build-effect и обычные agent-run проходят lease heartbeat; подтверждённый terminal event приходит в UI как `RESUMABLE`, а неподтверждённый результат остаётся `BLOCKED`; текущая общая storage schema — v29;
- `OperationsPanel` («Память и Pulse») — очередь подтверждения памяти и конфликты (только metadata, с действиями «сохранить»/«отклонить»/«заменить»), typed read-only projection child workflow (timeline, role/state/revision/budget, lease и dead-letter) и schedule-событий, а также управление локальным индексом workspace: status, update/rebuild/cancel, optional embeddings и bounded search;
- specialized child workflows реализованы в Core: versioned typed request/report, correlation и atomic per-parent sequence, schema/size/provenance/grant revalidation, coordinator state machine, monotonic lease recovery, bounded transport retry/revision, durable checkpoint/dead-letter retention и deterministic fan-in. Context allowlist и ArtifactStore offload/read policy не допускают произвольный child context или raw Sensitive/Secret output; audit rejection и trace projection разделены;
- Context Budget Manager: перед каждым model call Core собирает контекст под bounded budget вместо отправки всего накопленного диалога. Профиль модели, обязательный минимум, конечная лестница сокращения, детерминированный `content_hash` и immutable `context_ledger` с hash реализованы в `crates/context-budget`; ledger, scratchpad, content-addressed artifact store и RAG generation storage живут в SQLite (schema v19). Отказ сборки доходит до UI как `BudgetUnavailable` с кодом и стадией, а не как молчаливый обрыв. Сжатие истории выполняет отдельный bounded вызов model gateway с deterministic fallback, tool schemas ограничены loadout детерминированного intent router, а вызов вне loadout отклоняется до эффекта. Событие `ModelContext` получило additive projection состава и причин сокращения; добавлены команды просмотра ledger и scratchpad, `summarize now`, очистки scratchpad, `pin/unpin item` и чтения артефакта. См. `docs/architecture.md`;
- Local Agentic RAG: Core выполняет bounded canonical workspace scan, versioned chunking, atomic generation publication и incremental reuse в SQLite FTS5. Deterministic planner/checker loop возвращает validated evidence с score explanation и uncertainty; optional локальные embeddings публикуются отдельным поколением и дают RRF `k=60` с автоматическим FTS5 fallback. Citations version 1 проходят первичную и финальную re-read проверку, metadata-only `rag_context_ledger` не хранит text/raw output, а stale evidence не может подтвердить финальный ответ или Memory Extraction candidate. IPC и UI поддерживают index/rebuild/cancel/search/status; evaluation fixtures лежат в `tests/evals/fixtures/workspace-rag/`. См. `docs/architecture.md`;
- Memory Extraction: Core извлекает кандидатов в память из диалога после явного триггера пользователя (`strict` по умолчанию, `EVOHIME_MEMORY_EXTRACTION`), прогоняет их через версионируемый policy gate и сохраняет как `pending_confirmation`, пока пользователь не подтвердит. Активной памятью без approval может стать только low-risk предпочтение, произнесённое пользователем; секреты не сохраняются, конфликт не разрешается автоматически. См. `docs/architecture.md`;
- чаты shell хранятся в `%LOCALAPPDATA%\EvoHime\shell\chats.json` с ограничениями (100 чатов на workspace, 500 сообщений на чат). Это UI-группировка, а не состояние агента: Core остаётся владельцем задач и заново проверяет каждую команду.

### Инструменты агента

Read-only Git loadout расширен операциями git.log, git.show, git.blame и git.changed_files. Они зарегистрированы в Core ToolRegistry, входят в модельные схемы и read-only resilience/policy-контуры; история ограничена 100 записями, blame — 500 строками, а revision и пути проходят валидацию до запуска Git.

### Model request provenance 05

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

### Workflow orchestration 06

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

### Разработка

- `.env.example` описывает переменные провайдера для локального запуска; `start-dev.ps1` читает `.env` по allow-list и передаёт значения только дочерним native-процессам.
- deterministic evaluation catalog и security smoke gates находятся в `tests/evals/` и запускаются из `scripts/eval-gate.tests.ps1` и `scripts/security-eval-gate.tests.ps1`; redacted CI summary сохраняется в `artifacts/eval-gate/summary.jsonl`.

## Последняя проверка checkout

22 августа 2026 года пройдены `cargo fmt --all -- --check`,
`cargo check --workspace --all-targets`, `cargo test -p evohime-core -p
evohime-local-storage -p evohime-desktop-ipc` (477 + 146 + 33 тестов),
`cargo check -p evohime-supervisor`, полный Electron-прогон (`check:protocol`,
`typecheck`, 405 тестов при 2 пропущенных, `build`, `check:bundle`), real-Core
E2E с запуском одного workflow-шаблона против собранного `evohime-core.exe`,
`scripts/eval-gate.tests.ps1`, `scripts/security-eval-gate.tests.ps1`,
`scripts/native-package.tests.ps1`, а также C#/WinUI compatibility suites
(24 + 34 теста).

21 августа 2026 года пройдены `cargo test -p evohime-core` (421 unit-тест,
integration/recovery/doc-tests), `cargo test -p evohime-model-provenance -p
evohime-local-storage -p evohime-receipts` (5 + 138 + 56 тестов, один
receipt export test ignored по контракту), а также свежие `cargo check` и
format/diff проверки. 19 августа 2026 года пройдены `cargo check --workspace --all-targets` и полный
Electron-прогон (typecheck и 321 тест). 16 августа 2026 года были пройдены Rust,
Electron, protocol, bundle, deterministic RAG/evaluation и security smoke checks;
C#/WinUI compatibility и native package проверяются полным acceptance-прогоном. Source-update E2E
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
## Provenance model requests

После этапов 05.1–05.9 checkout содержит канонический model-request contract,
SQLite provenance repository в общей schema v29 (internal provenance schema 2),
durable request/response/tool/source/shadow/tombstone tables, Core checkpoint
API, startup recovery/retention hooks и offline bundle boundary. Existing
receipt contract remains backward-compatible; request linkage is additive.

## Tooling 07 — реализовано в текущем checkout

- `tool/manifest/v1` и единый schema catalog находятся в `tool-runtime`; Core
  больше не содержит отдельную таблицу `tool_parameters`. Model loadout
  получает canonical manifest hash, а recovery использует ту же схему.
- Toolkit catalog durable в SQLite поддерживает discover, enable, disable и
  атомарный rollback с audit history; quarantined/unavailable версии не
  включаются.
- Action Console передаёт durable approval decision с idempotency key,
  rejection reason и cancel; Electron показывает grant/reject/cancel и
  восстанавливает состояние из Core event replay.
- MCP model calls принимают только Core identity (`server_id`, `tool_name`),
  endpoint разрешается через `WorkflowRegistry`, а legacy runtime adapter
  получает endpoint только после проверки allowlist/transport/host.
- Tool telemetry сохраняется в EventJournal и экспортируется bounded,
  redacted JSONL; Operations Panel показывает calls, results и approval
  requests. Manifest/hash/policy evals зарегистрированы в deterministic eval
  catalog.

Проверки после реализации: `cargo test -p evohime-core --lib` — 477 passed,
`cargo test -p evohime-tool-runtime --lib` — 119 passed и 1 ignored,
Electron targeted tests — 25 passed, `npm run typecheck` и `git diff --check`
проходят.
