# EvoHime — доменный контракт: контекст, память, знания и навыки

Канонический контракт домена контекстного бюджета, памяти, RAG, чекпоинтов, навыков и знаний проекта. Обновлено: 2026-11-18.
Остов архитектуры: [../architecture.md](../architecture.md).

## Context Budget Manager

Сборка контекста реализована в `crates/context-budget` (контракты и детерминированная логика), `crates/evohime-local-storage` (ledger, scratchpad, artifact store, команды) и `crates/evohime-core/src/context_budget.rs` (интеграция в agent loop). Этот раздел — канонический контракт: исходный план удалён из `docs/plans/` после реализации, как того требует правило каталога.

**Контур.** Перед каждым model call Core выполняет `selection -> compress/offload -> финальная проверка бюджета -> событие ModelContext -> вызов модели`. Финальная проверка обязательна и выполняется до формирования события; при её невыполнении Core проходит оставшиеся уровни лестницы, а после их исчерпания завершает вызов через `BudgetUnavailable` без обращения к модели.

**Бюджет и профиль.** `ModelContextProfile` версионируется и выбирается по provider/model из каталога `crates/context-budget/profiles.json`, который можно перекрыть пользовательским конфигом того же формата. Профиль обязан удовлетворять правилам валидности `0 < target < soft < hard <= max`, `target + reserves <= soft` и `absolute_mvc_max_limit + reserves <= hard`; невалидный профиль отклоняется при загрузке, а неизвестная модель получает fallback-профиль (60% / 75% / 85% окна) и не может обойти эти ограничения. `target_tokens` — цель сокращения, `soft_limit_tokens` — порог его запуска, `hard_limit_tokens` — граница отказа; резервы считаются сверх контекста и не могут быть заняты историей или схемами.

**Обязательный минимум.** `minimum_viable_context` вычисляется детерминированно и всегда включает safety/system policy и текущий user prompt; approval semantics, незавершённый tool-call и cancellation добавляются при наличии таких состояний. Порядок частей фиксирован и задаёт как порядок в собранном контексте, так и выбор `missing_part`. Safety- и approval-часть не сокращается никогда: конфликт «safety не влезает в бюджет» разрешается отказом от вызова, а не урезанием safety.

**Лестница сокращения** конечна и упорядочена: L1 expired/duplicate/superseded, L2 low-priority optional, L3 самые старые завершённые tool outputs, L4 offload крупных item в artifact store, L5 сжатие истории, L6 отказ от необязательных резервов (`retry` → `streaming` → `tool_schema`; `tool_call` и `final_answer` не сокращаются никогда). Каждый уровень применяется не более одного раза и обязан строго уменьшать размер, поэтому цикл завершается всегда. Внутри уровня порядок детерминирован: pinned последним, затем по возрастанию `effective_priority`, `created_at`, `content_hash` и `id`. Недоступные artifact store или summarizer пропускают L4/L5 с diagnostic, а не роняют сборку.

**Отказ сборки.** `BudgetUnavailable` — терминальный результат со стадиями `mandatory_overflow`, `drops_exhausted`, `estimator_unavailable` и `provider_replan_failed`. Автоматический retry запрещён на всех уровнях; context-length error провайдера даёт ровно один deterministic re-plan с уменьшенным `hard_limit_tokens`, повторный отказ каскада не порождает. До UI отказ доходит bounded причиной с кодом, стадией, требуемым и доступным объёмом и указанием непоместившейся части — не молчаливым обрывом ответа.

**Оценка токенов.** Estimator версионируется и обязан быть консервативным: занижение считается дефектом. При недоступности основного используется fallback-estimator (`ceil(utf8_bytes / 2) + 16`) с порогами профиля, масштабированными на 0.70; при недоступности обоих сборка завершается отказом, а не оценкой по умолчанию. Оценка контента и tool schema кэшируется bounded LRU по `content_hash`, форме данных и версиям tokenizer, нормализатора и chat-template, поэтому смена любой из них не даёт стухший кэш-хит.

**`content_hash`** — SHA-256 в строчном hex от `normalizer_version`, разделителя `0x00`, `kind`, `0x00` и нормализованного содержимого. Текст нормализуется в фиксированном порядке: UTF-8, NFC, перевод CRLF и CR в LF, удаление завершающих пробелов в строках и завершающих пустых строк. JSON приводится к канонической форме с сортировкой ключей и фиксированным представлением чисел, двоичное содержимое хешируется как есть. Правила зафиксированы эталонными векторами в тестах, и версия нормализатора входит в hash input, а не только в кэш-ключ.

**`context_ledger`** — одна immutable запись на один model call. Её hash покрывает ids и порядок выбранных item, версии profile, tokenizer, нормализатора и стратегии, обязательный набор, отброшенные item с причинами, применённые compression- и ladder-решения, fallback-флаг и loadout. Hash считается один раз после фиксации состава и публикуется до вызова модели, поэтому потребители сравнивают его с записью, а не пересчитывают контекст. Фактический usage провайдера пишется в отдельную append-only таблицу `context_ledger_usage`, чтобы запись оставалась hash-стабильной. Ротация хранит записи моложе 30 дней или принадлежащие последним 200 сессиям и не удаляет записи, на которые ссылается неэкспортированный receipt.

**Scratchpad задачи** делится на `facts`, `open_questions`, `decisions`, `tool_findings` и `next_actions`. Внешний вывод инструмента помещается в `data_not_instructions` envelope и проверяется на prompt-injection; `confirmed` запись появляется только после provenance/policy-проверки Core, явного подтверждения пользователя или завершённой policy-операции — успешный tool result сам по себе фактом не становится. Подтверждённая запись не перезаписывается на месте, только новой ревизией. После restart в рабочий контекст возвращаются только `confirmed`; остальные изолируются как `recovered` с `trust=unverified` и пониженным приоритетом и удаляются через час или 10 шагов. При переполнении категории бюджета самые старые `confirmed` записи выгружаются в artifact store и остаются в контексте bounded ссылкой с hash и locator; `open_questions` и обязательный минимум не вытесняются, молчаливое усечение запрещено.

**Artifact store** адресует содержимое по `content_hash`: повторный offload переиспользует артефакт и добавляет ссылку, а не копию. Пространство имён per-task, доступ по locator ограничен задачей-владельцем и её детьми, чтение заново сверяет hash и помечает ссылку `invalid` при расхождении. Вытеснение идёт по TTL и последнему обращению; ссылка живого ledger entry или confirmed записи scratchpad помечается `expired` с сохранением hash и размера, а удалённое содержимое оставляет tombstone, который не считается доступным dedup-hit.

