# EvoHime — текущее состояние

Обновлено: 2026-09-02. Планы 01–25 завершены и
удалены из каталога временных планов. План 19.0 добавил только пользовательский
self-repair/self-update цикл; автоматического ремонта, push или перезапуска нет.
Code signing явно исключён из текущего release scope.

Планы 23 и 24 полностью реализованы и закрыты: этапы 23.1 (versioned TaskCheckpoint
contract, provenance validation, immutable storage и schema v32), 23.2 (Core
runtime capture/recovery), 23.3 (IPC/UI projection) и 23.4 (acceptance/release
evidence) реализованы и проверены. Комплект планов 23.0–23.4 удалён после
переноса контракта и подтверждённого состояния в эту документацию.
План 24 добавил Agent Skills v1: bounded deterministic discovery локальных
`SKILL.md`, progressive disclosure metadata/body, fail-closed validation,
capability intersection, typed authenticated IPC tags 139–141, renderer panel
и metadata-only trace без durable хранения тела. Комплект 24.0–24.4 удалён
после проверки полного Rust/Electron контура.
План 25 добавил Persistent Goal v1: Core-owned durable objective/progress
projection с immutable revisions/events, canonical SHA-256 hash, Core-evidence
completion, optimistic version/idempotency, links на workflow/child/checkpoint,
BudgetLimited recovery и schema v33. Authenticated Goal commands используют
tags 142–150, typed projections — oneof 20–22; Verify принимает только
идентификатор manual-критерия, а evidence/verifier mint-ит Core. Ссылки проходят
проверку существования runtime-объекта, список ограничен protobuf budget и
возвращает `projection_truncated`; Electron `GoalPanel` показывает только
bounded projection и посылает явные действия. Recovery не повторяет unknown
effect, а Goal не создаётся автоматически из каждого сообщения.
Комплект планов 25.0–25.4 удалён после полного Rust/Electron verification.

План 26 реализован в текущем checkout: Core владеет `ContinuationPolicyV1`,
решением по typed evidence, durable policy/run/attempt/action/gate storage
(schema v36), task binding, bounded reservations, restart blocking и
idempotent pause/resume/stop. Typed authenticated IPC использует tags 151–156,
а Electron показывает metadata-only `ContinuationPanel`. Tool gates повторно
проверяют capability/approval непосредственно в Core; unavailable и failed
outcomes не превращаются в success. Полный реальный Core IPC E2E также
проверяет handshake, reconnect и workflow command path после boxing oversized
`process_once` future. Дальнейшие workflow/evidence gate adapters остаются
явно unavailable до появления соответствующих Core-owned providers и не
расширяют authority policy.

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
- Agent Skills v1: Core-owned registry с precedence explicit → project-native →
  global → compatibility → bundled, bounded frontmatter/reference reads,
  collision diagnostics, unsafe/secret/executable metadata rejection,
  capability subset validation и process-local cache. Typed `ListSkills`,
  `LoadSkill`, `LoadSkillReference` (139–141) и projections oneof 17–19 не
  переносят тело в generic payload и не записывают его в journal; Electron
  `SkillCatalogPanel` открывает полный документ только после явного действия;
- Skill Trust Pipeline v1 (план 47) реализован поверх registry: offline
  deterministic scanner `skill-scanner-v1` выдаёт стабильные finding codes,
  severity и только SHA-256 fingerprints; trust decision привязан к полному
  content hash и policy/scanner versions. `load`, `load_reference` и
  `effective_permissions` Core-gate-ят только `trusted`/`enabled`, поэтому
  quarantined, review_required, hash mismatch и malformed package не попадают
  в execution selection. Metadata-only projection показывает trust, risk и
  bounded finding count; raw secrets/paths/snippets не сохраняются. Durable
  metadata store `skill_trust_records`/`skill_trust_audit` добавлен миграцией
  schema 48. Contextual review — optional read-only typed adapter и при
  unavailable/malformed результате fail-closed;
- TaskCheckpoint v1: bounded canonical contract с разделением Core-derived и
  model-proposed данных, SHA-256 content hash, immutable parent chain,
  idempotent insert, workspace/event-sequence/state-transition checks, typed
  errors, SQL/JSON metadata consistency и fallback на предыдущую валидную
  запись; TaskCheckpoint был добавлен в schema v32, текущая общая storage
  schema — v33. Core сохраняет checkpoints перед lease,
  compaction/context projection и terminal состояниями, публикует только
  bounded metadata-событие, а recovery блокирует усечённый replay, неизвестный
  outcome и blind retry. Проверки 23.1–23.2: 10 focused storage tests,
  4 focused runtime tests, полный `evohime-local-storage` suite (196 tests),
  полный `evohime-core` suite (553 tests), `cargo check -p evohime-core`,
  `cargo fmt --all -- --check` и `git diff --check`. IPC/UI 23.3 добавляет
  typed `GetTaskCheckpoint`/`ResolveTaskCheckpoint`, bounded projection и
  action result в protobuf, атомарную idempotency запись Core и панель
  `TaskCheckpointPanel`; проверены typed Rust IPC (553 Core tests, 35
  desktop-ipc tests), 2 renderer tests, полный Electron suite (466 tests, 2
  skipped), `npm run check:protocol`, `npm run typecheck`, production build и
  bundle checks. Release audit подтвердил backup/restore, redaction,
  automation и license gates; evidence для закрытия зафиксирован в
  [`release-evidence.md`](release-evidence.md);
