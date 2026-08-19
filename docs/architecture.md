# EvoHime — Windows desktop architecture

Статус: текущая утверждённая архитектура продукта. Фактическое состояние реализации см. в [`current-state.md`](current-state.md).

EvoHime — локальное Windows-приложение.
Пользовательское короткое имя агента — «Ева».

```text
EvoHime.exe               Electron main + bundled renderer
        │ preload/contextBridge → desktop-ipc-v1 / named pipe
evohime-core.exe          agent loop, model gateway, tools, SQLite
        ▲
evohime-supervisor.exe    mutex, Job Object, restart, JSONL diagnostics
        │
evohime-transaction.exe   transactional update worker
```

Renderer не имеет node integration, не выполняет shell-команды и не открывает базу. Electron main ограничен окном, lifecycle, локальным состоянием оболочки и IPC adapter. Core владеет workspace, инструментами, моделью, секретами и локальным состоянием. Supervisor запускает core в Job Object и завершает дочернее дерево при остановке.

Ревью планов — отдельный read-only pipeline Core. Electron main выбирает и
ограниченно читает Markdown-файл через native dialog, затем передаёт его Core.
Core вызывает 2–8 моделей текущего provider catalog по очереди, по одному
запросу за раз, чтобы не упираться в лимиты провайдера, и затем отдельную
synthesis-модель. Per-request model overrides сохраняются. Состав и порядок
рецензентов фиксируются на момент запуска: неудачное обновление каталога
возвращает пустой список и трактуется как «нет новостей», а не как «нет
моделей», поэтому уже выбранные модели не теряются. Исходный Markdown
ограничен 512 КБ, ответ каждого рецензента — 256 КБ. На ревью можно подать
несколько файлов сразу (диалогом или перетаскиванием): оболочка склеивает их в
один документ с нумерованными разделами и проверяет суммарный размер. Перед
запуском объём запроса сверяется с окном каждой выбранной модели: заведомо не
влезающий план блокирует запуск, а худший случай синтеза (план плюс все ответы
рецензентов) остаётся предупреждением. Review не получает tools,
не изменяет workspace и сохраняется в локальном event journal без credentials;
история ревью очищается отдельной командой и исчезает из UI сразу, а не после
перезапуска.

Правка плана по ревью — второй шаг того же pipeline и такой же read-only на
стороне модели: один вызов synthesis-модели получает исходный план и текст
ревью и возвращает переписанный план целиком. Диффа не запрашивается — модели
надёжнее воспроизводят документ, чем адресуют куски. Текст ревью Core берёт из
своего кэша или журнала, а не из запроса оболочки, поэтому выдать за ревью
произвольный текст нельзя. Результат живёт в памяти Core до отдельной команды
сохранения, показывается пользователю целиком и записывается только по его
решению — поверх оригинала или в новый файл; расширение `.md` проверяет Core.
Правка работает по одному файлу: склеенный из нескольких планов документ
нельзя однозначно разложить обратно.

WinUI 3 больше не является пользовательской оболочкой пакета. Он сохранён как
временный compatibility runtime и oracle для совместимости IPC до отдельного
решения о его удалении.

## Оболочка

Renderer состоит из панели проектов и чатов, ленты диалога и инструментальных разделов.

| Поверхность | Назначение |
| --- | --- |
| `ProjectSidebar` | проекты (workspace) и чаты внутри проекта; аккаунт и вход в настройки внизу |
| `HomeScreen` | стартовый экран; первый запрос сам создаёт чат |
| `TaskTimeline` + `ActivityLine` + `transcript.ts` | ход задачи, свёрнутый в читаемую ленту; ответы агента рендерятся Markdown |
| `tool-names.ts` | русские подписи инструментов вместо служебных идентификаторов |
| `RepositoryBar` | ветка и счётчики изменений открытого репозитория |
| `ModelPicker` | выбор модели в чате; каталог разделён на free и paid |
| `ProviderForm` | единственная поверхность настроек провайдера (ключ, модель, base URL) |
| `PlanReviewPanel` | коллективное read-only ревью Markdown-плана несколькими моделями и synthesis-моделью; итог копируется в буфер или экспортируется в Markdown, история очищается кнопкой |
| `RecoveryBanner` + `recovery-state.ts` | состояние восстановления, выведенное только из подтверждённых Core событий |
| `OperationsPanel` | очередь подтверждения памяти и конфликты (только metadata), плюс read-only проекция child- и schedule-событий |
| `OverviewPanel`, `TracePanel` | сводка событий запуска и фильтруемая трасса |