**Compression и pruning.** `duplicate` — совпадение `content_hash`, `superseded` — новая ревизия того же ключа при другом содержимом, `expired` — истёкший TTL или retention. Иерархия прав: safety и approval выше system instructions, те выше явных ограничений пользователя, далее confirmed facts, history и данные инструментов, ниже всего recovered и unverified. Recency и trust решают исход только внутри одного уровня. Summarizer — отдельный Core-вызов того же model gateway с собственным `summary_budget` и входным лимитом на prompt, без инструментов и без повторов; недоступность, превышение бюджета или невалидный результат дают deterministic fallback без каскадного повтора. Исходные item остаются source of truth, а summary хранит связь `summary_id -> source_ids`.

**Tool loadout.** Инструменты делятся на обязательные, read-only и mutation. Deterministic intent router нормализует prompt текущей задачи и активные `open_questions`, сопоставляет их с versioned таблицей capability keywords и применяет deny-правила. Явное mutation-action-слово (неотрицаемый keyword, которого нет в read-only правилах) важнее числа совпадений: запрос вида «найди, проверь и добавь переключатель» получает mutation-loadout, иначе модель видит только inspect-набор, а delivery gate требует отсутствующий `filesystem_write`. Без такого слова при конфликте правил выбирается более безопасный read-only результат, при неопределённом intent — read-only fallback loadout. Отрицание («не добавляй») распознаётся общим предикатом `contains_unnegated_keyword`, который используют и intent router, и классификатор требований поставки. История диалога не участвует в классификации: intent берётся только из prompt текущего запуска, иначе old read-only обмен понижал бы loadout нового mutation-запроса. Обязательные инструменты входят всегда и расходуют отдельный `mandatory_schema_reserve`, остальные ограничены `tool_schema_reserve`. Adaptive catalog добавляет к лексическому top-N уже авторизованные инструменты, без которых требование поставки невыполнимо (`filesystem_write`/`filesystem_patch`, `shell_execute`, `git_commit`); добавление никогда не расширяет authority, потому что берётся только из permission preflight. Permission preflight — источник истины по mutation: когда write-инструменты разрешены без approval (режим полного доступа), шаг получает их независимо от формулировки prompt (`mutation_authorized`), поэтому полный доступ в композере не ограничивает модель по словам запроса; каталог дополнительно сохраняет рабочий набор агента (filesystem list/read/search/write/patch/mkdir/stat, shell_execute, git status/diff/commit). В режиме подтверждения write-инструменты предлагаются при явном mutation-action-слове, иначе задача остаётся исследовательской (`task.delivery_mutation_skipped`). Требования, которые выданная сессия не может выполнить, не приводят к отказу: Core снимает их и пишет bounded trace `task.delivery_requirements_degraded`, а модель отвечает тем, что реально может сделать. Проверка без изменения и без явной команды тестов считается осмотром и не образует обязательство прогонять сборку; сама проверка распознаётся по фактической команде (cargo, dotnet, ctest, gradle, maven, npm/pnpm/yarn, pytest, go, `git diff --check`), а не по тексту prompt. Три одинаковых gate-запроса подряд означают, что модель не продвигается: Core перестаёт требовать продолжение и отдаёт пользователю её ответ, фиксируя невыполненные требования в `task.delivery_gate.stalled`. Permission и approval semantics выбранного инструмента остаются видимыми, а вызов вне loadout Core отклоняет до эффекта с bounded diagnostic `loadout_miss`: deny/intent-отказ остаётся policy denial, а вытеснение по `tool_schema_reserve` даёт исправимый `InvalidInput` (`tool_omitted_by_budget`). Системный prompt перечисляет и требует только инструменты из набора текущей сессии; вызов инструмента, которого в этом наборе нет, даёт исправимый `InvalidInput` со списком доступных инструментов (`tool.session_miss`), а не отказ политики и не остановку задачи. Код `tool_iteration_limit` попадает в redacted `task.failed` projection и показывается в UI вместо общего `task_failed`.

**Гарантия завершения.** Сверх лимита tool-итераций цикл делает один дополнительный tool-free вызов: инструменты в него не передаются, модель обязана вернуть текст, а tool call, напечатанный даже без схем, не исполняется и фиксируется в `task.iteration_limit_tool_calls_ignored`. Поэтому лимит шагов больше не завершает задачу красным `task.failed` без итога. Для задачи с обязательным изменением Core дополнительно считает успешные шаги без правки и на 6-м, 12-м и 18-м шаге добавляет в историю bounded guidance с требованием перейти к `filesystem_patch` или `filesystem_write` (`task.delivery_mutation_nudge`); после трёх подсказок они прекращаются, чтобы не вытеснять контекст. `filesystem_patch` объявляет `expected_hash` обязательным в объявленной схеме и runtime, а catalog probe передаёт для него placeholder: без этого probe не проходил preflight, инструмент молча выпадал из набора модели, и правка существующих файлов становилась недоступной даже в полном доступе. Core подставляет хеш последнего чтения того же логического пути, если модель опустила поле.

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

Semantic Repository Map из плана 86 закрыт evidence reuse этого canonical workspace index; отдельная graph database или второй authority не создаются.

## Checkpoint Forking & Replay v1 (план 89)

Fork создаёт отдельную Core-owned lineage запись (schema 1, storage 87) из
validated checkpoint boundary с новой run identity, workspace fingerprint и
bounded overrides. Source immutable; `effects_replayed=false`, внешние
side effects автоматически не повторяются. IPC tags 241/86 дают typed
fork request и metadata-only projection.

## Memory Extraction

Извлечение фактов из диалога реализовано в `crates/evohime-core/src/memory_extraction.rs`. Этот раздел — канонический контракт поведения и границ подсистемы; подтверждённый статус текущего checkout указан в [`current-state.md`](../current-state.md).