- Workspace State Checkpoints v1 добавляет отдельный Core contract для bounded
  file-state capture/compare и conflict-safe restore. Контракт не изменяет
  immutable TaskCheckpoint и исключает `.git`, зависимости, build-кэши и
  symlink/reparse entries; лимиты — 4096 файлов, 64 MiB всего и 1 MiB на файл.
  Storage metadata/journal установлены additive schema v57. Существующий
  `RestoreTaskSnapshot` использует adapter плана 58 и повторяет fingerprints
  перед записью; изменённый пользователем файл возвращает conflict. Additive
  IPC tag 209 и Electron developer panel дают create/compare/restore,
  независимое восстановление task projection и combined restore; результаты
  пишутся в bounded restore journal. Focused проверки: Core 3 tests,
  local-storage 1 test и recovery integration 1 test.
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

- План 42 реализован 31 августа 2026 года: Core-owned `model-resilience-policy-v1`
  добавляет versioned rules, canonical hash, allowlisted profile refs,
  capability/privacy/residency compatibility, bounded retry/fallback metadata,
  normalized provider errors, cancellation и fail-closed unknown outcome.
  Existing gateway routing/retry/provenance остаются единственными authority;
  новая policy state ephemeral, storage schema остаётся v44.
- Authenticated additive IPC использует command 188/event 43. Electron panel
  «Надёжность модели» показывает только policy hash, budgets и terminal rule;
  raw prompt/output, credentials и provider-specific payload не пересекают
  boundary.

### Execution Backend Registry v1

План 43 закрыт 31 августа 2026 года. Core реализует versioned registry с
обязательным `local.core` и metadata-only remote registrations, bounded endpoint
и capability validation, typed health/failure states, Core-policy intersection
и immutable run snapshot contract. Remote executor/network transport намеренно
не включён и даёт `transport_unavailable`; automatic failover side-effecting
runs запрещён.

Durable metadata и default backend хранятся additive schema v45; credentials
сохраняются только как refs. Authenticated IPC использует command 189/event 44,
Electron panel показывает bounded backend metadata. Проверены invalid endpoint,
stale mutation, local snapshot, remote unavailable, storage idempotency,
protocol/typecheck и metadata-only renderer projection.

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
  и чаты, пользовательские разделы и настройки доступны из меню пользователя,
  открывающегося вверх; технические панели скрыты внутри свёрнутого раздела
  «Интерфейс разработчика» и не перегружают обычный список. Добавлены
  клавиатурное закрытие по Escape, закрытие по клику вне меню,
  active/hover/focus-состояния и адаптивные отступы. Изменения
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
- после перезапуска Core durable build-effect и обычные agent-run проходят lease heartbeat; подтверждённый terminal event приходит в UI как `RESUMABLE`, а неподтверждённый результат остаётся `BLOCKED`; этот срез плана 26 использует schema v36, а после retained-child migration текущая общая storage schema — v37;
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

Windows CI разделён на две фазы: после `Изменения` быстрые проверки
`Документация`, `Rust — быстрые проверки` и `Electron — protocol и typecheck`
запускаются параллельно; после их успеха последовательно выполняются тяжёлые
Rust security-gates, Electron E2E/package, Windows compatibility, native
acceptance и публикация. Каждый этап останавливает последующие при ошибке, а
job-фильтр сохраняет корректные пропуски для незатронутых областей.

## Следующие направления

План 22 закрыт: diagnostics/recovery hardening, защита credential persistence и
documentation/release gate реализованы и перенесены в этот документ и
`architecture.md`. Дальнейшие изменения оформляются новым временным планом
только после отдельного evidence review; до этого поддерживаются текущие
Windows compatibility, package/installer и release gates.

## Граница продукта

Пользовательский продукт ограничен `EvoHime-Setup.exe`, `EvoHime.exe`, локальным Core, supervisor и данными в профиле Windows. Исследовательские и экспериментальные каталоги не входят в установочный runtime.

Legacy web UI, HTTP server, browser launcher и PostgreSQL migrations удалены из репозитория. Electron UI и authenticated versioned named-pipe IPC — текущая пользовательская оболочка и transport boundary; WinUI остаётся временным compatibility runtime для совместимости и тестов.

План 27 реализован в Core/storage/runtime surface: retained-child contract и
registry находятся в `crates/evohime-core/src/retained_child.rs`, durable
таблицы и recovery mailbox — в `crates/evohime-local-storage/src/retained_child_store.rs`,
текущая SQLite schema — v37. IPC использует additive tags 157–161 и
metadata-only projection в OperationsPanel.
29 августа 2026 года свежая проверка этого направления дала: Core 563 теста,
local-storage 208, desktop IPC 35, Electron 470 passed и 2 штатно skipped;
`cargo clippy` с `-D warnings`, `npm run typecheck`, `npm run check:protocol` и
`git diff --check` прошли. Первоначальный timeout chat-store был отдельно
повторён как 8/8 passed, а повторный полный Electron-прогон завершился 470/470.

План 28 закрыт 30 августа 2026 года. Реализованы Core/storage contract v1,
canonical SHA-256 session hash, SQLite schema v38 с metadata-only manifest,
bounded ephemeral `KernelRuntime`, отдельный allowlisted worker binary с
supervisor Job Object launch contract, authenticated additive IPC commands
162–165/events 27–28 и Electron projection/вкладка `Анализ`. Core↔supervisor↔worker
transport проверен в authenticated packaged запуске: Core выполняет admission,
supervisor владеет process lifecycle, а transport failure переводит runtime в
`Crashed`; recovery fencing очищает ephemeral state и запрещает blind retry.
Checkpointable immutable refs и selected child subset подключены к
TaskCheckpoint; ephemeral memory в handoff запрещена. Optional artifact/tool
surfaces fail closed typed `forbidden_capability`/`unavailable`.

