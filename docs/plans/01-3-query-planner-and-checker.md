# Этап 01.3: Deterministic query planner и bounded agentic loop

Этап плана [01 Локальный Agentic RAG](01-0-local-agentic-rag.md).

## Зависимости

Блокирующие: этап 01.2 — planner и checker работают поверх FTS5 retrieval.

Опциональные: этап 01.4 — semantic strategy не нужна для рабочего lexical
режима. До появления 01.4 последняя rewrite-попытка завершается с
`semantic_unavailable`, без вызова LLM и без изменения scope или security
filters.

Разблокирует: 01.5 (отбор evidence для контекста) и 04.1 (роль `researcher`
возвращает evidence и unknowns).

## Что этап отдаёт наружу

Детерминированный pre-check planner со строгим валидируемым контрактом,
evidence checker с versioned метриками и bounded agentic loop. Реализация не
требует LLM: rewrite в этом этапе — фиксированный набор локальных стратегий.
Любой будущий semantic/LLM rewrite является отдельной явно включаемой
возможностью и не входит в acceptance этого этапа.

## Planner

### JSON Schema

На границе planner применяется strict JSON Schema Draft 2020-12 с
`additionalProperties: false`. Каноническая схема должна храниться рядом с
реализацией и проверяться unit-тестами; ниже зафиксирован её контракт:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "additionalProperties": false,
  "required": ["need_search", "strategy", "query", "filters", "reason", "confidence"],
  "properties": {
    "need_search": {"type": "boolean"},
    "strategy": {"enum": ["exact_symbol", "lexical", "path", "metadata"]},
    "query": {"type": "string", "minLength": 1, "maxLength": 512},
    "filters": {
      "type": "object",
      "additionalProperties": false,
      "required": ["path", "language"],
      "properties": {
        "path": {"type": ["string", "null"], "maxLength": 1024},
        "language": {"type": ["string", "null"], "pattern": "^[A-Za-z0-9._+-]{1,32}$"}
      }
    },
    "reason": {"type": "string", "minLength": 1, "maxLength": 256},
    "confidence": {"type": "number", "minimum": 0.0, "maximum": 1.0}
  }
}
```

`need_search=false` допускается только с пустым retrieval scope, но `query`
остаётся непустым для аудита решения. `filters.path` — относительный путь
внутри выбранного workspace; абсолютные пути, `..`, UNC-пути и значения,
выходящие за workspace, отклоняются. `filters.language` — allow-listed
идентификатор языка.

Валидация выполняется сразу после построения planner output отдельным
validator-слоем до retrieval. При ошибке output не используется: создаётся
детерминированный fallback `strategy=lexical`, `confidence=0.0`,
`reason="validation_failed"`, с безопасными исходными filters. В diagnostic
log сохраняются только код ошибки и имя поля, а не исходное содержимое.
Если fallback также невозможно построить, loop завершается с кодом
`planner_validation_failed` и uncertainty-ответом.

Pre-check выбирает стратегию без LLM:

- очевидный path (`/`, `\\`, расширение файла или path-компоненты с известным
  разделителем) — `path`;
- `?`, вопросительные слова или явная команда поиска — `lexical`;
- ровно один identifier в формате языка — `exact_symbol`;
- иначе — `lexical` с ограниченными terms.

Ограничение lexical query: не более 8 terms, каждый длиной 2–64 символа,
стоп-слова удаляются, исходная строка не попадает в diagnostic log. `metadata`
используется только для явно заданных фильтров и не выбирается как скрытый
fallback.

Planner не меняет workspace root, sandbox или security filters. Фильтры
передаются в retrieval как структурированные поля, повторно валидируются
retrieval-слоем и отражаются в результате проверки.

## Evidence и checker

Каноническая запись evidence:

```json
{
  "source_id": "stable-document-id",
  "chunk_id": "stable-chunk-id",
  "chunk_text": "...",
  "score": 0.0,
  "metadata": {
    "relative_path": "src/file.rs",
    "language": "rust",
    "content_hash": "sha256:...",
    "file_mtime": "RFC3339"
  }
}
```

`chunk_text` используется только внутри bounded context и не попадает в
diagnostic log. Независимыми считаются evidence с разными `source_id` и
разными content hash; два chunk одного документа независимыми не считаются.
Для утверждения с требованием независимого подтверждения нужны минимум два
таких источника.

Checker использует профиль `evidence_metrics/v1.0`. Профиль хранится как
версионированная allow-listed конфигурация с фиксированными defaults; override
может атомарно перечитать только числовые пороги в начале новой loop, но не
меняет профиль посреди loop. В журнале каждой проверки указывается
`metrics_version`.

| Стратегия | Формула | Порог по умолчанию |
| --- | --- | --- |
| `lexical` | `matched_query_terms / max(query_terms, 1)` | `>= 0.80` |
| `exact_symbol` | `symbol_found && in_scope` | `true` |
| `path` | `docs_under_queried_path / max(returned_docs, 1)` | `>= 0.50` |
| `metadata` | `docs_matching_all_filters > 0` | `true` |

Дополнительные метрики профиля: `score >= 0.20`, diversity — не менее двух
разных `source_id` при запросе, требующем независимого evidence, и freshness —
`file_mtime >= requested_freshness` либо совпадение с актуальным content hash.
`sandbox_valid` истинен только при успешной проверке относительного пути,
доступности файла и принадлежности workspace. При отсутствии metadata freshness
или sandbox validity равны `unknown`, а не `true`.

## Retrieval errors и пустой результат

Пустой результат — валидный результат поиска: loop пишет `empty_result`,
strategy и номер итерации, затем запускает следующую уникальную rewrite-попытку,
если лимиты позволяют. Техническая ошибка (`timeout`, недоступный индекс,
ошибка чтения или нарушение sandbox) не маскируется под отсутствие данных:

- transient timeout допускает одну bounded retry той же попытки;
- ошибка индекса завершает loop с `retrieval_error`;
- ошибка sandbox/security завершается с `security_rejected`;
- после исчерпания retry система возвращает uncertainty и сообщает, что
  данные недостаточны.

## Bounded agentic loop

Defaults: `max_iterations=2`, `wall_clock_timeout=30s`, `token_budget` задаётся
до старта loop. Бюджет распределяется заранее: 60% retrieval/context,
20% rewrite/diagnostics, 20% optional model response; неиспользованный бюджет
не переносится между категориями. В 01.3 model budget равен нулю, если LLM
rewrite явно не включён будущим этапом.

История попыток хранится внутри loop по tuple
`(strategy, normalized_query, filters)`. Повтор такой tuple запрещён;
следующая попытка обязана выбрать следующую стратегию из порядка:
`exact_symbol`, `lexical`, `path/metadata`, затем optional `semantic`.

На каждой контрольной точке причина остановки выбирается в фиксированном
порядке: `iteration_limit` → `timeout` → `token_budget`. До начала операции
проверяется deadline, поэтому просроченная операция не запускается. При
одновременном достижении нескольких лимитов в log указывается выбранная
причина и полный набор достигнутых лимитов.

После низкого coverage выполняется максимум одна новая уникальная rewrite-
попытка за итерацию. После лимита loop возвращает «данных недостаточно» с
опциональным запросом расширения.

## Конфликты

Если deterministic policy не разрешила конфликт по актуальности, затем по
явно настроенному path priority, обе записи передаются модели в bounded
формате:

```text
[CONFLICT source_id=<id> confidence=<score>]
Source A: <evidence reference>
[CONFLICT source_id=<id> confidence=<score>]
Source B: <evidence reference>
Instruction: MUST acknowledge the conflict; do not choose silently.
```

Модель обязана либо явно сообщить конфликт, либо ответ считается
`conflict_unacknowledged` и не превращается в утверждение факта. В 01.3
конфликт не разрешается LLM: loop возвращает uncertainty и ссылки на оба
evidence.

## Expansion request

После исчерпания bounded loop разрешение на расширение описывается строгим
объектом без содержимого файлов:

```json
{
  "type": "request_expansion",
  "suggested_scope": {"path": "src", "languages": ["rust"]},
  "reason": "low_coverage",
  "estimated_cost": {"iterations": 1, "tokens": 1200, "seconds": 5}
}
```

`suggested_scope.path` снова проверяется относительно workspace, `languages`
allow-listed, а все cost-поля — неотрицательные bounded integers. Запрос не
исполняется без отдельного разрешения пользователя/политики.

## Диагностика и UI streaming

Diagnostic log — JSONL с retention по умолчанию 30 дней. Допустимые поля:
`loop_id`, `iteration`, `strategy`, `reason_code`, `metrics_version`,
`coverage`, `conflict_flag`, `document_hashes`, `result_count`, `stop_reason`.
Запрещены file contents, API keys, environment values, variable values,
полные query strings и абсолютные пути; document hashes — только стабильные
SHA-256 identifiers.

UI получает только bounded события: `planner.started`, `retrieval.updated`,
`checker.updated`, `rewrite.started`, `loop.stopped`, `expansion.requested`.
`retrieval.updated` агрегируется не чаще одного события за 100 ms и содержит
strategy, result count, coverage и iteration; финальное событие отправляется
всегда.

## Проверки

- strict schema: неизвестное поле, недопустимая strategy, пустой query,
  неверные filters и confidence вне диапазона;
- validation fail → deterministic lexical fallback и безопасный diagnostic
  code;
- query rewrite не меняет workspace, sandbox или security filters;
- уникальность rewrite history и отсутствие повторной попытки;
- checker formulas и thresholds для профиля `v1.0`;
- freshness, diversity, independent evidence и unresolved conflicts;
- пустой retrieval, transient timeout, index error и sandbox rejection;
- приоритет остановки `iteration_limit`, `timeout`, `token_budget`, включая
  одновременное срабатывание;
- нулевой iteration budget, уже истёкший timeout и нулевой token budget;
- bounded UI events и throttling 100 ms;
- schema expansion request и отказ от исполнения без разрешения;
- integration test полного loop с low coverage, rewrite и uncertainty.

## Критерии готовности

- planner имеет каноническую strict JSON Schema, отдельную валидацию и
  предсказуемый fallback/error path;
- checker использует формулы, пороги и `metrics_version=v1.0`, а не скрытую
  оценку LLM;
- evidence имеет определённую структуру, freshness и критерий независимости;
- agentic loop bounded, не повторяет попытки и логирует детерминированную
  причину остановки;
- retrieval errors, empty results, conflicts и expansion requests имеют
  отдельные контракты;
- diagnostic JSONL не содержит секретов и полноразмерных запросов;
- UI streaming status имеет фиксированные события и throttling;
- unit/integration tests покрывают граничные случаи и полный loop;
- агент не утверждает документальный факт без evidence или явно маркирует
  uncertainty.