- Единственный владелец extraction, policy, validation и storage — Core. Всё, что вернула модель, — это candidate, а не память.
- По умолчанию работает `strict`-режим: извлечение запускается только после явного триггера пользователя («запомни», «важно», «ограничение» и эквиваленты). Режим переключается переменной `EVOHIME_MEMORY_EXTRACTION` (`disabled` | `strict` | `open`); в `open` результат всегда получает `pending_confirmation`. Даже при `disabled` ручной триггер продолжает работать.
- `constraint`, `decision`, любой high-risk, `sensitive` privacy, неоднозначный subject, недостаточный confidence и незавершённая проверка дают `pending_confirmation`. Автосохранение возможно только для low-risk предпочтения, подтверждённого явным утверждением пользователя. Секреты не сохраняются вообще.
- `model_confidence` — уверенность извлекателя; `verification_confidence` поднимает только версионируемая verification policy. Повтор факта моделью уверенность не повышает.
- Конфликт определяется по `kind + canonical_subject + scope`. Неразрешённый конфликт оставляет старую запись активной, а новую — pending; supersede происходит только по явному выбору пользователя и хранит причину из закрытого набора.
- До отправки `TaskCompleted` Core проверяет eligibility и durable-записывает metadata-only ссылку на источник и его ревизию; само извлечение запускается после ответа. Поэтому потеря процесса не оставляет принятую extraction intent без записи, а недоступность извлекателя не ломает основной ход.
- Диалоговый и ambient-пути используют общий cancellation-safe RAII `ExtractionLease` на экземпляр Core-agent. Lease захватывается до async-вызова извлекателя и освобождается через `Drop`, поэтому конкурентный второй запуск не проходит даже при отмене первого; он получает bounded `ThrottleReason::Reentrant` и пишет только `memory.extraction.skipped` либо `memory.ambient.skipped` с причиной `reentrant`.
- Typed `MemoryExtractionOrigin`, root execution reference и конечная глубина принадлежат extraction-контракту Core; storage отвергает неизвестные origin, пустую lineage и глубину выше limit, а restart recovery повторно проверяет lineage для dialog и ambient источников. Неизвестный origin из SQLite fail-closes при чтении.
- Публикация кандидата выполняется владельцем Memory storage одной SQLite-транзакцией: additive schema v176 хранит metadata-only source/lifecycle, stable candidate-slot, source order и ожидаемую head/revision; вставка `memory_entries` и `committed` остаются одним CAS эффектом. Повтор ключа возвращает прежний `memory_id`, а более старый source basis не перезаписывает новый head; statement, transcript, prompt, URL и secrets в extraction lifecycle не сохраняются.
- Перед каждым auxiliary extractor dispatch Core durable-коммитит Model Request Provenance v1 envelope, receipt и dispatch marker. Каждый retry получает новый request id под общей logical request; failure provenance закрывает dispatch с bounded `provenance_unavailable`. Response provenance фиксируется до capture candidate. Extractor получает только restricted policy context и не получает generic tools.
- Startup recovery проходит через существующую bounded background-reconciliation точку. Dialog source rehydrate-ится только из единственной bounded пары `TaskStarted`/`TaskCompleted`, её source revision сверяется с сохранённым digest; duplicate, oversized, missing или mismatched данные закрываются typed outcome.
- Metadata-only event `memory.extraction` показывает lifecycle diagnostics в read-only `OperationsPanel`; Core projection и renderer ограничивают enum-поля и размеры, исключая memory body, prompt и model output. Unknown major/field values fail closed, существующие memory controls и forget semantics остаются владельцами изменения данных.
- Кандидата можно изменить до подтверждения или оставить только на текущую сессию (`ReviseMemoryCandidate`). Правка делает запись пользовательским утверждением и сбрасывает прошлую проверку, но ничего не подтверждает; session-only не создаёт persistent row и живёт до автоматического expiry.
- `forget` — logical deletion с tombstone из одних metadata и digest; он же вращает backup-контейнеры старше 7 дней, потому что стёртое утверждение остаётся в снимках, снятых до удаления.
- **Ambient как источник кандидатов.** `SourceTrust::Ambient` — пятое значение доверия, строго более слабое, чем остальные: `can_ground_strict_save()` для него ложно, `requires_validation()` истинно, а `evaluate` возвращает `pending` с причиной `ambient_never_auto_confirms` сразу после проверки на секрет — раньше любых порогов, kind и scope. Перебор всех комбинаций `kind × scope × privacy × confidence × subject` показывает, что `AutoConfirm` для ambient недостижим. Секрет по-прежнему отвергается раньше: услышанный ключ не сохраняется даже в pending.
- **Ambient-точка входа отдельная.** `run_memory_extraction` принимает пару (реплика пользователя, ответ агента) одного хода, поэтому речь входит своим путём `run_ambient_memory_extraction`, а не подделанным «ходом»: подмена реплики пользователя ambient-текстом сломала бы смысл `user_asserted`. Триггером служит закрытие эпизода, `TurnContext::ambient` несёт `user_asserted = false`. Диалоговый `check_can_extract` не ослаблен ни в одной ветке — он отверг бы ambient как `no_explicit_trigger`, поэтому у ambient свой гейт `check_can_extract_ambient`: circuit breaker общий, а режимы и бюджеты свои.
- **Режим и бюджеты ambient.** `EVOHIME_AMBIENT_MEMORY` (`off` | `pending`, по умолчанию `pending`); аналога `open` нет, а неизвестное значение даёт `off`, а не молчаливое включение. Общий выключатель старше частного: при `EVOHIME_MEMORY_EXTRACTION=disabled` ambient-извлечение не запускается вовсе. Бюджеты отдельные — 6 кандидатов и 12 эпизодов в час и собственный часовой лимит токенов; переполнение даёт `ambient_candidate_limit`/`ambient_episode_limit`, а не `hourly_limit` диалогового пути, иначе два троттлинга стали бы неразличимы в трассах.
- **Что принимается из речи.** Только `preference`, `entity` и `lesson`. `constraint` и `decision` отбрасываются до persistence: они влияют на действия, и ошибиться в них слишком дорого. `session_summary` не принимается потому, что у речи нет диалоговой сессии, а session-note не проходит через очередь подтверждения. Говорящий — всегда `unverified`: диаризации и голосового профиля в v1 нет намеренно, потому что голосовой шаблон это биометрия, а ошибка диаризации приписала бы пользователю чужое утверждение. Если в высказывании распознан субъект не в первом лице, `privacy_class` поднимается минимум до `sensitive`, что даёт pending и скрытое тело записи. Валидатора у речи нет, поэтому `validation_status` — `unknown`, а не `valid`.
- **Связь с эпизодом.** `provenance_source_id` ambient-кандидата — это `episode_id`: `RawEvidenceLocator` получил поле `episode_id`, и `memory_provenance_source_id` проверяет его первым. Именно по этому значению `ambient_store` при удалении эпизода переводит его кандидатов в `rejected` с причиной `source_deleted`; без этой правки условие осталось бы холостым. `content_hash` для ambient пуст: по правилу 04.1 хеш короткой фразы приравнивается к её содержимому. Живут ambient-записи в собственном scope `workspace/ambient` — речь у стола не принадлежит рабочему каталогу, — но очередь подтверждения одна: `ListMemoryPending` добавляет их к записям текущего воркспейса, а `OperationsPanel` показывает бейдж «услышано», подпись «говорящий не подтверждён» и фильтр по источнику.
- Модель извлекателя задаётся `EVOHIME_MEMORY_EXTRACTION_MODEL`; при отсутствии используется модель маршрута. Пользовательская файловая evidence сверяет полный content hash; `document` evidence дополнительно проходит published RAG generation + chunk hash + свежий file hash. Tool/API-валидация без replayable validator возвращает `unknown`, поэтому такие записи остаются pending, а не подтверждаются вслепую.