Свежая release-проверка: Core 573 теста, local-storage 212, supervisor 32 unit
и 2 real-worker integration, Electron protocol/typecheck/build/bundle checks,
packaged worker manifest и Core/supervisor fault smoke прошли; `clippy` с
`-D warnings` и `git diff --check` прошли. Evidence redacted: raw values,
credentials, transcripts и абсолютные пути не сохраняются.

План 29 закрыт 30 августа 2026 года. Continual Refinement v1 реализован как
Core-owned bounded proposal pipeline: `crates/evohime-core/src/refinement.rs`
проверяет независимые evidence/task ids, canonical SHA-256 content hash,
запрещённые authority-bearing изменения и typed unavailable для Skill/PromptRule;
`crates/evohime-local-storage/src/refinement_store.rs` хранит metadata-only
candidate revisions/events с optimistic versioning в SQLite schema v39. Для
authenticated desktop IPC добавлены additive commands 166–168 и typed
metadata-only projections; Electron `OperationsPanel` показывает очередь и
явные approve/reject/activate/rollback actions. Raw candidate body, transcript,
credentials и hidden reasoning в projection не передаются.

Проверка плана 29: Core 578 тестов, local-storage 214, desktop IPC 35,
Electron 472 passed и 2 штатно skipped; strict clippy, cargo fmt,
`npm run check:protocol`, `npm run typecheck`, release Core build и isolated
real-Core workflow E2E прошли. Skill/PromptRule остаются fail-closed
`unavailable` до появления их Core-owned activation owners.

План 30 реализован 30 августа 2026 года. Workflow Package v1 находится в
`crates/evohime-core/src/workflow_package.rs`: bounded JSON до 1 MiB,
deterministic portable hash, explicit portable/credential argument policy,
fail-closed validation, safe package extension/path boundary и atomic export.
Package bytes не сохраняются в SQLite; metadata-only import history находится в
`crates/evohime-local-storage/src/workflow_package_store.rs` и ставится поверх
schema v39. Authenticated additive IPC commands 169–172 подключены к Core и
Electron, а renderer получает только bounded package metadata/actions через
новую `WorkflowPackagePanel`.

Проверка плана 30: Core package tests, storage package-store test, `cargo check`,
Electron protocol/typecheck и focused workflow tests прошли; полный workspace и
полный Electron regression прогон выполнены перед публикацией.

План 31 реализован: Core владеет typed graph/layout contract, additive
draft/version/handoff schema, bounded Composer handoff, IPC 173/event 33,
authoring, immutable publish, recovery и metadata-only live inspection.
Проверены optimistic revision, single-use handoff, redaction и независимые
execution/layout hashes; комплект планов удалён после финального regression.

План 32 реализован 30 августа 2026 года: Conversational Workflow Composer v1
добавляет strict `composer-request/v1`/`composer-proposal/v1` parser и Core-only
validation/binding поверх Builder v1. Реализованы bounded model gateway path,
typed generate/validate/edit/save/handoff/discard outcomes, additive IPC 174/event
34 и Electron Composer panel. Provenance сохраняется только redacted hashes в
общем Builder storage; raw prompt/output и credentials не входят в storage или
projection. Stale handoff revision/hash rejection и Builder save/reload
проверены focused tests; Composer не запускает workflow сам.

План 33 реализован 30 августа 2026 года. Integration Provider SDK v1 добавляет
bounded Core contract и validator, metadata-only SQLite schema v40,
`fixture.echo`/`FixtureCredentialResolver`, version-pinned integration binding,
authenticated IPC commands 175–176/event 35 и Settings → Integrations.
GitHub/Google/Slack/Linear production adapters остаются typed `unavailable` до
отдельных направлений; credentials, raw output и prompt не пересекают Core
boundary. Проверены Rust contract/storage/runtime tests, protocol/typecheck,
Electron regression (476 passed, 2 штатно skipped) и clippy.

План 34 реализован 30 августа 2026 года. Event Trigger Runtime v1 добавляет
Core contract/validator и bounded local/system ingress с pinned workflow
binding, mapping allowlist, Core-local authenticity, dedup, rate/queue bounds и
typed duplicate/throttle/reject outcomes. Durable metadata schema — v41;
provider webhook без production adapter остаётся typed `unavailable`. IPC
использует additive commands 177–178/event 36, Electron показывает metadata-only
панель «Триггеры событий». Проверены Core 596 tests, local-storage 220 tests,
desktop IPC protocol, Electron typecheck и полный Electron regression: 477
passed, 2 штатно skipped; `cargo fmt --all -- --check` и `git diff --check`
прошли.

План 35 реализован 30 августа 2026 года. Invocation Presets v1 хранит
version-pinned immutable revisions без credential secrets, считает canonical
SHA-256 redacted hash и принимает completed-run metadata только через
fail-closed sanitizer. Explicit migration даёт bounded preview и commit в
новую revision; temporary overrides применяются только к запуску. Automation
schedule хранит preset revision/hash/workspace snapshot, проверяет drift и
передаёт валидный preset в обычный WorkflowRuntime с повторной policy/approval
проверкой. Authenticated IPC использует commands 179–180/event 37, Electron
WorkflowPanel показывает только metadata projection. Комплект планов 35-0 …
35-4 удалён после Core 600, storage 222, desktop IPC 35 и Electron regression
478 passed/2 skipped, protocol/typecheck, fmt, clippy и diff-check.