Бизнес-логики в renderer нет: он отображает состояние, полученное через IPC, и отправляет команды.

## IPC

Контракт находится в `crates/desktop-ipc/proto/evohime.desktop.proto`.

- major-версия несовместима, minor-расширения совместимы;
- фреймы ограничены 4 MiB;
- события имеют монотонный `sequence_id`;
- UI может запросить replay после последнего sequence ID;
- cancellation передаётся отдельной командой `StopTask`;
- `SelectModelRequest` меняет модель следующего запроса без перезапуска Core: gateway разрешает модель на каждый вызов, пустое значение возвращает модель маршрута;
- `CancelDatabaseOperation` кооперативно отменяет выполняющийся backup или restore;
- `ClearPlanReviewHistory` удаляет сохранённые ревью планов из локального журнала; Core отвечает подтверждением, и UI перестаёт показывать историю немедленно;
- `RevisePlan`, `StopRevision` и `SaveRevisedPlan` правят план по готовому ревью. `RevisePlan` подтверждается сразу, результат приходит событием `task.completed` с `task_id` вида `revision-<uuid>`, прогресс — событием `revision.progress`. `SaveRevisedPlan` пишет файл сам и принимает только путь с расширением `.md`: запись — единственный шаг, где правка покидает память Core. Отказ сохранения приходит событием `plan.save_failed`, а не ошибкой кадра: ошибка кадра рвёт соединение с оболочкой, и опечатка в имени файла читалась бы как падение ядра. Правку, которой уже нет в памяти, Core ищет в журнале — перезапуск Core при обновлении не должен отнимать возможность сохранить готовый текст;
- команды памяти `GetMemory`, `ListMemoryPending`, `GetMemoryConflicts`, `ConfirmMemory`, `RejectMemory`, `SupersedeMemory`, `ReviseMemoryCandidate` аддитивны. `ListMemory`, `SearchMemory` и `ListMemoryPending` возвращают только metadata; тело записи доступно исключительно через явный `GetMemory` и маскируется для `sensitive` и забытых записей. Confirm/reject/supersede требуют approval-токен и idempotency key: повтор безопасен и возвращает фактическое состояние записи.

## Signed receipts

Canonical Receipt v1 реализован в `crates/evohime-receipts` и Electron main
consumer `desktop/evohime-electron/src/main/receipt-crypto.ts`. Нормативные
JCS bytes, envelope `receipt_hash`, Ed25519, result domain, schema, limits,
stable error codes и cross-language vectors находятся в
`contracts/receipts/v1/`; подробное правило — `docs/security/receipt-canonical-v1.md`.
Этап 01.1 фиксирует bytes и проверку контракта. Key lifecycle реализован в
`crates/evohime-receipts`: Windows DPAPI CurrentUser, owner-only DACL,
SQLite-источник переходов и audit, journaled rotation/recovery, explicit
trusted genesis, signed checkpoint contract и `evohime-verify.exe`. Core
публикует renderer только bounded status/key metadata; private material не
выходит из Core. JSONL history является post-commit snapshot с manifest и
статусом stale при ошибке экспорта. Runtime orchestration 01.3 теперь
выполняется Core-owned `ReceiptRuntime`: mutation path использует durable
`pre_action` до dispatch, approval хранится как bounded one-shot intent, а
terminal post/refusal append-ятся в SQLite hash-chain. Startup recovery
устанавливает guard, истекает старые approval intents и переводит незакрытые
вызовы в `pending_recovery`; raw input/result в receipt runtime не сохраняются.
Для восстановления результата Core предоставляет authenticated
`ReconcilePendingReceiptAction`: он создаёт новый read-only action с собственным
hash/call binding и атомарно связывает его с историческим action; исходный tool
повторно не запускается. `ClosePendingReceiptAction` закрывает только explicit
unknown-result как signed refusal, а authenticated `UnquarantineReceiptAction`
проверяет trusted signed checkpoint и закрывает только invariant violation как
refusal. Protected recovery rows шифруются
AES-256-GCM, а storage-key rotation выполняется возобновляемыми bounded batch с
durable cursor. Read-only sampling, recovery state и bounded runtime counters
доступны только через Core diagnostics.
Retention/compaction receipt chain по-прежнему относится к отдельному этапу
01.4.