Часть команд renderer не доходит до Core и обслуживается main-процессом: `workspace.*`, `chat.*`, `provider.*`, `identity.get`, `repository.get`. Это локальное состояние оболочки, а не права: Core заново проверяет capability, policy и approval для каждой команды, которая до него доходит.

## Typed memory, retrieval и compaction

Memory extraction хранится в schema v31. `memory_entries` сохраняет bounded
`record_version`, JSON-массивы `evidence_refs` и `execution_event_refs`; Core
записывает их до публикации retrieval. `privacy_class=secret` отвергается до
SQLite. Retrieval выполняется через Core-owned adapter: scope/privacy
проверяются до ranking, порядок детерминирован по score/freshness/id, citations
проверяют evidence и generation и не могут пересечь workspace scope.

Forget — логическое удаление с обезличенной tombstone-строкой: содержание,
provenance, evidence refs и execution refs очищаются, а digest tombstone
сохраняется для аудита. Повторный forget не создаёт новую tombstone.

Context compaction имеет SQLite-backed operation state machine
`planned/running/cancelled/committed/failed`. Уникальный operation key
защищён SQLite, а versioned projection и provenance linkage хранят snapshot
revision, summarizer version и связь item с `sequence_id`. UI получает только
bounded metadata/projection; исходная execution/evidence история не удаляется
compaction-операциями.

TaskCheckpoint v1 добавляет bounded immutable continuity record поверх этих
источников. Контракт живёт в `evohime-core::task_checkpoint` и
`evohime-local-storage::task_checkpoint`, сериализуется детерминированным JSON
и получает SHA-256 `content_hash` без самого hash-поля. Core-derived evidence
(завершённые пункты, blockers, файлы, тесты, gates, approvals и typed refs)
отделено от model-proposed remaining/open/next/narrative/semantic decisions;
модель не может подтвердить effect, approval, test или completion.

Migration v32 добавила SQLite storage для TaskCheckpoint: она хранит только
canonical JSON и bounded metadata в append-only `task_checkpoints`. Повторная запись того же id и payload
идемпотентна, другая запись с тем же id отклоняется. Parent checkpoint обязан
принадлежать тому же workspace и иметь более ранний event sequence. Чтение
`latest_valid` пропускает повреждённую последнюю запись и возвращает предыдущую
валидную для последующего replay; paths, secret-shaped text, secret refs,
traversal и превышение лимитов отклоняются до persistence.
Публичный `TaskCheckpointV1` сериализуется, но не принимает произвольный JSON:
storage декодирует запись через приватный wire-тип и сверяет все SQL metadata с
canonical JSON. Parent projection также проходит документированную таблицу
допустимых state transitions; нарушения возвращаются стабильными typed error
codes, а не строковым контекстом.

Runtime использует этот контракт в Core-owned `TaskCheckpointRuntime`: перед
получением durable agent lease выполняется bounded recovery, затем сохраняется
checkpoint `run_started`. Перед крупной compaction и после проекции контекста
сохраняются throttled checkpoints с hash-ссылкой на ledger; terminal, paused и
conflicted результаты также фиксируются до публикации финального состояния run.
Recovery возвращает только bounded metadata event replay и не повторяет внешний
effect вслепую: усечённое окно, неизвестный outcome, stale/conflicted/blocked
checkpoint и повреждённая цепочка требуют явной reconciliation. Событие
`task.checkpoint.saved` содержит только идентификатор, hash, причину и sequence,
без prompt, ledger payload или другого сырого содержимого.

Desktop surface этапа 23.3 остаётся additive в `desktop-ipc-v1`: команды
`GetTaskCheckpoint` и `ResolveTaskCheckpoint` используют tags 137–138, а
`TaskCheckpointProjection` и `TaskCheckpointActionResult` — typed EventEnvelope
oneof tags 15–16. Проекция содержит bounded counts, статус, blockers, typed refs,
policy id, recovery disposition и event-type metadata; prompt, credentials,
hidden reasoning и raw payload отсутствуют. `ResolveTaskCheckpoint` принимает
только `acknowledge_recovery` или `request_resume`, проверяет checkpoint id и
`source_event_seq`, пишет action вместе с `command_dedup` одной SQLite-транзакцией
и никогда не запускает внешний effect. Electron main/preload только валидирует
форму и пересылает команды, а renderer отображает projection и посылает явное
действие; reconnect/resync продолжает использовать существующий sequence cursor.

### Workspace State Checkpoints (plan 58)

Workspace file state is a separate Core-owned contract from `TaskCheckpointV1`.
`evohime_core::workspace_state_checkpoints` captures only bounded regular files
under a canonical workspace root, excludes VCS metadata and build/dependency
directories, rejects symlink/reparse entries and validates SHA-256 hashes before
use. The defaults are 4096 files, 64 MiB total, 1 MiB per file, 512 path bytes
and 128 components. Snapshot metadata and restore journal tables are additive
schema v57; bytes are not placed in the renderer or user `.git`.

The existing build snapshot IPC remains a compatibility transport. The
dedicated additive `workspace_state_checkpoint` command (desktop IPC tag 209)
exposes create, compare, workspace restore, task-projection restore and
combined restore. Electron shows it only in the collapsed developer interface;
the renderer receives bounded metadata and never reads files. Restore is
preflighted, journaled and conflict-safe: user/external edits produce a typed
conflict and no overwrite. Task restore is an independent projection action;
combined restore executes both explicit actions. Neither path mutates
credentials or external effects.

### Agent Skills

Agent Skills v1 — Core-owned локальный registry для bounded `SKILL.md`. Для
workspace используются roots `.agents/skills`, `%APPDATA%/EvoHime/skills`,
совместимые `.codex/skills`/`.claude/skills` и bundled resources; явный session
root доступен через тот же registry API. Приоритет детерминирован: explicit,
project-native, global, compatibility, bundled. Collision не скрывается:
победившая запись выбирается по приоритету, а диагностика остаётся bounded.