План 36 реализован 31 августа 2026 года. Agent Benchmark Matrix v1 добавляет
Core contract `agent_benchmark_matrix`, bounded deterministic/unavailable
executors, агрегирование multiple attempts с pass-rate и P50/P95/P99,
baseline/regression verdicts и hard security failures. Metadata-only SQLite
schema обновлена до v42 (`benchmark_store`), suite fixture находится в
`tests/evals/benchmarks/core.json`; существующий static/deterministic eval gate
не изменён. Authenticated IPC использует additive commands 181–182/event 38,
Electron добавляет вкладку «Бенчмарки» с Core-only list/start actions.

Проверено: Core benchmark tests 3/3, local-storage benchmark store 1/1,
`cargo eval benchmark` с 3 attempts создал redacted JSON report, Core check,
Electron protocol/typecheck, focused benchmark panel test и полный Electron
regression 480 passed/2 skipped. Provider `real` без настроенного окружения
честно возвращает `unavailable`; production data/network effects не входят в
  локальную deterministic проверку.

План 37 реализован 31 августа 2026 года. Agent Middleware Pipeline v1 добавляет
Core-owned typed contract с фазами `before/after_agent`, `before/after_model`,
`wrap_model_call`, `before/after_tool`, deterministic ordering, immutable
override, run-pinned contract/policy/capability snapshot и typed duplicate,
blocked, stale/invalid и unknown-safe outcomes. Existing observability и
`PolicyGate` переиспользуются без второй authority; middleware не расширяет
capabilities и не исполняет imported code.

SQLite schema обновлена до v43: сохраняются только definition/run metadata,
не raw prompt/output или transient payloads. Authenticated IPC использует
commands 183–184/event 39, Electron добавляет metadata-only Middleware panel.
Проверены Core contract/recovery tests 7/7, storage idempotency 1/1, desktop
IPC 35/35, protocol/typecheck и focused Electron panel 1/1; full regression
evidence фиксируется в release-evidence для текущего commit.
### Adaptive Tool Catalog v1

План 38 реализован 31 августа 2026 года. Core строит bounded catalog из
`ToolRegistry` manifests, сначала отбрасывая инструменты, которые не проходят
Core permission preflight. Deterministic token selector выбирает максимум 8
tools (hard limit 32), а model/semantic output принимается только после
allowlist validation; unknown/duplicate ids fail closed. Empty/no-match query
использует deterministic top-ranked fallback и не расширяет authority.

Catalog и selection cache derived и process-local: новой SQLite schema/migration
нет, после рестарта они строятся заново. Cache key включает catalog, registry,
policy, grant, query, selector и limit hashes; diagnostics bounded и redacted
входят в `model-trace.jsonl`. `ToolSpec` содержит full input schema только для
выбранных разрешённых tools и их manifest hash. Electron `AdaptiveToolCatalog`
panel показывает только список из authenticated `model.context`; raw prompts,
schemas, credentials и selector output в renderer не передаются.
План 39 реализован 31 августа 2026 года. Structured Response Contract v1
добавляет Core-owned contract/hash/validation, capability-aware Auto,
provider-native и synthetic output-tool strategies с bounded repair, а также
ephemeral recovery lifecycle без новых SQLite таблиц. Authenticated additive
IPC использует tags 185/40; Electron показывает metadata-only projection.
Unknown, unsupported, parse, validation и multiple outcomes остаются
fail-closed; raw model output и credentials не пересекают desktop boundary.

План 40 реализован 31 августа 2026 года. Sensitive Data Guardrails v1 добавляет
Core-owned versioned detector/redactor с deterministic policy hash и actions
redact/mask/hash/block. Recursive structured traversal ограничен depth 16 и
512 nodes; streaming redactor использует bounded carry и ловит patterns между
chunks. Admission подключён к model outbound messages, tool input/output,
stream API и model trace; block/oversize остаются fail-closed, permissions,
approval и effect ledger не ослабляются. Runtime state ephemeral, schema остаётся
v43, raw payload и credentials не входят в IPC/UI projection.

Authenticated IPC расширен additive tags 186/41 (`status`/`evaluate`), Electron
показывает metadata-only панель «Защита данных». Финальные release counts и
команды зафиксированы в `release-evidence.md`.

План 41 закрыт 31 августа 2026 года. Execution Policy Profiles v1 подключён
к `shell.execute` и `process.run` через общий ToolRegistry resolver:
deny-by-default environment, canonical workspace sandbox, bounded timeout и
stdout/stderr, а на Windows — Job Object cleanup всего дерева с fail-closed
required backend. Core возвращает profile metadata/hash, storage schema v44
сохраняет только validated catalog; process handles/output/leases не
персистируются.

Authenticated IPC использует additive tags 187/42, Electron показывает
metadata-only панель «Профили выполнения». Полные release counts и команды
проверки зафиксированы в `release-evidence.md`.

План 44 закрыт 31 августа 2026 года. Tool Simulation Runtime v1 реализует
Core-owned modes `Real`/`Fixture`/`Emulated`/`DryRun`, exact schema-v1 fixture
matching, Structured Response validation и synthetic/fixture provenance.
Simulation перехватывает workflow tool nodes до effect adapter, имеет
idempotent ephemeral state и fail-closed missing/invalid outcomes; после
restart state не восстанавливается, SQLite schema остаётся v45.

Benchmark Matrix получил `FixtureToolBenchmarkExecutor`, authenticated IPC
расширен command 190/event 45, а Electron показывает metadata-only панель
«Симуляция инструментов» с обязательным mode/provenance предупреждением.
Real fallback, raw payload, prompts и credentials запрещены. Contract/recovery,
workflow/benchmark, protocol/typecheck и diff-check evidence зафиксированы в
`release-evidence.md`.