## Context Budget Manager

Сборка контекста реализована в `crates/context-budget` (контракты и детерминированная логика), `crates/evohime-local-storage` (ledger, scratchpad, artifact store, команды) и `crates/evohime-core/src/context_budget.rs` (интеграция в agent loop). Этот раздел — канонический контракт: исходный план удалён из `docs/plans/` после реализации, как того требует правило каталога.

**Контур.** Перед каждым model call Core выполняет `selection -> compress/offload -> финальная проверка бюджета -> событие ModelContext -> вызов модели`. Финальная проверка обязательна и выполняется до формирования события; при её невыполнении Core проходит оставшиеся уровни лестницы, а после их исчерпания завершает вызов через `BudgetUnavailable` без обращения к модели.

**Бюджет и профиль.** `ModelContextProfile` версионируется и выбирается по provider/model из каталога `crates/context-budget/profiles.json`, который можно перекрыть пользовательским конфигом того же формата. Профиль обязан удовлетворять правилам валидности `0 < target < soft < hard <= max`, `target + reserves <= soft` и `absolute_mvc_max_limit + reserves <= hard`; невалидный профиль отклоняется при загрузке, а неизвестная модель получает fallback-профиль (60% / 75% / 85% окна) и не может обойти эти ограничения. `target_tokens` — цель сокращения, `soft_limit_tokens` — порог его запуска, `hard_limit_tokens` — граница отказа; резервы считаются сверх контекста и не могут быть заняты историей или схемами.

**Обязательный минимум.** `minimum_viable_context` вычисляется детерминированно и всегда включает safety/system policy и текущий user prompt; approval semantics, незавершённый tool-call и cancellation добавляются при наличии таких состояний. Порядок частей фиксирован и задаёт как порядок в собранном контексте, так и выбор `missing_part`. Safety- и approval-часть не сокращается никогда: конфликт «safety не влезает в бюджет» разрешается отказом от вызова, а не урезанием safety.

**Лестница сокращения** конечна и упорядочена: L1 expired/duplicate/superseded, L2 low-priority optional, L3 самые старые завершённые tool outputs, L4 offload крупных item в artifact store, L5 сжатие истории, L6 отказ от необязательных резервов (`retry` → `streaming` → `tool_schema`; `tool_call` и `final_answer` не сокращаются никогда). Каждый уровень применяется не более одного раза и обязан строго уменьшать размер, поэтому цикл завершается всегда. Внутри уровня порядок детерминирован: pinned последним, затем по возрастанию `effective_priority`, `created_at`, `content_hash` и `id`. Недоступные artifact store или summarizer пропускают L4/L5 с diagnostic, а не роняют сборку.

**Отказ сборки.** `BudgetUnavailable` — терминальный результат со стадиями `mandatory_overflow`, `drops_exhausted`, `estimator_unavailable` и `provider_replan_failed`. Автоматический retry запрещён на всех уровнях; context-length error провайдера даёт ровно один deterministic re-plan с уменьшенным `hard_limit_tokens`, повторный отказ каскада не порождает. До UI отказ доходит bounded причиной с кодом, стадией, требуемым и доступным объёмом и указанием непоместившейся части — не молчаливым обрывом ответа.

**Оценка токенов.** Estimator версионируется и обязан быть консервативным: занижение считается дефектом. При недоступности основного используется fallback-estimator (`ceil(utf8_bytes / 2) + 16`) с порогами профиля, масштабированными на 0.70; при недоступности обоих сборка завершается отказом, а не оценкой по умолчанию. Оценка кэшируется по `content_hash` вместе с версиями tokenizer, нормализатора и chat-template, поэтому смена любой из них не даёт стухший кэш-хит.

**`content_hash`** — SHA-256 в строчном hex от `normalizer_version`, разделителя `0x00`, `kind`, `0x00` и нормализованного содержимого. Текст нормализуется в фиксированном порядке: UTF-8, NFC, перевод CRLF и CR в LF, удаление завершающих пробелов в строках и завершающих пустых строк. JSON приводится к канонической форме с сортировкой ключей и фиксированным представлением чисел, двоичное содержимое хешируется как есть. Правила зафиксированы эталонными векторами в тестах, и версия нормализатора входит в hash input, а не только в кэш-ключ.