Discovery читает только bounded frontmatter и metadata; тело и
`references/` загружаются только отдельной typed-командой. Пакеты с unsafe
path, prompt-injection/secret-shaped metadata, запрещёнными executable или
permission-полями, неизвестной схемой и превышением лимитов становятся
`invalid` и не исполняются. Capability grant — пересечение с родительским
grant; требование вне него возвращает `capability_escalation`. Registry не
запускает scripts, install/network hooks или model invocation и не сохраняет
тело skill: cache живёт только в процессе Core, а durable trace содержит лишь
skill id, version, hash и source reference.

Skill Trust Pipeline v1 находится между discovery и enable. Offline scanner
`skill-scanner-v1` выдаёт стабильные finding codes, severity и только masked
SHA-256 fingerprints. Trust record keyed by `skill_id + content_hash +
scanner_version + review_policy_version`; изменение package или policy не
наследует старое решение. `SkillRegistry::load`, `load_reference` и
permission selection отклоняют всё, кроме `trusted`/`enabled`, через
Core-owned gate. Contextual review — optional read-only typed report; malformed
или unavailable report не повышает доверие. Metadata records и audit хранятся
в SQLite schema 48, raw body/secrets/credentials не сохраняются.

В `desktop-ipc-v1` добавлены additive commands `ListSkills`, `LoadSkill` и
`LoadSkillReference` (tags 139–141), а typed projections
`SkillCatalogProjection`, `SkillContentResult` и `SkillReferenceResult` имеют
oneof tags 17–19. Electron main валидирует bounds и traversal, renderer
показывает metadata и запрашивает полный документ только по явному клику;
generic payload и authority в UI не используются.

### Chat-scoped Goals

Goal v1 — Core-owned progress projection, привязанная к одному чату. Она может
переживать отдельные model turns и перезапуск Core, пока существует связанная
история чата, но не является постоянной workspace/user goal. Контракт и `GoalStore` находятся в
`evohime-local-storage::goal`, runtime facade — в `evohime-core::goal`; общая
SQLite schema — v33. Projection хранит только bounded objective, criteria,
статус, progress, blockers, next action, budgets и ссылки на workflow/child/
checkpoint. Workspace path используется только как входной scope selector и
преобразуется в стабильный SHA-256 `workspace_id`; сам путь, credentials,
capabilities, prompt и hidden reasoning в Goal не сохраняются. Link-команда
принимается только для существующего Core-owned runtime-объекта; отсутствующий
optional backend даёт typed `reference_not_found`, а не успешную ссылку.

Изменения objective/criteria получают новую immutable revision и append-only
event. Повтор команды с тем же idempotency key возвращает прежний typed result,
а устаревший `expected_version` отклоняется. `Completed` разрешён только после
Core-подтверждения всех criteria с evidence ref и verifier identity/version; для
manual criterion клиент передаёт только явное решение пользователя, а
evidence/verifier mint-ит Core. Текст модели или completion одного workflow/
child этого не делает. Исчерпание доступного бюджета представляется статусом
`BudgetLimited`, а recovery сверяет связанные durable ссылки, выдаёт только
bounded warning и не повторяет неизвестный внешний effect.

В authenticated `desktop-ipc-v1` Goal добавлен additive-комплект команд
`CreateGoal`, `GetGoal`, `ListGoals`, `GoalAction`, `UpdateGoal`,
`VerifyGoalCriterion` и `LinkGoalReference` (tags 142–150). Typed
`GoalProjection`, `GoalListProjection` и `GoalActionResult` используют
EventEnvelope oneof tags 20–22; список ограничивается фактическим protobuf
размером и сообщает `projection_truncated` до записи во frame. `ListGoals`
обязательно принимает `chat_id`; Core и SQLite возвращают только цели именно
этого workspace/chat. Создание из агента без активного чата отклоняется.
Electron main/preload только проверяет форму и маршрутизирует команды; цель
отображается в панели соответствующего чата, а общий Overview больше не
показывает workspace-список целей. Goal не является
scheduler: автоматического создания или продолжения задач из одного сообщения
нет.

## Resumable Conversation Event Log v1 (план 49)

История диалога принадлежит Rust Core и хранится в SQLite schema v50 отдельно
от глобального transport/audit-журнала. `conversation_id` имеет собственный
монотонный `sequence`; envelope v1 содержит UUID события, kind/category,
correlation/causation/task/run/turn refs, persistence class и sensitivity.
Authoritative payload остаётся в Core, а renderer получает только bounded
64 KiB projection после Sensitive Data Guardrails. Timestamp не используется
как cursor.

`StartTask` принимает additive `conversation_id` и стабильный
`client_message_id`. Core одной транзакцией записывает
`user_message_accepted`, task binding и SHA-256 content hash до dispatch.
Повтор того же id/hash возвращает прежнее принятие и не запускает вторую
задачу; повтор с другим content hash даёт typed `idempotency_conflict`.
Task/model/tool/approval/child/workflow/storage события проецируются в общий
conversation log. Streaming delta имеет `transient_stream`, а завершение или
ошибка создают отдельное durable finalized/failed событие. Tool projection
различает command/file/browser/tool, child payload ограничен summary-полями,
usage содержит purpose/source для раздельной агрегации.

History API поддерживает mutually exclusive before/after cursor, limit 1…200,
bounded kind filter, `has_older`/`has_newer` и retention metadata. Логическая
compaction фиксирует snapshot ref/hash и сдвигает earliest available sequence;
старый cursor получает `cursor_expired`, а audit-строки физически не удаляются.
Authenticated IPC использует commands 197–198 и event 49 без изменения major.
Subscribe сначала возвращает catch-up page после `after_sequence`; дальнейшие
typed live events идут существующим journal tail и дедуплицируются по event id.

Для нового задания из существующего чата Core читает сообщения строго до
sequence текущего принятого запроса: последние 40 durable `user_message_accepted`
и `assistant_message_finalized`, суммарно не более 64 KiB. Текущий запрос
добавляется отдельно последним user-сообщением; предыдущие реплики сохраняют
свои роли и проходят через обычный context budget/compression. Tool/status/
usage events и streaming deltas в историю модели не включаются. Ошибка чтения
истории завершает задачу с ошибкой, а не запускает модель без запрошенного
контекста. Codex CLI получает ту же ограниченную историю как сериализованный
JSON conversation transcript перед текущим запросом.