План 45 закрыт 31 августа 2026 года. External Coding Agent Adapter v1 добавляет
Core-owned bounded `evohime.external-agent/v1` framed protocol, validated preset
contract, immutable conversation snapshots, declared credential-slot metadata и
typed capability handshake. Durable metadata использует additive SQLite schema
v46 (`external_agent_presets`, revisions, conversations/events); credential
values, prompts, output, process handles и executable paths не персистируются.
Core→supervisor передаёт только opaque validated run spec; supervisor запускает
allowlisted executable без shell в отдельном Job Object и на cancel/timeout
уничтожает дерево. Vendor adapters без mapping дают `unavailable`, unknown
outcome не retry-ится.

Authenticated IPC добавляет commands 191–192/event 46. Electron panel показывает
только bounded status и фактический `core_control_level`, без external frames,
credentials и raw output. Проверены Rust compilation, protocol generation/check,
TypeScript typecheck и focused Electron privacy test.

План 46 закрыт 31 августа 2026 года. Agent Role Profiles v1 добавляет
Core-owned bounded contract с objective/constraints/skills/tools/strategy,
typed input/output contracts, budget defaults и `human`/`ai` execution mode.
Requested grants вычисляются только как intersection parent grants, policy,
registry и requested set; revision + canonical SHA-256 hash закрепляются на
runtime instance. Catalog хранится metadata-only в SQLite schema v47 с
immutable revisions и recovery после restart; transient instances не
персистируются и unknown outcome не retry-ится вслепую.

Authenticated IPC использует commands 193–194/event 47. Electron показывает
metadata-only панель «Профили ролей»; raw prompts, credentials, executable code
и hidden reasoning не передаются. Реализуемые операции — list/get/create/revise/
start/cancel, stale/duplicate/cancel outcomes типизированы.
# План 48 — Team SOP Protocols v1

Реализован 31 августа 2026 года: Core contract/runtime, schema v49,
immutable session snapshots, authenticated IPC 195–196/event 48 и
metadata-only Electron panel. Focused evidence: Core 2/2, storage 1/1,
Electron 1/1, protocol/typecheck, fmt и diff-check.

# План 49 — Resumable Conversation Event Log v1

Реализован 31 августа 2026 года. Core-owned conversation history использует
SQLite schema v50, отдельную per-conversation sequence, transactional message
acceptance/task binding и stable `client_message_id` dedup. Cursor history
работает в обе стороны, resume подписки начинается строго после
`after_sequence`, retained boundary возвращает typed `cursor_expired`.

Core строит redacted message/status/tool/command/file/browser/approval/task/
goal/child/usage/artifact/backend/recovery/error projections из одного log;
stream delta отделён от durable finalized/failed. Authenticated IPC additive:
commands 197–198/event 49 и поля StartTask 6–7. Electron `TaskTimeline`
использует чистую conversation projection с gap/conflict/duplicate detection,
optimistic reconciliation, retry тем же id и compatibility fallback на
глобальные Core events. Authoritative payload, raw credentials и child details
renderer не получает.
### Memory Governance v1 (план 50, реализован 2026-09-01)

- storage schema обновлена до v51 additive-миграцией: в существующей записи
  памяти хранятся `authority`, `durability` и bounded `confidence`; legacy
  записи получают безопасный `user_asserted`/`durable` профиль;
- `MemoryWriteGate` Core проверяет каждую durable запись непосредственно перед
  SQL effect и отклоняет unknown authority/durability, secret, ephemeral/session
  bypass и непроверенные model/imported confirmed records;
- reinforcement допускается только по двум и более различным evidence refs;
  extraction и ambient кандидаты получают `model_proposed`, остаются pending и
  не становятся retrieval authority без Core validation/explicit confirmation;
- существующие authenticated memory commands и OperationsPanel расширены
  metadata-only governance projection; renderer не получает raw body,
  credentials или authority state-machine.

## Causal Collaboration Bus v1 (план 51, реализован 2026-09-01)

- Core validates a typed envelope and pins routing to an active TeamSession
  protocol hash; sender identity is Core-derived and peer destinations are
  bounded by the Team SOP roster.
- Durable significant messages use the retained-child sequence substrate and
  `collaboration_messages` in SQLite schema v52. Inbox is capped at 128
  pending messages and payload at 32 KiB; subscriptions are process-local.
- Authenticated additive IPC uses commands 199–200/event 50. Electron exposes
  only message metadata, hashes, delivery and provenance; raw payloads,
  prompts, credentials and grants never cross the boundary.
- Delivery is compare-and-set and unknown dispatch outcomes are terminal until
explicit reconciliation; no provider, tool or artifact authority is created
by the bus.

План 52 реализован 1 сентября 2026 года: Conversation Workbench v1 добавляет
рядом с chat единую bounded surface из шести capability-aware tabs. Core
composer использует существующий conversation event log и возвращает typed
metadata-only projection через authenticated command 201/event 51; schema
остаётся v52, отдельного Workbench store нет. Tasks/Usage показывают Core
evidence, Files/Diff/Terminal/Browser — typed `unavailable` до будущих
capabilities планов 55/60. Presentation state ограничен и хранится per
conversation в shell ChatStore; смена conversation сбрасывает projection,
новый event-log cursor инициирует refresh. Raw content, credentials и authority
renderer не пересекают границу.

## Diagnostics & Support Bundle v2 (план 53, реализован 2026-09-01)

Core добавляет authenticated additive command 202 для ephemeral bounded
snapshot: schema v2, health outcomes `PASS/WARN/FAIL/SKIPPED`, duration, bounds,
safe SHA-256 fingerprint, redaction omissions и metadata-only conversation/run
references. Snapshot строится только из Core-owned health facts; новая SQLite
схема/store, raw prompts, workspace files, credentials, tool payloads и
external effects отсутствуют.