**`context_ledger`** — одна immutable запись на один model call. Её hash покрывает ids и порядок выбранных item, версии profile, tokenizer, нормализатора и стратегии, обязательный набор, отброшенные item с причинами, применённые compression- и ladder-решения, fallback-флаг и loadout. Hash считается один раз после фиксации состава и публикуется до вызова модели, поэтому потребители сравнивают его с записью, а не пересчитывают контекст. Фактический usage провайдера пишется в отдельную append-only таблицу `context_ledger_usage`, чтобы запись оставалась hash-стабильной. Ротация хранит записи моложе 30 дней или принадлежащие последним 200 сессиям и не удаляет записи, на которые ссылается неэкспортированный receipt.

**Scratchpad задачи** делится на `facts`, `open_questions`, `decisions`, `tool_findings` и `next_actions`. Внешний вывод инструмента помещается в `data_not_instructions` envelope и проверяется на prompt-injection; `confirmed` запись появляется только после provenance/policy-проверки Core, явного подтверждения пользователя или завершённой policy-операции — успешный tool result сам по себе фактом не становится. Подтверждённая запись не перезаписывается на месте, только новой ревизией. После restart в рабочий контекст возвращаются только `confirmed`; остальные изолируются как `recovered` с `trust=unverified` и пониженным приоритетом и удаляются через час или 10 шагов. При переполнении категории бюджета самые старые `confirmed` записи выгружаются в artifact store и остаются в контексте bounded ссылкой с hash и locator; `open_questions` и обязательный минимум не вытесняются, молчаливое усечение запрещено.

**Artifact store** адресует содержимое по `content_hash`: повторный offload переиспользует артефакт и добавляет ссылку, а не копию. Пространство имён per-task, доступ по locator ограничен задачей-владельцем и её детьми, чтение заново сверяет hash и помечает ссылку `invalid` при расхождении. Вытеснение идёт по TTL и последнему обращению; ссылка живого ledger entry или confirmed записи scratchpad помечается `expired` с сохранением hash и размера, а удалённое содержимое оставляет tombstone, который не считается доступным dedup-hit.

**Compression и pruning.** `duplicate` — совпадение `content_hash`, `superseded` — новая ревизия того же ключа при другом содержимом, `expired` — истёкший TTL или retention. Иерархия прав: safety и approval выше system instructions, те выше явных ограничений пользователя, далее confirmed facts, history и данные инструментов, ниже всего recovered и unverified. Recency и trust решают исход только внутри одного уровня. Summarizer — отдельный Core-вызов того же model gateway с собственным `summary_budget` и входным лимитом на prompt, без инструментов и без повторов; недоступность, превышение бюджета или невалидный результат дают deterministic fallback без каскадного повтора. Исходные item остаются source of truth, а summary хранит связь `summary_id -> source_ids`.

**Tool loadout.** Инструменты делятся на обязательные, read-only и mutation. Deterministic intent router нормализует prompt и активные `open_questions`, сопоставляет их с versioned таблицей capability keywords и применяет deny-правила; при конфликте правил выбирается более безопасный read-only результат, при неопределённом intent — read-only fallback loadout. Обязательные инструменты входят всегда и расходуют отдельный `mandatory_schema_reserve`, остальные ограничены `tool_schema_reserve`. Permission и approval semantics выбранного инструмента остаются видимыми, а вызов вне loadout Core отклоняет до эффекта с bounded diagnostic `loadout_miss`.

**IPC и UI.** Событие `ModelContext` расширено additive-полем `context` с bounded projection: бюджет, ids выбранных item, причины сокращения, `context_ledger_hash`, compression summary, loadout и отказ сборки. Старый клиент игнорирует неизвестное поле, major bump не требуется. Команды `GetContextLedger`, `ListTaskScratchpad`, `ClearTaskScratchpad`, `SummarizeContextNow`, `PinContextItem` и `ReadContextArtifact` аддитивны; каждая mutation получает запись аудита и подчиняется rate limit, посчитанному по журналу и потому переживающему перезапуск Core. `SummarizeContextNow` действует только на текущую task-scoped сборку и не меняет долговременную память. `PinContextItem` повышает приоритет, но не гарантирует включение: при нехватке бюджета pinned item отбрасывается последним и с явной причиной. `ForgetMemory` каскадно удаляет производные заметки и task artifacts, сохраняя redacted факт удаления. UI не получает prompt, тело памяти, raw tool output и неограниченные списки ids.

