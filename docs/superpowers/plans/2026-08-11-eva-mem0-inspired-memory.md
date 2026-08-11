# План памяти Евы по мотивам Mem0

## Цель

Добавить Еве долговременную, проверяемую и управляемую память: извлекать из диалогов и завершённых действий только полезные факты, находить их по смыслу и ключевым словам, учитывать область действия и время, а также позволять пользователю просматривать, исправлять и удалять записи.

Это план адаптации идей Mem0, а не предложение встраивать Mem0 целиком. EvoHime остаётся локальным native Windows-приложением: WinUI 3 отображает состояние, Rust Core владеет моделью, инструментами, разрешениями, памятью и SQLite, а versioned named-pipe IPC остаётся единственной границей UI/Core.

## Что изучено в Mem0

По репозиторию и официальной документации Mem0 выделены следующие полезные идеи:

1. Единый memory API для добавления, поиска, получения списка, обновления и удаления записей.
2. Извлечение фактов из сообщений через LLM вместо сохранения всего диалога как необработанного текста.
3. Разделение памяти по сущностям и контексту: user, agent и run/session.
4. Поиск с фильтрами, top-k, threshold и оценкой релевантности.
5. Гибридный retrieval: semantic search, BM25/keyword search и entity matching с объединением сигналов.
6. Временная релевантность: различать текущую настройку, прошлое событие и будущий план.
7. Контур исправления и удаления памяти, включая защиту от опасного удаления без явной области.
8. Асинхронные операции и наблюдаемость вокруг memory pipeline.

Главная оговорка: README Mem0 отдельно указывает, что лучшие показатели нового алгоритма относятся к managed Platform и включают proprietary optimizations. Поэтому эти цифры нельзя обещать для локальной реализации Евы; их можно использовать только как направление для собственных benchmark-тестов.

Источники:

- [Mem0 repository](https://github.com/mem0ai/mem0) — состав SDK, CLI, server и общая модель использования.
- [Add Memory](https://docs.mem0.ai/core-concepts/memory-operations/add) — извлечение фактов, metadata и scope идентификаторы.
- [Search Memory](https://docs.mem0.ai/core-concepts/memory-operations/search) — фильтры, top-k, threshold и retrieval.
- [Update Memory](https://docs.mem0.ai/core-concepts/memory-operations/update) — исправление содержания и metadata.
- [Delete Memory](https://docs.mem0.ai/core-concepts/memory-operations/delete) — безопасное точечное и scoped-удаление.
- [Migration to the new memory algorithm](https://docs.mem0.ai/platform/features/graph-memory) — ADD-only extraction, hybrid search, entity linking и temporal reasoning.
- [Mem0 REST API](https://docs.mem0.ai/open-source/features/rest-api) — набор CRUD/search операций, который можно использовать как ориентир для Core API, но не как обязательный HTTP-слой.

## Текущее состояние EvoHime

- `crates/tool-runtime/src/tools/memory.rs` уже содержит контракт `memory.search`, parser входа и формат результата, но само выполнение делегировано несуществующему memory backend.
- `crates/evohime-local-storage` сейчас хранит только event journal v1; миграции транзакционные и создают `.db.bak` перед изменением схемы.
- `crates/evohime-core` уже журналирует task events, выполняет tool loop и имеет reflection/experience направление.
- В `crates/permissions` уже предусмотрено разрешение `MemorySearch`.
- IPC поддерживает versioned envelope, sequence replay и bounded frame size, но отдельных memory-команд и событий ещё нет.

Следствие: основу следует реализовать как отдельный memory domain в Rust Core и local-storage, не превращая event journal в таблицу текущих фактов и не добавляя бизнес-логику в WinUI.

## Архитектурные решения

### 1. Локальный native backend

Не добавлять обязательный Python runtime, Mem0 server, Docker, PostgreSQL, Redis или облачный API. Первая реализация живёт в Rust crates и SQLite в `%LOCALAPPDATA%\\EvoHime`.

Embedding и rerank должны быть интерфейсами model gateway. Сначала нужен рабочий lexical fallback через SQLite FTS5, затем подключаемый embedding provider. Без embedding provider Ева должна оставаться полезной и детерминированной.

### 2. Append-only provenance и derived current view

Не затирать старый факт без следа. Каждая запись получает `memory_id`, source event/task, timestamps, scope, confidence и status. Исправление создаёт новую версию или явно помечает старую как superseded; удаление создаёт audit event и удаляет запись из активного retrieval.

Это сохраняет объяснимость, replay и возможность восстановления, но не позволяет старым противоречивым фактам конкурировать с текущим представлением.

### 3. Области и границы памяти

Минимальные scopes:

- `user` — устойчивые предпочтения и правила владельца;
- `agent` — факты о поведении и конфигурации Евы;
- `workspace` — соглашения конкретного проекта и его технический контекст;
- `session`/`run` — временный контекст одной задачи;
- `experience` — обобщённые patterns из успешных и неуспешных tool-run.

По умолчанию поиск ограничивается текущим user + workspace + session и только явно разрешённые более широкие scopes. Нельзя смешивать память разных пользователей или workspace.

### 4. Не копировать graph database

Полная graph memory с Neo4j для локального Windows-продукта не нужна на первом этапе. Entity linking можно реализовать в SQLite таблицами сущностей и связей между memory records. Отдельный graph backend рассматривать только после измерения реальной пользы для рабочих задач Евы.

## Предлагаемая модель данных

В `evohime-local-storage` добавить транзакционной миграцией:

- `memories`: id, scope, user_id, agent_id, workspace_id, session_id, kind, content, normalized_content, status, confidence, importance, source_task_id, source_event_sequence, observed_at, valid_from, valid_until, created_at, updated_at;
- `memory_metadata`: memory_id, key, value с ограниченной и валидируемой схемой либо JSON metadata с безопасным размером;
- `memory_versions`: прежнее содержание, причина изменения, parent memory id и event sequence;
- `memory_entities`: entity id, canonical name, type, normalized name;
- `memory_entity_links`: memory id, entity id, confidence;
- `memory_fts`: FTS5-индекс content, normalized_content и полезных metadata;
- `memory_embeddings`: memory id, provider/model, vector payload, dimension и checksum — только когда выбран embedding backend.

Индексы должны покрывать scope, status, workspace, session, `updated_at` и source task. Векторный формат не должен привязать схему к одному поставщику; на машинах без векторного backend работает FTS5 + entity matching.

## Приоритеты

| Приоритет | Возможность | Польза для Евы | Зависимости |
| --- | --- | --- | --- |
| P0 | Memory domain и SQLite schema | Реальное durable-хранилище вместо заглушки | storage migration |
| P0 | Scoped add/search/get/delete | Контролируемая персонализация и privacy | Core + IPC |
| P0 | FTS5 retrieval с filters/top-k/threshold | Работа без внешней vector DB и embedding | SQLite |
| P0 | Интеграция с task/context loop | Память автоматически помогает следующей задаче | Core, redaction |
| P1 | LLM fact extraction и deduplication | Сохраняются факты, а не весь шум диалога | model gateway |
| P1 | Hybrid retrieval и entity linking | Лучше находятся связанные факты | FTS5, optional embeddings |
| P1 | Native Memory inspector | Пользователь видит и контролирует, что помнит Ева | IPC + WinUI |
| P1 | Retention, export, import и erasure | Безопасное обслуживание и переносимость | storage, approvals |
| P2 | Feedback/self-healing | Thumb up/down корректирует confidence и retrieval | UI + events |
| P2 | Background consolidation | Сводка повторяющихся experience records | scheduler, budget |
| P2 | Optional remote Mem0 adapter | Экспериментальное сравнение с внешним backend | explicit opt-in, secrets |

## Этап 1. Memory domain и storage foundation (P0)

1. Создать отдельный Rust-модуль или crate `evohime-memory` с типами `MemoryRecord`, `MemoryScope`, `MemoryKind`, `MemoryStatus`, `MemoryFilter`, `MemorySearchResult` и `MemoryProvenance`.
2. Расширить `LocalDatabase` транзакционной миграцией; перед миграцией сохранять backup по существующему правилу.
3. Реализовать create/get/list/delete и versioned supersede. Для `delete_all` требовать хотя бы один scope/filter; wildcard сделать отдельным явно опасным действием с approval.
4. Ввести нормализацию, лимиты длины, redaction секретов и запрет записи API keys, credentials, полных tool payloads и приватных заголовков.
5. Покрыть storage тестами на миграцию, backup, scope isolation, duplicate handling, version history и delete safety.

Критерий: память переживает перезапуск Core, не смешивает workspace и восстанавливается через существующие backup/replay механизмы.

## Этап 2. Поиск и рабочий Core API (P0)

1. Реализовать `memory.search` вместо текущей заглушки через Core-owned backend.
2. Сначала использовать SQLite FTS5/BM25, exact/entity matches, scope filters, `top_k`, minimum score и temporal decay.
3. Объединять сигналы в объяснимый score: lexical relevance, entity match, recency/validity, importance и confidence. Возвращать breakdown, source task и timestamps, но не секретный исходный контекст.
4. Добавить `memory.add`, `memory.get`, `memory.update`/supersede и `memory.delete` с permission matrix. Search остаётся отдельным permission, write/delete требуют более строгой политики.
5. Добавить в IPC команды и события: `MemorySearch`, `MemoryList`, `MemoryAdd`, `MemoryUpdate`, `MemoryDelete`, `memory.changed`, `memory.search.completed`, `memory.search.failed`.
6. Обновить protobuf, Rust transport, C# envelope и compatibility tests; сохранить major compatibility и использовать minor extension для обратно совместимых добавлений.

Критерий: одинаковые данные и запрос дают стабильный результат на mock backend; UI reconnect/replay не создаёт повторных записей.

## Этап 3. Интеграция в agent loop и extraction (P1)

1. Перед вызовом модели выполнять один bounded memory search по пользовательскому prompt и текущему workspace.
2. Вставлять найденные записи в отдельный явно помеченный memory context с лимитом токенов; пустой и низкоуверенный результат не должен искусственно заполнять prompt.
3. После завершения задачи асинхронно запускать extraction только для user message, подтверждённых assistant decisions, tool outcomes и reflection events.
4. Начать с эвристического extraction для явных предпочтений/ограничений и mock extractor в тестах; LLM extraction подключить через model gateway отдельным budgeted call.
5. Принять ADD-only как безопасный ingestion path: конфликт не уничтожает историю, а создаёт новую запись и supersedes устаревшее представление после проверки.
6. Дебаунсить повторные записи по normalized content, scope и source; не сохранять каждый streaming delta.
7. При timeout, cancellation, approval denial или failed task сохранять только безопасный диагностический/experience факт с корректным status, а не весь tool output.

Критерий: память не увеличивает основной latency без budget, не сохраняет секреты и не может сама обойти permission/approval gate.

## Этап 4. Hybrid retrieval и temporal/entity signals (P1)

1. Добавить trait для embeddings: provider, model, dimension, timeout, cancellation и redaction-safe tracing.
2. Сделать embedding index опциональным; при недоступности модели автоматически использовать FTS5, но показывать фактический режим retrieval в trace.
3. Реализовать entity extraction с ограниченным типизированным набором: person, project, file, tool, provider, device, concept.
4. Связывать memory records с entities и использовать entity match как отдельный сигнал, не разрешая без проверки непредсказуемые графовые выводы.
5. Добавить temporal fields и запросы «сейчас», «раньше», «планируется», чтобы новая preference не конкурировала с устаревшей.
6. Провести локальный benchmark на synthetic fixtures: recall@k, precision@k, latency p50/p95, prompt token savings, duplicate rate и false-memory rate.

Критерий: hybrid режим улучшает recall/precision относительно FTS5 baseline на данных Евы, а при сбое embedding provider остаётся рабочий fallback.

## Этап 5. Native Memory inspector, privacy и lifecycle (P1)

1. Добавить в WinUI раздел «Память» в approved shell: поиск, фильтры scope/kind/workspace, confidence и временной диапазон.
2. Показывать содержание, источник, дату наблюдения, причину появления и статус superseded; отдельно обозначать машинный inference.
3. Добавить actions: исправить, забыть одну запись, забыть scope/workspace и экспортировать выбранные записи. Опасные массовые операции требуют approval/confirmation.
4. Реализовать retention policy для session/run и отдельную настройку для user/workspace/experience memory.
5. Сделать JSONL export/import с version, checksum и redaction; import сначала загружает preview/dry-run.
6. Поддержать полный user/workspace erasure и тестировать, что FTS5, embeddings, entities и versions не оставляют активных ссылок.
7. В trace показывать `memory_ids` и агрегированный retrieval decision, но не незамаскированный private content.

Критерий: пользователь может понять, почему Ева что-то помнит, изменить это и убедиться, что удалённая запись больше не участвует в поиске.

## Этап 6. Feedback, consolidation и экспериментальные adapters (P2)

1. Добавить feedback event: useful, irrelevant, wrong, outdated. Feedback меняет confidence/importance только через Core policy.
2. Реализовать bounded background consolidation: объединять дубли в новую версию, сохранять provenance и не менять память во время активного approval-sensitive task без явного разрешения.
3. Добавить experience reports по повторяющимся tool failures/successes, связав их с существующим reflection loop.
4. После локального benchmark можно сделать optional adapter к Mem0 REST/Platform для сравнительных экспериментов. Он не должен быть default backend, а credential хранится через Credential Manager/DPAPI.

## IPC и UI-контракт

UI получает только DTO и события Core. В protobuf нужно передавать стабильные id, scope, score, timestamps, status и safe metadata; не передавать embedding vectors и полный сырой transcript без необходимости.

Для длинных списков использовать pagination/cursor, bounded page size и sequence replay. Все write/delete команды должны иметь request id, idempotency key и audit event, чтобы reconnect не дублировал операции.

## Безопасность и отказоустойчивость

- Memory retrieval не должен автоматически расширять workspace, shell, network или Git permissions.
- Сохранять только минимальный фрагмент, необходимый для будущей пользы; tool output по умолчанию хранить как summary/reference, а не как полный payload.
- Секреты, токены, cookies, private keys, credentials и чувствительные заголовки редактировать до extraction и до trace.
- Запросы к внешней LLM/embedding-модели считать внешней передачей данных и пропускать через provider policy/approval.
- Ограничить размер записи, число результатов, token budget, queue depth, embedding batch и retries.
- При повреждении индекса можно пересобрать FTS/embeddings из canonical records; database migration и backup должны оставаться атомарными.
- Сбой extraction не должен ломать основной task; память — best-effort side pipeline с наблюдаемым результатом.

## Что сознательно не переносить

- Не добавлять отдельный web UI, REST server или cloud account в базовый runtime.
- Не копировать Python SDK и не делать Mem0 обязательной runtime-зависимостью.
- Не включать graph database до доказанной пользы и benchmark.
- Не сохранять весь чат автоматически и не подмешивать найденные факты без указания их происхождения.
- Не разрешать модели самостоятельно делать массовые delete/reset или менять scope без approval.
- Не обещать опубликованные Mem0 benchmark-цифры для Евы без собственного воспроизводимого набора данных.

## Порядок реализации

1. Memory domain types, SQLite migration и storage tests.
2. Core-owned FTS5 search, filters, safe CRUD и `memory.search`.
3. IPC commands/events и C# compatibility tests.
4. Retrieval injection и bounded extraction после task/reflection.
5. Embedding interface, hybrid scoring, entities и temporal ranking.
6. Native Memory inspector, privacy lifecycle, export/import и erasure.
7. Feedback, consolidation, benchmark и только затем optional remote adapter.

Каждый этап должен быть отдельным task-only коммитом. Перед готовностью каждого этапа обязательны `cargo fmt --all -- --check`, релевантные Rust/WinUI/IPC тесты, `git diff --check`, native package smoke по затронутой области и очистка ненужных build artifacts.

## Критерии успеха

- Ева помнит полезные факты между сессиями, но не смешивает пользователей, workspace и run.
- Поиск работает офлайн на SQLite FTS5 и улучшается при подключении embedding backend.
- Каждая запись имеет источник, время, confidence и понятный lifecycle.
- Пользователь может увидеть, исправить, экспортировать или удалить память без обхода Core permissions.
- Память не ломает task loop при timeout/provider failure и не раскрывает секреты в prompt, trace, SQLite или IPC.
- Результаты retrieval и extraction измеряются собственными fixture/benchmark, а не принимаются на веру по цифрам внешнего сервиса.