Electron main расширяет локальный экспорт до `evohime-support-bundle-v2` ZIP:
manifest, health/runtime/errors/events/logs, локальный issue-draft и
redaction-report. Перед записью выполняется final whole-archive secret/path
scan, запись ограничена user-selected destination, а raw values fail closed.
Settings → Диагностика показывает preview и redaction summary, позволяет
сохранить bundle и скопировать draft; публикация и network upload не
выполняются.

## Human Work Items v1 (план 54, реализован 2026-09-01)

- Core добавляет versioned durable `HumanWorkItem` и state machine с typed
  response schema, optimistic revision, cancellation, revision request и
  fail-closed expiry; user response не становится approval/capability grant.
- Additive SQLite schema v53 хранит JSON current state и transition metadata;
  common migration path делает backup до upgrade. IPC использует command 203 /
  event 52, Electron отображает bounded Inbox-проекцию.
- Team-bound item допускается только для pinned Team SOP human slot; raw model
  prompts, credentials, hidden reasoning и approval payloads отсутствуют.

## Agentic Browser Session v1 (план 55, реализован 2026-09-01)

- Core добавляет `agentic_browser_session` с lifecycle, bounded snapshot,
  session/page revision refs и human takeover fencing; stale refs отвергаются
  typed ошибкой, а control generation переживает смену владельца.
- SQLite schema v54 содержит только bounded `browser_session_metadata`; прямой
  CDP endpoint, raw CSS selector, DOM, cookies, credentials и host screenshot
  path не входят в production contract. IPC command 204/event 53 и Electron
  `AgenticBrowserSessionPanel` показывают metadata-only projection.
- Legacy `browser.session.*` raw path получает `legacy_disabled`. Native
  manifest содержит packaged backend `EvoHime.exe`; supervisor передаёт его
  Core, а Core управляет отдельным ephemeral BrowserWindow. Redirect и
  resolved-IP enforcement выполняются backend-ом на каждом запросе. Snapshot,
  screenshot и download проходят через bounded ArtifactStore; upload принимает
  только artifact locator. При отсутствии package backend сохраняется typed
  `browser_backend_unavailable`, а неизвестный внешний эффект не объявляется
  успешным.

## Plan Artifact v1 (план 57, реализован 2026-09-01)

Plan Artifact v1 добавляет Core-owned durable planning contract: immutable
revisions, canonical SHA-256 hash, bounded steps/risks/assumptions/criteria,
explicit `draft/accepted/executing/paused/replan_required/completed/failed/
unknown_outcome` transitions и durable execution snapshot с exact plan/policy
hash. SQLite schema повышена до v56; IPC использует additive tags 206–208 и
typed event projection 55. Electron `PlanArtifactPanel` не вычисляет state
machine и не обладает authority: все действия проходят authenticated Core.
Неизвестный внешний результат не превращается в success и не повторяется
вслепую; raw prompts, secrets и executable identities не покидают Core.

## Artifact Handoff Registry v1 (план 56, реализован 2026-09-01)

Core получил typed `artifact-handoff/v1` semantic registry поверх существующего
ArtifactStore: registry хранит только immutable revision metadata, lineage,
handoff/acceptance state и idempotency outcomes; bytes не дублируются. SQLite
schema v55 добавляет registry tables транзакционно с backup-before-migrate.
Bounded validation отвергает неизвестную версию, oversized metadata, secret/
prompt/output fields и refs вне `artifact://`; lifecycle включает produced,
review, accepted, needs-revision, superseded, stale и rejected.

Workspace и parent fingerprints остаются Core evidence: unrelated paths не
инвалидируют revision, неизвестный scope даёт `possibly_stale`, исторические
revisions не переписываются. Authenticated additive IPC command 205/event 54 и
Electron `ArtifactHandoffRegistryPanel` показывают только metadata projection;
renderer не получает bytes, raw prompts, outputs, credentials или authority.
Операции list/get/publish/handoff/accept/revise/mark-stale fail closed при
invalid scope/ref и не создают capabilities или внешних effects.
## Incremental Change Protocol v1 (план 59, реализован 2026-09-01)

Добавлен Core-owned bounded pipeline от requirement delta и impact analysis до
versioned run, с exact refs на Plan Artifact/Workspace Checkpoint, optimistic
version и scope fingerprint. SQLite schema v58 хранит только metadata и
redacted evidence; stale drift, duplicate idempotency и terminal unknown-safe
outcomes проверены focused tests. Authenticated IPC использует command 210/event
57, Electron показывает metadata-only `IncrementalChangeProtocolPanel`; внешний
effect базовым executor намеренно не выполняется.

План 60 закрыт 1 сентября 2026 года: Revision-Safe Workspace Files v1
реализован в общей tool boundary. Typed namespaces, `FileRef` hash/revision,
stale preconditions, immutable uploads, run-scoped scratch, strict patch
context, advanced file adapters и additive authenticated IPC/UI projection
проверены Rust 677 тестами, tool-runtime tests, Electron suite 498 тестами,
protocol/typecheck, clippy и diff-check. Raw absolute paths новая surface не
выдаёт; IPC mutations остаются только в Core approval path.