Electron main/preload только валидирует и маршрутизирует contract. Чистая
`conversation-projection` обнаруживает gap/conflict, игнорирует exact duplicate,
reconciliate-ит optimistic bubble только по `client_message_id`, агрегирует
usage и batch-ит delta исключительно для отображения. `TaskTimeline` сначала
показывает bounded `chats.json` presentation cache, затем заменяет его Core
history, продолжает cursor catch-up и показывает sending/retry/failed. При
переключении conversation projection сбрасывается; global Core events остаются
compatibility fallback. Renderer не читает SQLite, не решает recovery и не
запускает effect. Загруженные history pages и текущий `subscribed`/`live` поток
хранятся в projection раздельно: новые live events не вытесняют уже загруженную
историю. Пользовательский timeline рендерит перемещаемое окно до 80 строк с
overscan, сохраняет якорь `scrollTop` при prepend старой страницы и не меняет
позицию чтения при новых событиях, если пользователь ушёл от нижнего края.
Для renderer event-поток обрабатывается курсором новых элементов: страницы и
вложенные conversation events индексируются по стабильным ключам и ID, поэтому
изменение глобального массива из другого чата не повторяет применение всей
истории. Подписка управляется отдельным effect только при смене чата или
восстановлении соединения.

Shell группирует чаты по `workspacePath` и показывает только историю выбранной
папки. При смене workspace активный чат закрывается до следующего запроса;
нельзя продолжить conversation одного проекта с путём другого. Core связывает
`conversation_id` с производным scope переданной workspace-папки и отклоняет
несовпадение. Модельная история остаётся внутри conversation, а проектная память
и RAG читаются в scope текущего workspace.

### Memory Governance v1

План 50 закрыт 1 сентября 2026 года. Durable
`evohime-local-storage::memory_store::MemoryRecord` остаётся единственной
персистентной authority; Core `memory_domain` используется только как
bounded validation/DTO-слой. `MemoryExtractionFields` дополнен typed
governance metadata: `authority` (`user_asserted`, `system_defined`,
`model_proposed`, `imported`), `durability` (`ephemeral`, `session`,
`durable`) и bounded independent `confidence` в диапазоне 0..=1.

`MemoryWriteGate` в `crates/evohime-core/src/memory_governance.rs` выполняет
fail-closed проверку непосредственно перед каждым Core durable insert:
неизвестная authority/durability, secret, ephemeral/session bypass,
невалидная confidence и непроверенная model/imported запись не доходят до
SQL. Reinforcement требует минимум двух различных evidence refs. Existing
pending-confirmation, dedup/conflict, approval, provenance, retention и
tombstone semantics сохраняются; retrieved metadata не расширяет capability.

Storage schema — v51, миграция additive и backup-before-migrate выполняется
общим storage ladder. Legacy rows получают user-confirmed durable defaults,
а extraction/ambient candidates сохраняются как `model_proposed`. Existing
authenticated memory IPC commands и metadata-only Electron OperationsPanel
показывают governance metadata; raw body/provenance payload, credentials и
hidden reasoning не передаются в renderer. Restart/recovery не повторяет
внешние эффекты вслепую.

### Memory Views & Adaptive Recall v1 (план 96)

План 96 закрыт 3 сентября 2026 года. `memory_views_and_adaptive_recall`
добавляет Core-owned hierarchical `LogicalMemoryScope` и versioned
`MemoryView`: view содержит только разрешённые logical roots, bounded depth и
раздельные `read`/`write` права. Shared read-only view валиден (`read=true,
write=false`), write без read запрещён; scope traversal проверяется Core и не
использует path naming как ACL.

`RecallMode` поддерживает `Shallow`, `Deep` и `Auto`; Auto выбирает глубину по
typed `QueryComplexity`, а Deep дополнительно ограничивается `view.max_depth`.
`rank_candidates` отбрасывает scope вне view до вычисления score и возвращает
детерминированный explainable composite из lexical/freshness/provenance.
Каждый recall фиксирует `read_barrier_generation`, задавая явную consistency
boundary для background ingestion. Durable view revisions и recall barriers
хранятся аддитивно и optimistic/idempotency-fenced в
`memory_views_and_adaptive_recall_*` tables. Authenticated command 244/event 89
и Electron developer panel показывают только bounded metadata, причины,
scope IDs и score breakdown; raw memory body, prompt, credentials и hidden
reasoning не пересекают IPC.

**Causal Collaboration Bus v1 (план 51).** Core-owned typed messages are
scoped to an active TeamSession and its pinned protocol hash. Sender identity
and peer routes are derived by Core; the bus extends the retained-child
sequence substrate with metadata-only `collaboration_messages` in schema v52.
Payloads are bounded to 32 KiB, each session has at most 128 pending messages,
and subscriptions are ephemeral. Authenticated IPC commands 199–200/event 50
and the Electron panel expose only redacted metadata. Compare-and-set delivery
and terminal `unknown` recovery prevent blind retries; subscriptions do not
grant artifact, tool or provider authority.

## Revision-Safe Workspace Files v1 (план 60)

Core-owned file tools используют typed namespaces `uploads`, `workspace`,
`outputs` и run-scoped `scratch`. `FileRef` публикует только logical path,
namespace, размер, SHA-256 content hash и revision; host path не входит в
tool/UI projection. Read/write/patch/delete/move/copy проходят через общую
canonical containment boundary с bounded payloads и traversal/reparse checks.
Existing-file mutations требуют expected hash; fuzzy patch fallback удалён,
uploads immutable. Внешнее изменение обнаруживается при следующей mediated
operation и даёт stale outcome. UI surface показывает только bounded ref/preview
по project scope; мутации остаются в approval-пути Core tools.
После filesystem_read Core передаёт модели санитизированный preview и точные
content_hash, ready-to-copy expected_hash и revision. Для filesystem_write и
filesystem_patch Core подставляет хеш последнего успешного чтения того же
логического пути, если модель пропустила поле; внешний конфликт всё равно
отклоняется проверкой хеша на границе записи. Оба mutating-инструмента возвращают
новый content_hash и revision после успешного изменения.

## Workspace Bootstrap Manifest v1 (план 64)

Core принимает versioned `WorkspaceBootstrapManifest` только через
authenticated additive IPC command 215/event 61. Manifest имеет bounded schema,
SHA-256 content hash и явное описание шагов, executable identity,
idempotency и network requirement. Discover читает только
`.evohime/bootstrap.json` из уже выбранного проекта; Core повторно проверяет
workspace scope, относительные пути и размер.