## Local Agentic RAG

Локальный индекс workspace реализован в `crates/evohime-core/src/workspace_rag.rs`, миграции и данные находятся в общей Core-owned SQLite (schema v19), а команды проходят через authenticated desktop IPC. Исходный план 01 удалён после реализации; этот раздел является каноническим контрактом.

**Граница безопасности.** Перед чтением scanner канонизирует workspace и каждый путь, запрещает абсолютные/parent/UNC escapes, symlink/reparse traversal, встроенные secret paths (`.env*`, ключи, `secrets/`, `.git`, build/vendor directories) и patterns из `.ragignore`. Canonical path и metadata проверяются до и после bounded read. Секретный, binary-looking, oversized, minified или нестабильный файл не создаёт chunks. Renderer не читает filesystem/SQLite, не выбирает embedding backend и не может расширить scope; retrieval никогда не является разрешением на действие и не ослабляет sandbox, permission или approval.

**Публикация индекса.** `workspace_index_runs`, `workspace_documents`, `document_chunks` и `workspace_chunks_fts` хранят отдельные поколения. Run получает производный от canonical root `workspace_key`, строит новое поколение в состоянии `running`, проверяет отсутствие orphan/ghost FTS rows и короткой транзакцией переводит прежнее `published` поколение в `superseded`, а новое — в `published`. Отмена, timeout, crash, unstable snapshot или ошибка не меняют published pointer; незавершённый run при следующем старте становится `failed`. На workspace разрешён один run: параллельная команда получает bounded lease error, `CancelWorkspaceIndex` кооперативно отменяет scanner/vector build. После успешной публикации остаются текущее и одно предыдущее поколение.

**Scanner и chunker v1.** Defaults валидируются до run и ограничивают размер/число файлов, длину строки, chunks на документ/run, размер chunk, retry, timeout и частоту progress. Поддержаны README/Markdown, Rust, TypeScript/JavaScript, JSON, TOML, YAML и plain text; UTF-8 и UTF-16 распознаются явно, lossy decode разрешён только для неструктурного текста. Markdown режется по заголовкам, код — по структурным boundary с детерминированным fallback, structured text — по ключам/блокам, остальное — bounded recursive chunks. `file_hash` — SHA-256 исходных bytes; `chunk_hash` — SHA-256 versioned payload из текста, parent context, language и chunker version, без offset. Incremental run копирует неизменившийся snapshot, а изменившийся файл перестраивает; byte offsets относятся к исходным bytes, line range всегда перепроверяется по свежему файлу.

**FTS5 retrieval.** SQLite FTS5 использует trigram tokenizer для content, normalized symbol, path и parent context; metadata scope по workspace/generation/path/language применяется отдельными индексами. Три лимита удовлетворяют `max_retrieval_chunks >= max_evidence_chunks >= max_context_chunks`; целый файл никогда не попадает в prompt из-за одного совпадения. Ranking использует фиксированные веса BM25 и tie-break `score -> path bytes -> document id -> byte start -> chunk id`. Каждый результат содержит path, byte range, свежие lines или `null`, file/chunk hash, score explanation, `stale` и redaction status. Перед возвратом Core заново проверяет canonical path, size, file hash и range; stale/full-redacted content не передаётся модели.

**Planner и checker.** Каноническая strict Draft 2020-12 schema лежит в `crates/evohime-core/schemas/workspace-query-plan.schema.json`. Pre-check без LLM выбирает `exact_symbol`, `lexical`, `path` или `metadata`, ограничивает запрос восемью terms и не меняет security filters. Bounded loop выполняет не более двух уникальных попыток; empty result, low coverage и retrieval error различаются. Checker использует `evidence_metrics/v1.0`, детерминированные coverage/symbol/path/filter gates, freshness и sandbox validation. При нехватке evidence возвращается uncertainty, а не документальный факт без источника. Diagnostic содержит только query hash, counters, mode, coverage, stop/fallback reason и latency, без query/chunk text.