План 61 закрыт 1 сентября 2026 года: Task Worktree Isolation v1 добавляет
durable registry в schema v59, optimistic lifecycle transitions, task-to-root
pinning и approved detached Git worktree create/remove tools. Dirty cleanup,
ref injection, duplicate/stale registry transitions и реальный временный Git
worktree проверены focused tests; authenticated IPC и Electron projection
передают только metadata, без host-path authority.
Team Resource Budget v1 реализован 1 сентября 2026 года: Core contract
TeamBudgetPolicy/TeamBudgetState/ResourceUsageEvent с canonical hash,
soft/hard shared limits, allocations, protected reserve, unknown-cost и
wall-clock modes; SQLite schema v60 сохраняет policy/state/usage/request
metadata с idempotency и optimistic version fence. Core IPC предоставляет
validation, durable policy/state/usage recording и conservative preflight;
Electron получает только bounded metadata projection.
План 63 закрыт 1 сентября 2026 года: Composable Termination Conditions v1
добавляет Core-owned policy/state для тринадцати встроенных условий и композиций
`Any`/`All`, canonical hash, event cursor, first-trigger evidence и terminal
outcome. Состояние и policy сохраняются в SQLite schema v61 с optimistic
version fence; authenticated IPC использует command 214/event 60, Electron
показывает bounded metadata-only projection. Replay не удваивает counters,
hard-stop не уступает continuation, а model/provider/tool authority не
расширяется. Комплект `63-0` … `63-4` удалён после свежих regression gates.
План 64 закрыт 2 сентября 2026 года: Workspace Bootstrap Manifest v1. Добавлены
bounded Core manifest/validator, allowlisted discovery, exact trust/hash
проверка, schema v62 preparation cache с fingerprint invalidation, single-flight
lease и recovery в `unknown_outcome`. Запуск ограничен существующей
ExecutionPolicy process boundary и не передаёт наружу raw output, environment,
секреты или абсолютные пути. Authenticated IPC использует command 215/event 61,
а Electron — metadata-only developer panel.
План 65 закрыт 2 сентября 2026 года: Team Coordination Policies v1 добавляет
versioned TeamSpec, RoundRobin/Selector/DirectedHandoff/RoleRouter, Core
validation selector target и repeated-selection guards. State хранится в SQLite
schema v63 с optimistic version/idempotency, а authenticated IPC использует
command 216/event 60 и metadata-only Electron projection. Координация не
запускает effects и не расширяет grants.
Обновлено: 2026-09-02. План 66 закрыт: Typed Agent Handoff Contract v1
реализует bounded `HandoffPacket`, ACK/NACK lifecycle, expiry/stale guards,
structured context budget и provenance. Pending handoffs сохраняются в SQLite
schema v64; Core handler не наследует capabilities/credentials, а command 217
и event 62 с Electron-панелью передают только metadata projection.
План 67 закрыт 2 сентября 2026 года: Schema-Driven Agent Configuration v1.
Core публикует versioned schema и effective snapshots для пяти слоёв, валидирует
typed registry references и semantic patches, redacts secret values и сохраняет
revision-fenced snapshots в SQLite schema v65. Authenticated IPC command 218 /
event 63 и Electron developer panel передают только bounded metadata projection;
active run snapshot не мутируется.
План 68 закрыт 2 сентября 2026 года: Experience Replay Library v1. Добавлены
bounded `ExperienceRecord`/`ExperienceStep`, evidence-backed Write Gate,
unknown-outcome rejection, scope/hash/redaction checks, duplicate-safe SQLite
schema v66 и bounded untrusted context projection. Authenticated IPC использует
command 219/event 64, Electron — metadata-only panel; опыт не становится
Memory, Refinement или источником capabilities.
План 69 закрыт 2 сентября 2026 года: Runtime Intervention Pipeline v1.
Middleware-контур расширен до typed hook phases для handoff, workflow commit и
external publish, добавлены explicit pause/abort decisions, fail-closed policy
metadata и bounded reentrancy guard. Authenticated IPC command 220/event 65 и
Electron developer panel показывают только безопасную диагностику; renderer не
регистрирует authoritative handlers.
План 70 закрыт 2 сентября 2026 года: Code Diagnostics Feedback Loop v1.
Добавлены Core-owned provider registry, versioned diagnostics с canonical
workspace/file binding, durable SQLite schema v67, snapshots и deterministic
introduced/resolved/persisting delta, typed quality gate, authenticated IPC
command 221/event 66 и Electron Problems projection. Stale refs, oversized или
неcanonical input отклоняются; renderer не получает authority или raw output.
План 71 закрыт 2 сентября 2026 года: Workflow Optimization Lab v1.
Добавлены bounded OptimizationRun/Candidate, declarative search constraints,
Benchmark Matrix evaluation, durable SQLite schema v68, security hard rejection,
explicit holdout-gated promotion, authenticated IPC command 222/event 67 и
Electron metadata-only panel. Лаборатория offline-only и не может сама менять
production workflow.
План 72 закрыт 2 сентября 2026 года: Core Topic/Subscription Event Bus v1.
Добавлены typed topic/event/subscription contracts, exact/prefix/type routing,
capability checks, durable SQLite schema v69, ACK/NACK/retry/dead-letter,
backpressure bounds и restart reconciliation. Authenticated IPC command 223 /
event 68 и Electron metadata-only panel не расширяют authority; внешний broker
не требуется.
План 73 закрыт 2 сентября 2026 года: Dependency-Aware Task Graph v1.
Добавлены bounded typed tasks/dependencies, DAG validation, deterministic
ready-set, immutable completed revisions, atomic semantic replan patch и
downstream-only invalidation. Durable SQLite schema v70, authenticated IPC
command 224/event 69 и metadata-only Electron panel проверены; renderer не
владеет графом и не запускает effects.
План 74 закрыт 2 сентября 2026 года: Declarative Agent Component Registry v1.
Добавлены stable provider IDs, typed descriptors, built-in trust allowlist,
schema/version validation, explicit migration, dependency-cycle detection,
secret rejection, inspect/diff/dump операции и durable SQLite schema v71.
Authenticated IPC command 225/event 70 и metadata-only Electron panel не
расширяют authority и не загружают dynamic code.
План 75 закрыт 2 сентября 2026 года: Typed Context References v1. Добавлены
versioned ContextRef/ResolvedContextRef, built-in resolver registry,
revision/hash binding, lazy context budget, safe locator/URL validation и
durable SQLite schema v72. Authenticated IPC command 226/event 71 и
metadata-only Electron panel не дают renderer filesystem/network authority;
referenced content остаётся untrusted data.