SQLite schema v62 хранит manifest, trust decision и preparation cache по
`workspace_id + manifest_hash + fingerprint`. Новый hash/fingerprint не
наследует доверие или результат. Запуск возможен только для exact trusted hash;
running lease single-flight, истёкший lease переводится в `unknown_outcome`, а
повтор подготовленного exact cache возвращает bounded metadata. Процессы идут
через существующий `ExecutionPolicyProfile`/Job Object boundary, с timeout,
cancel-on-drop и scrubbed environment; network effects и неподдержанные file
effects fail closed. Electron показывает только developer metadata projection.

## Experience Replay Library v1 (план 68)

Core-owned `ExperienceRecord` хранит bounded episodic trajectory summaries,
typed outcome и evidence-backed score отдельно от Memory/Refinement. Write Gate
требует evidence, scope, redaction, hash и не принимает `UnknownOutcome`;
credentials/raw outputs и chain-of-thought не сохраняются. Опыт хранится в
SQLite schema v66 с duplicate-safe записью. Bounded context projection остаётся
untrusted advisory; Core сохраняет scope/retention policy и не расширяет
capabilities. Authenticated additive IPC command 219/event 64 и Electron
developer panel показывают только metadata/action projection.

## Typed Context References v1 (план 75)

Core-owned typed resolver принимает только bounded built-in reference kinds
для file/folder/workspace, git, diagnostics, terminal, artifact/task/plan,
goal/workflow, browser snapshot и read-only URL. Resolver валидирует scope,
safe locator, SSRF/path boundary, exact revision/hash и sensitivity; refs не
содержат raw content и не расширяют capabilities. Lazy projection budget
детерминированно отбрасывает или откладывает контекст сверх лимита.
Durable reference metadata хранится в SQLite schema v72, authenticated
additive IPC использует command 226/event 71, Electron показывает только
metadata/chip projection.

## Project Instruction Stack v1 (план 80)

Project Instruction Stack — Core-owned bounded registry и compiled snapshot
проектных инструкций. Разрешённые источники: глобальные правила из
`%APPDATA%/EvoHime/rules`, workspace `.evohime/rules/*.md`, вложенные
`AGENTS.md` и корневой compatibility `AGENTS.md`. Markdown рассматривается
как untrusted text: frontmatter не расширяет tools, capabilities, grants,
credentials или approvals, fenced code не исполняется, а security policy и
Skills остаются отдельными authoritative surfaces.

Discovery ограничен 64 файлами, 64 KiB на файл, 256 KiB snapshot и 16 384
оценочными токенами; canonical workspace paths, symlink/reparse escape,
malformed metadata и переполнение бюджета дают typed diagnostics без silent
truncation. Activation поддерживает Always, RelevantPath и Explicit,
детерминированный precedence/ordering, content hash и source revision.

SQLite schema v76 хранит registry, immutable snapshots и idempotency results.
Authenticated IPC command 230/event 75 и Electron developer panel передают
только bounded metadata projection: active rules, source/scope, why active,
hash/revision, diagnostics и budget. Перед model call Core pin-ит snapshot
hash, добавляет bounded untrusted project-instruction context и включает
rule hash/revision в model provenance; authority не расширяется.

Plan 111 confirms this stack as the Project Guidance Registry compatibility
surface: `ProjectGuidanceDocument` is an alias over the canonical rule record,
trust is derived from the allowlisted source and sensitivity class, and the
compiled snapshot has a stable Prompt Cache segment. This adds no second store
or authority and keeps guidance below system/user security policy.

## Workspace Sets v1 (план 100)

Workspace Set — Core-owned bounded объединение до 8 самостоятельных roots без
фиктивной общей filesystem/VCS transaction. Каждый root имеет уникальные
`root_id`/alias, canonical binding, RootKind, независимые grants, VCS state и
revision identity. Root-qualified `WorkspaceResourceRef` и bounded cross-root
search не допускают parent escape, implicit соседний root или доступ без grant;
read-only reference root не может получить write grant.

Durable SQLite schema v77 хранит versioned set definitions, idempotency results
и pinned `WorkspaceSetRunBinding` snapshots. Bind проверяет enabled roots и
set version, сохраняет exact set/root hash и восстанавливает binding после
restart; независимые roots не объявляются atomic. Authenticated additive IPC
command 231/event 76 предоставляет create/get/update/bind/search, а Electron
Workspace Sets panel получает только bounded redacted projection.

Лимиты: 8 roots/set, 128 bytes IDs/aliases, 260 bytes path hint, 64 KiB
definition, 256 KiB binding/event/evidence, 32 grants/refs per root и 1 024
search matches per root. Imported config требует явного user mapping; rules,
skills и diagnostics сохраняют source-root provenance и не расширяют authority.

## Knowledge Source Registry v1 (план 101)

Knowledge Source Registry — отдельный от Memory Core-owned слой reference
corpus. `KnowledgeSource` имеет typed kind/status lifecycle, origin/fingerprint,
sensitivity/trust и content revision; `KnowledgeBinding` связывает источник с
project, role, workflow, team protocol или session без расширения grants.
`KnowledgeView` строится Core из Ready и authorized bindings, а
`KnowledgeHit` сохраняет source revision, chunk id, locator, freshness и
retrieval provenance. Knowledge hit автоматически не становится Memory.

SQLite schema v78 хранит sources, bindings, manifests и bounded chunks с
monotonic revisions. Core операции register/get/bind/index/retrieve используют
provider-neutral keyword path, Ready/sensitivity authorization и typed failure;
parser scripts/macros и embedded fetch не исполняются. Authenticated IPC
command 232/event 77 и Electron Knowledge Sources panel передают только
redacted metadata; raw chunks/credentials/prompts/outputs остаются Core-owned.

План 109 добавляет поверх этого registry `KnowledgeCollection`: versioned
bounded список source IDs с retrieval profile и scope. Коллекция не копирует
источники или chunks; Core проверяет уникальность, существование источников и
монотонную версию, а `collection_register`/`collection_get` возвращают только
redacted metadata через существующую authenticated surface. Parser/fetch,
vector backend и отдельная UI authority не добавляются.

## Context Mentions v1 (план 82)

Explicit user-authored `@` mentions проходят Core lexer и переиспользуют
Typed Context References v1: file/folder/git/diagnostics/terminal/artifact
refs, canonical revision/hash и bounded projection. Imported/tool/model text не
разрешается автоматически; mention не расширяет capabilities и unknown syntax
не регистрирует resolver. Resolver остаётся Core authority, renderer получает
только chips/projection.