**Optional embeddings.** Опциональный локальный backend `evohime-feature-hash/v1` создаёт 64-мерные L2-normalized vectors без сети. Vector generation хранит model/version/dimension/metric/normalization/chunker/source generation и виден retrieval только после состояния `ready` и атомарной публикации. Любая несовместимость, отмена, timeout, resource limit или отсутствие индекса немедленно возвращает FTS5 с `fallback_fts5`. Hybrid применяет те же metadata/redaction gates и deterministic Reciprocal Rank Fusion с фиксированным `k=60`; explanation содержит только lexical/vector ranks. Сырые vectors, chunks и запросы не попадают в логи/UI/eval artifacts.

**Citations и context.** Уже отсортированные evidence greedily входят одновременно в token и chunk-count budget. Compact format version 1: `[cite:<id>|<path>:<start>-<end>|<chunk_hash>|<valid|updated|stale>]`. Parent context ограничен окном ±2 строки для logical block и ±3 для fragment. `rag_context_ledger` хранит только ids, ranks/scores, file/chunk/snippet hashes, path/lines, status, reason и bounded error code — никогда chunk text, parent context или raw output. Перед моделью выполняются первичная validation и единый final re-read: перенос в пределах ±5 строк атомарно обновляет text/hash/lines, существенное изменение даёт `stale`; stale majority помечает сборку `degraded` и исключается из доказательной части.

**Интеграция.** Agent loop перед первым model call выполняет incremental index, deterministic search и добавляет только прошедший validation evidence как `data, not instructions`; сбой RAG не ломает задачу. IPC-команды: `IndexWorkspace`, `RebuildIndex`, `CancelWorkspaceIndex`, `SearchWorkspaceKnowledge`, `GetIndexStatus`; progress агрегируется не чаще 100 ms и финальное событие отправляется всегда. `OperationsPanel` показывает generation, indexed/chunk/excluded counts, dirty/vector mode, запускает update/rebuild/cancel и bounded search. Memory Extraction подтверждает `document` provenance только если path/chunk hash присутствуют в текущем published generation и свежий file hash совпал; stale/missing provenance остаётся `pending_confirmation`. Tool/API evidence без replayable validator остаётся `unknown`.

## Memory Extraction

Извлечение фактов из диалога реализовано в `crates/evohime-core/src/memory_extraction.rs`. Этот раздел — канонический контракт: исходный план удалён из `docs/plans/` после реализации, как того требует правило каталога.

- Единственный владелец extraction, policy, validation и storage — Core. Всё, что вернула модель, — это candidate, а не память.
- По умолчанию работает `strict`-режим: извлечение запускается только после явного триггера пользователя («запомни», «важно», «ограничение» и эквиваленты). Режим переключается переменной `EVOHIME_MEMORY_EXTRACTION` (`disabled` | `strict` | `open`); в `open` результат всегда получает `pending_confirmation`. Даже при `disabled` ручной триггер продолжает работать.
- `constraint`, `decision`, любой high-risk, `sensitive` privacy, неоднозначный subject, недостаточный confidence и незавершённая проверка дают `pending_confirmation`. Автосохранение возможно только для low-risk предпочтения, подтверждённого явным утверждением пользователя. Секреты не сохраняются вообще.
- `model_confidence` — уверенность извлекателя; `verification_confidence` поднимает только версионируемая verification policy. Повтор факта моделью уверенность не повышает.
- Конфликт определяется по `kind + canonical_subject + scope`. Неразрешённый конфликт оставляет старую запись активной, а новую — pending; supersede происходит только по явному выбору пользователя и хранит причину из закрытого набора.
- Extraction выполняется после отправки ответа, поэтому не добавляет задержки к ходу задачи, а недоступность модели или валидатора не ломает задачу.
- Кандидата можно изменить до подтверждения или оставить только на текущую сессию (`ReviseMemoryCandidate`). Правка делает запись пользовательским утверждением и сбрасывает прошлую проверку, но ничего не подтверждает; session-only не создаёт persistent row и живёт до автоматического expiry.
- `forget` — logical deletion с tombstone из одних metadata и digest; он же вращает backup-контейнеры старше 7 дней, потому что стёртое утверждение остаётся в снимках, снятых до удаления.
- Модель извлекателя задаётся `EVOHIME_MEMORY_EXTRACTION_MODEL`; при отсутствии используется модель маршрута. Пользовательская файловая evidence сверяет полный content hash; `document` evidence дополнительно проходит published RAG generation + chunk hash + свежий file hash. Tool/API-валидация без replayable validator возвращает `unknown`, поэтому такие записи остаются pending, а не подтверждаются вслепую.