План 76 закрыт 2 сентября 2026 года: Safe UI Extension Framework v1.
Добавлены versioned declarative manifest и bounded host-rendered
contributions, Core-owned trust/compatibility metadata, durable scoped
install-disabled lifecycle с revision fencing, SQLite schema v73,
authenticated IPC command 227/event 72 и metadata-only Electron UI.
Arbitrary renderer code, shell/filesystem/network bindings и auto-enable после
restart запрещены; focused Rust/storage/Electron checks и полный Electron
regression пройдены.

План 77 закрыт 2 сентября 2026 года: Headless Core CLI v1. Добавлены Core
contract для bounded RunRequest и стабильных terminal events, роль
authenticated `cli`, Rust companion binary `eva.exe`, human/one-shot и NDJSON
вывод, bounded stdin, detached run acceptance, status/watch/cancel и
reconnect по event cursor. CLI использует существующие Core task/workflow
contracts, не получает прямой доступ к БД или runtime, redacts sensitive
projection и поставляется в Windows package; `resume` возвращает typed
unavailable, поскольку безопасный task-resume contract в Core ещё не объявлен.

План 78 закрыт 2 сентября 2026 года: Capability Workbench v1. Добавлены
versioned Core-owned descriptor/instance/lifecycle/scope/concurrency contract,
dynamic capability-filtered tools, shared resources, typed call/cancellation
outcomes, safe snapshot validation, heartbeat/expiry recovery и durable SQLite
schema v74 для instances, snapshots и leases. Authenticated IPC использует
command 228/event 73; Electron developer panel остаётся metadata-only и не
получает authority или runtime handles. Raw credentials, prompts/outputs,
OS handles и executable identities в portable state запрещены.
План 79 закрыт 2 сентября 2026 года: Team Coordinator v1 реализует durable
bounded work items, capability/output-aware proposals, consultations,
decomposition/reassignment, managerial review с независимыми gate checks,
revision-fenced storage, authenticated IPC command 229/event 74 и
projection-only Electron panel. SQLite schema поднята до v75; raw prompts,
outputs, credentials, grants и executable identities не передаются.

План 80 закрыт 2 сентября 2026 года: Project Instruction Stack v1 реализует
Core-owned discovery/normalization/precedence для глобальных, workspace,
вложенных и AGENTS-источников, path/explicit activation, bounded snapshot,
hash/revision provenance, durable SQLite schema v76, authenticated IPC command
230/event 75 и metadata-only Electron panel. Перед model call Core фиксирует
snapshot hash и применяет только bounded untrusted instruction context;
authority-bearing metadata, executable semantics, secrets и raw content в UI
projection запрещены.

План 100 закрыт 2 сентября 2026 года: Workspace Sets v1 реализует bounded
Core-owned multi-root contract с уникальными aliases, per-root grants/kinds/VCS
identity, canonical root-qualified refs, bounded cross-root search, durable
schema v77, idempotent version-fenced updates, pinned run bindings и typed
recovery metadata. Authenticated IPC command 231/event 76 и Electron panel
остаются projection-only; host paths, secrets, raw prompts/outputs и implicit
parent-directory authority не передаются. Независимые Git roots не объявляются
filesystem-atomic; partial/unknown outcomes сохраняют per-root semantics.

План 101 закрыт 2 сентября 2026 года: Knowledge Source Registry v1 реализует
отдельные от Memory versioned KnowledgeSource/Binding/View/Chunk/Hit contracts,
Ready/stale lifecycle, sensitivity authorization, source revision/locator
provenance, durable SQLite schema v78, bounded keyword indexing/retrieval,
authenticated IPC command 232/event 77 и metadata-only Electron panel. Registry
и manifests переживают restart; parser content не исполняется, secrets/raw
chunks не передаются renderer, unauthorized source не используется.
План 102 закрыт 2 сентября 2026 года: Agent Git Change Sets v1 реализует
Core-owned bounded baseline, attribution и immutable commit-candidate metadata,
durable schema 79, additive IPC 233/78, Electron projection и безопасные
typed refusals для commit/undo без preflight. Проверены focused Rust tests,
clippy, protocol/typecheck и `git diff --check`.
План 90 закрыт 2 сентября 2026 года: Runtime Stall Guard добавил bounded
static detector `scripts/runtime-stall-guard.ps1`, fingerprint-based
suppression metadata, redacted JSON report и CI gate в Windows workflow.
Локальный smoke `scripts/runtime-stall-guard.tests.ps1` пройден; detector не
исполняет найденный код и не пишет absolute paths или sensitive payload.
План 103 закрыт 2 сентября 2026 года через подтверждённое переиспользование
Capability Workbench v1 (план 78): versioned descriptor/session, lifecycle,
scope/concurrency, leases, reset/degraded recovery, capability filtering и
secret-free bounded snapshots уже реализованы в Core/storage/IPC/UI. Второй
authority не добавлялся.