## Grounded Research Workspace v1 (план 120, реализован 2026-09-09)

Grounded Research хранит только bounded metadata и lineage. `ResearchSourceRevision`
фиксирует immutable content hash, origin snapshot, parser/index profile, typed
trust/status и locator root; bytes остаются у ArtifactStore либо workspace-RAG.
`EvidenceItem` обязан ссылаться на Ready revision, typed human-openable locator
и тот же hash revision. Legacy `ResearchEvidence` и `RunResearchFetch` остаются
совместимым direct-URL adapter и не создают новый artifact без этой цепочки.

Существующая `KnowledgeCollection` остаётся владельцем коллекций. SQLite schema
v95 добавляет revision/evidence/session/subtask/claim/citation/conflict/artifact/
delta metadata, foreign keys, bounded JSON и immutable artifact triggers; новая
таблица collections не создаётся. Ошибки lineage и immutable revision
отвергаются fail-closed. Accepted artifact передаётся через существующий
`artifact_handoff_registry_store`, без второго content store.

Bounded runner выполняет `query → policy-gated selected results → HTTPS fetch →
immutable revision/evidence → bounded extractive summary`, ограничен budget и
поддерживает cancellation/partial coverage. `research_session_run` проходит
через Core actor и единый `NetworkCapabilityPolicy`; startup переводит активные
research sessions в `interrupted`. Electron получает metadata-only projection
через authenticated `KnowledgeSourceRegistry` transport.

## Content-Aware Context Compression v1 (план 123, реализован 2026-09-09)

Compression добавляет Core-owned typed classifier, loss class, compact block,
omitted regions и bounded recovery request поверх существующего Context Budget
Ledger. Source hash/ref остаются exact lineage, raw source не копируется в новый
store. Deterministic MVP сохраняет head/tail, а diagnostics/build/test regions
с error/fail/panic выбирают ordinary bounded path; при отсутствии benefit
возвращается `NoBenefit`/fallback. SQLite schema v98 хранит только compact
metadata. Renderer показывает projection-only diagnostic и не выполняет
compression/recovery.

## Unified Context Namespace v1 (план 131, реализован 2026-09-09)

`evohime_core::context_namespace` — Core-owned metadata-only discovery layer
поверх существующих владельцев Memory, Knowledge, Skills, Guidance, Artifacts
и repository projections. В namespace сохраняются bounded node descriptors,
topology, source revision/hash, projection metadata и typed retrieval traces;
authoritative content, raw prompts, secrets, hidden reasoning и второй ACL не
создаются. `stable_ref` и logical path — только навигация, каждое чтение
проверяет immutable `ContextViewSnapshot`, allowed kinds, roots, exclusions и
sensitivity ceiling.

Контракт v1 использует `NodeKind`, `ProjectionLevel` (`Abstract`, `Overview`,
`Detail`), freshness/trust/index health, provenance class и canonical bounded
SHA-256 metadata. Projection принимается только при точном совпадении source,
parser/profile/policy revisions; stale projection остаётся typed failure.
Retrieval ограничен depth, visited nodes, per-level quotas и token budget,
сохраняет explicit-ref priority и стабильный `node_id` tie-break. Trace хранит
только visits, reason codes, selected/rejected refs, token contributions,
fallback и status. Retrieved Memory/Knowledge маркируется provenance и не
становится новым подтверждённым evidence.

Detail не извлекается из SQLite или renderer. `ContextDetailResolver` — typed
adapter boundary; пока versioned adapter не зарегистрирован, explicit detail
возвращает `projection_generation_failed` с
`detail_resolver_unavailable`, а не успешный пустой результат. Это намеренное
fail-closed состояние; direct source locator остаётся bounded reference для
будущего authoritative opener.

SQLite schema v102 аддитивно устанавливает nodes, projections, views, traces и
idempotency metadata с monotonic revision fences, bounded JSON и migration
backup/transaction semantics. Authenticated additive IPC использует command
261/event 106; replay/resync и correlation остаются общим IPC transport
контрактом. Electron получает только redacted projection для внутренних
Core/agent/model сценариев; отдельной вкладки `Context Namespace` нет.

## Agent Context Loadouts v1 (план 137)

`context_loadouts` — Core-owned versioned assignment layer поверх Memory Views,
Knowledge, Skills, Project Guidance, Repository Map, Artifacts и Experience
owners. Profile entries фиксируют source kind/ref, revision policy,
requiredness, priority и usage mode; loadout не создаёт знания, ACL или новый
retrieval backend. Migration v106 хранит immutable profile revisions с
idempotency. Resolve сохраняет exact snapshot identity и fail-closed health:
missing required entries блокируют, rejected entries деградируют.
Authenticated IPC command 265/event 110 и Electron дают metadata-only
projection; active run остаётся pinned к snapshot.

## Skill Source & Update Lifecycle v1 (план 138)

`skill_source_lifecycle` хранит provenance источника, installation mode, exact
installed revision, package/content hashes, trust reference, update state и
divergence. Skill Registry, trust scan и workspace mutation остаются
владельцами своих authorities. Migration v107 хранит только bounded metadata;
`check_update` запрещает silent overwrite локально изменённых/vendored копий.
Managed staging и активация проходят explicit policy boundary, active runs
остаются pinned. IPC 266/111 и Electron дают metadata-only projection.

## Project Knowledge Notebook v1 (план 143)

`project_knowledge_notebook.rs` хранит bounded metadata-only каталог ссылок
на знания проекта: title, source reference, content hash, tags, scope,
lifecycle и immutable revision. Raw note body не входит в контракт; schema
v112 хранит revisioned metadata, idempotency и durable run pin в транзакционной
migration. Core проверяет размеры, уникальность и canonical SHA-256, а `pin`
закрепляет только active revision.

Authenticated additive IPC command 271/event 116 проецирует status, revision,
entry count, hash prefix и typed errors. Existing knowledge/source owners
остаются владельцами содержания и прав; renderer не получает raw data,
secrets или authority, а invalid/stale/unknown paths fail closed.

## Offline Experience Consolidation Cycle v1 (план 147)

`offline_experience_consolidation.rs` хранит bounded cycle metadata, input
references, output hash и immutable revision. Schema v115 хранит только
metadata/idempotency; `consolidate` является offline metadata-only verdict и
не вызывает external effects, scheduler или второй knowledge owner.

Authenticated additive IPC command 274/event 119 проецирует status, counts,
hash prefix и effect boundary. Raw experience, prompts, transcripts, secrets
и external outputs не проходят через storage или renderer; invalid/unknown
состояния fail closed.