Часть команд renderer не доходит до Core и обслуживается main-процессом: `workspace.*`, `chat.*`, `provider.*`, `identity.get`, `repository.get`. Это локальное состояние оболочки, а не права: Core заново проверяет capability, policy и approval для каждой команды, которая до него доходит.

## Данные, диагностика и восстановление

SQLite находится в `%LOCALAPPDATA%\EvoHime` либо в `EVOHIME_DATA_DIR`. Миграции выполняются транзакционно; перед изменением схемы создаётся `.db.bak`. Журнал событий экспортируется в JSONL. Логи core и supervisor пишутся в `%LOCALAPPDATA%\EvoHime\logs`. Permission-правила читаются из `%LOCALAPPDATA%\EvoHime\permissions.json` как упорядоченный JSON-массив PolicyRule: побеждает последнее совпавшее правило, отсутствующий или пустой файл означает встроенный набор, пустой массив `[]` означает осознанное отключение правил. Обновление использует отдельный transaction worker, backup компонентов и recovery незавершённой транзакции перед запуском Core.

Локальное состояние оболочки лежит рядом, в `%LOCALAPPDATA%\EvoHime\shell\`:

| Файл | Содержимое | Ограничения |
| --- | --- | --- |
| `workspaces.json` | список запомненных папок и последняя выбранная | нормализованные пути |
| `chats.json` | чаты, привязанные к workspace, и отправленные промпты | 100 чатов на workspace, 500 сообщений на чат, 4096 символов на промпт |
| `provider.json` | выбранный провайдер, модель, base URL и зашифрованный ключ | режим `600`, запись через временный файл и `rename` |

Повреждённый файл не роняет оболочку: он читается как пустой.

## Бюджет запуска

`evohime_core::run_policy` описывает неизменяемый snapshot политики одного запуска: `max_iterations`, `max_wall_clock_ms`, `max_tool_calls`, `max_tokens`, `max_cost_micros` и `approval_required`. Core проверяет счётчики перед отправкой эффекта; превышение любого из них останавливает запуск с `BudgetExceeded`. Renderer может показать snapshot, но не может поднять лимит в середине запуска.

`evohime_supervisor::pulse` описывает контракт локального digest расписаний: dead-letter даёт `Failed`, пропуски и неуспехи — `Degraded`; успешный счётчик никогда не маскирует отказ. Модуль пока никем не вызывается: пользователь видит статус Pulse в `OperationsPanel`, где он выводится из событий `runtime.schedule_failed`/`runtime.schedule_dead_letter`.

## Ключ провайдера

Ключ вводится в `ProviderForm` и остаётся в main-процессе. Значение шифруется ОС через Electron `safeStorage` (DPAPI на Windows) и сохраняется в `provider.json`; renderer получает только summary с признаком `configured`. Core собирает model gateway из окружения при старте, поэтому сохранение ключа перезапускает supervisor вместе с Core, а pipe client переподключается к новой сессии. В окружение попадают только переменные выбранного провайдера, чтобы устаревший ключ второго не дошёл до gateway. Если ОС отказывается шифровать, ключ не записывается вовсе.

Base URL принимается только по `https` либо по `http` на loopback: ключ отправляется на этот адрес, и произвольный http-хост означал бы его утечку.

## Packaging и запуск

```powershell
.\scripts\build-windows-native.ps1
```

Для разработки используется `start-dev.ps1`; он читает `.env` по allow-list имён из `.env.example` и передаёт их только дочерним native-процессам. Для пользователя GitHub Actions собирает единственный `EvoHime-Setup.exe`. Установщик размещает внутренние `EvoHime.exe`, `evohime-core.exe`, `evohime-supervisor.exe`, `evohime-transaction.exe` и manifest в каталоге приложения и создаёт ровно один ярлык `EvoHime` на рабочем столе.

Пакет x64 предназначен для Windows 10 2004+ и Windows 11 и содержит bundled Electron runtime, Rust runtime и локальные компоненты; отдельная установка Node.js или браузера не требуется.

## Обновления из исходников

Обновление не связано с GitHub Release: клиент сравнивает коммит своей сборки с вершиной отслеживаемой ветки и пересобирает продукт на машине пользователя.

```text
update.json          репозиторий, ветка, launchPolicy, интервал проверки
%LOCALAPPDATA%\EvoHime\source           git checkout, которым владеет обновление
%LOCALAPPDATA%\EvoHime\update-staging   собранный пакет до подмены
%LOCALAPPDATA%\EvoHime\update-state     журнал транзакции и backup
```

- `evohime.build.json` рядом с бинарниками хранит коммит и ветку сборки; без маркера версия считается неизвестной и клиент пересобирается;
- коммит, не трогающий код клиента (документация, планы, CI-конфиг), не вызывает пересборку: клиент сравнивает установленный коммит с целевым через compare API и пропускает обновление, если ни один изменённый путь не влияет на сборку. Любая неопределённость — обрезанный diff, незнакомый путь, недоступный API — трактуется как «код менялся»: лишняя пересборка дешевле устаревшего клиента;
- обновление идёт только на зелёный коммит: перед сборкой клиент читает check-runs GitHub и берёт самый свежий коммит с пройденными проверками. Пока CI гоняет вершину ветки, берётся предыдущий зелёный коммит — иначе клиент отставал бы на каждый push; если зелёного нет в окне (`greenCommitDepth`, по умолчанию 10 коммитов) или проверки не читаются, обновление откладывается, а не выполняется вслепую. Отключается `requireGreenCommit`;
- проверка обновлений ходит в GitHub API с токеном пользователя, если он есть: анонимный лимит — 60 запросов в час на IP, и выбранный чужим трафиком с того же адреса лимит останавливает обновления с `403`, тогда как с токеном лимит 5000. Источники по убыванию явности: `EVOHIME_UPDATE_GITHUB_TOKEN`, поле `githubToken` в `update.json`, `GH_TOKEN`/`GITHUB_TOKEN`, `gh auth token`. Токен не обязателен — без него проверка работает как раньше; он уходит только на `api.github.com`, не пишется в логи и не сохраняется клиентом;
- git обновления работает без интерактива: сохранённые на машине учётные данные (`gh`, credential manager) используются, но ни git, ни credential helper не могут открыть диалог или спросить пароль в терминале. Зависшее за невидимым окном обновление хуже упавшего — оно блокирует запуск;
- `update.json` пишет установщик, репозиторий принимается только по `https`, ветка и интервал проверки нормализуются — конфигурация не может увести сборку на чужой источник или превратить проверку в busy loop;
- при запуске main-процесс проводит update gate до старта supervisor: собранный пакет нельзя подменить, пока Core держит файлы открытыми. Пользователь видит шаги пересборки и может нажать «Пропустить и запустить»;
- у уже запущенного клиента фоновая проверка собирает обновление в staging и предлагает перезапуск баннером, не прерывая работу;
- недостающие Git, Node.js, Rust и MSVC Build Tools ставятся через winget по фиксированным идентификаторам пакетов;
- локальная сборка падает транзиентно (оборванная загрузка Electron, недописанный `release/`), поэтому после первой неудачи производные каталоги удаляются и сборка повторяется один раз; вторая неудача показывается как есть. Полный вывод сборки лежит в `%LOCALAPPDATA%\EvoHime\logs\update-build.log` — UI показывает только последнюю строку;
- подмену выполняет `evohime-transaction.exe --apply-staging`: он копирует себя во временный каталог, дожидается не только выхода оболочки, но и момента, когда файлы установки действительно доступны на запись (дочерние процессы Electron держат их дольше), делает полный backup установки, переносит staging и при любой ошибке восстанавливает прежнюю установку. Копирование переживает блокировки повторами, а незавершённая транзакция откатывается при следующем запуске.

Неудачное обновление не блокирует работу: установленная сборка запускается как обычно, а причина отказа показывается в UI.

Безопасностные ограничения вынесены в [`../SECURITY.md`](../SECURITY.md).
