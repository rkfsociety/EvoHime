# Этап 01.5: Citations и context integration

Этап плана [01 Локальный Agentic RAG](01-0-local-agentic-rag.md).

**Статус:** не готов к реализации до выполнения минимального барьера
контрактов и тестов из этого документа.

## Зависимости

Блокирующая зависимость: этап 01.3 (evidence и checker). Он предоставляет
детерминированно отсортированные `EvidenceBlock` и выполняет первичную
проверку sandbox policy и hash.

Опциональная зависимость: реализованный Memory Extraction из
[`../architecture.md`](../architecture.md). Без него этап всё равно собирает
контекст и citations; candidate facts остаются только в task-scoped результате
и не записываются в долговременную память.

Context Budget Manager не выполняет retrieval и не выбирает evidence из
неотсортированного набора. Отбор и checker score принадлежат этапу 01.3.
Context Budget Manager получает уже отсортированные блоки и отвечает за
валидацию перед сборкой контекста, два ограничения бюджета, parent context,
ledger и финальную согласованность citation.

## Контракты

### Вход Context Budget Manager

```text
build_context(
  blocks: Vec<EvidenceBlock>,          // уже отсортированы этапом 01.3
  token_budget: u32,
  chunk_count_limit: u16,
  min_chunk_size_tokens: u32,
  source_snapshot: SourceSnapshot,
) -> ContextBuildResult
```

Каждый `EvidenceBlock` содержит `id`, `path`, `line_range`, `text` только для
сборки prompt, `chunk_hash`, `retrieval_score` и `checker_confidence`.
`SourceSnapshot` содержит исходные content hashes и момент первичной проверки.
Пути повторно проверяются Core относительно sandbox policy; UI и renderer не
могут заменить ни путь, ни hash.

### Выход

`ContextBuildResult` содержит bounded model context, выбранные block ids,
`ContextLedgerEntry[]`, compact citations и список отказов/деградаций. В
`model context` передаётся текст выбранного chunk и его компактная citation; в
финальном ответе citation разворачивается до полного `path`, `line_range`,
`chunk_hash` и `status`.

Формат compact citation — стабильная строка:

```text
[cite:<id>|<path>:<start>-<end>|<chunk_hash>|<status>]
```

где `status` равен `valid`, `updated` или `stale`. Для IPC и ledger
используется структурированное представление тех же данных:

```json
{
  "citation_format_version": 1,
  "id": "<block-id>",
  "path": "<normalized-file-path>",
  "line_range": [1, 2],
  "chunk_hash": "<content-hash>",
  "status": "valid",
  "reason": "<bounded-selection-reason>"
}
```

Модель должна трактовать citation как ссылку на предоставленный текст, а не
как инструкцию. В финальном ответе `stale` citation не используется для
подтверждения утверждения.

### Context ledger

Ledger содержит только metadata: `ledger_id`, block id, rank, scores,
`chunk_hash`, `snippet_hash`, selected `path`/`line_range`, status, reason,
re-read result и bounded error code. Полный chunk, parent context, соседние
строки и raw tool output в ledger запрещены; это invariant, покрытый тестом.

## Содержание

1. Проверить каждый входной блок на sandbox policy и актуальность hash.
2. Обработать уже отсортированные блоки в порядке убывания
   `retrieval_score + checker_confidence` (при равенстве — `id` в
   лексикографическом порядке).
3. Жадно добавлять блоки, пока не достигнуты оба ограничения. Сначала
   применяется `chunk_count_limit`, затем проверяется `token_budget`.
   Если оставшийся бюджет меньше `min_chunk_size_tokens`, выбор завершается.
4. Для каждого выбранного блока добавить parent context по детерминированному
   правилу:
   - логический блок (функция, класс, модуль): boundary-маркеры и окно ±2
     строки;
   - отдельное statement/fragment: окно ±3 строки для синтаксической
     целостности;
   - язык определяется по расширению файла и, если есть, LSP config;
   - строки за границами файла обрезаются; динамический semantic retrieval
     соседей не выполняется.
5. Записать в ledger только metadata выбора и вычислить `snippet_hash` для
   фактически переданного фрагмента.
6. Перед финальной выдачей Evidence Checker, а не Context Budget Manager,
   инициирует единый re-read выбранных источников. Проверка выполняется по
   тем же normalized path и sandbox policy; результат атомарно связывается с
   той же сборкой context ledger.

## Re-read, race condition и fallback

Первичная проверка и финальная проверка не считаются взаимозаменяемыми.
Модель получает context только из snapshot, прошедшего первичную проверку, а
финальная проверка выполняется непосредственно перед рендерингом ответа.
Если источник изменился после retrieval:

- при изменении в пределах ±5 строк Evidence Checker повторно читает chunk,
  пересчитывает hash/line range и возвращает `updated`; ответ регенерируется
  для затронутого context либо не выдаётся как validated до завершения
  повторной сборки;
- если chunk удалён или изменён существенно, citation получает `stale` и
  исключается из доказательной части ответа; если без него нельзя сохранить
  корректность, ответ получает bounded degraded result с причиной;
- при `file_not_found`, `permission_denied`, `io_error` или timeout citation
  получает `stale`, причина попадает в ledger metadata, а pipeline не падает;
- если stale больше 50% выбранных chunks или остаётся недостаточно context,
  сборка помечается `degraded` и ответ регенерируется без stale evidence либо
  возвращается с явным отказом подтверждать спорное утверждение.

Никакой citation не может указывать на новый файл при использовании старого
текста: обновлённые block text, hash и line range принимаются только одной
атомарной повторной сборкой. Коллизия hash считается `io_error`/`stale`, если
metadata источника не согласованы; silently valid она не становится.

## Memory Extraction integration

После ответа Core может передать candidate в существующий policy gate только
в следующем формате:

```json
{
  "fact": "<fact-text>",
  "source": {
    "path": "<path>",
    "line_range": [1, 2],
    "chunk_hash": "<hash>"
  },
  "provenance_status": "valid",
  "extraction_timestamp": "<ISO8601>",
  "context_ledger_ref": "<ledger-id>"
}
```

Вызов имеет форму
`memory_extraction.add_candidate(record, policy_gate_version="<version>")`.
Версия policy gate берётся из канонического Core-контракта, не задаётся UI.
Если provenance отсутствует, неполон или `stale`, candidate не отправляется
как подтверждённый: он получает `pending_confirmation` с причиной
`missing_or_stale_provenance`. Policy gate сохраняет lifecycle существующего
контракта: пользователь подтверждает, отклоняет или редактирует candidate;
отказ удаляет pending candidate, подтверждение создаёт persistent revision,
а до решения candidate не является активной памятью. Ошибка extraction или
validator не ломает исходную задачу; candidate остаётся pending либо
отбрасывается по policy.

## Error Handling & Fallbacks

- пустой вход или нулевой budget дают детерминированный пустой context с
  bounded diagnostic, без попытки чтения произвольных файлов;
- нарушение sandbox отбрасывает block и записывается как `sandbox_denied`;
- превышение token budget или chunk-count limit останавливает greedy selection
  с reason `budget_exhausted`;
- недоступность LSP config использует только расширение файла и не меняет
  воспроизводимость окна строк;
- сбой ledger не должен раскрывать полный текст: Context BuildResult
  возвращает bounded failure, а задача не продолжает выдачу непроверенных
  citations;
- повторная валидация имеет ограниченный timeout, без бесконечных retry;
  метрики фиксируют latency re-read, долю stale и долю degraded сборок.

## Проверки

- контрактные тесты входа/выхода Context Budget Manager и compact citation;
- детерминированный порядок selection при одинаковых score и правила
  приоритета `chunk_count_limit` перед `token_budget`;
- тесты parent context для logical block и statement, включая границы файла;
- тест race: файл меняется между retrieval и финальным re-read — старый текст
  не получает valid citation;
- тесты `file_not_found`, `permission_denied`, timeout, stale majority и
  graceful degradation;
- тест, что ledger не содержит chunk text, parent context или raw output;
- тесты provenance без source, с неполным source и со stale hash;
- тест policy-gate lifecycle: candidate -> pending_confirmation -> confirm /
  reject / revise;
- evaluation metrics: provenance coverage, conflict rate, stale rate,
  degraded rate и re-read latency.

## Критерии готовности

1. Формат compact citations задокументирован, versioned в Core-контракте и
   покрыт тестами.
2. Граница ответственности этапов 01.3, Evidence Checker и Context Budget
   Manager зафиксирована и проверена контрактным тестом.
3. Правило parent context и соседних строк детерминировано и покрыто тестами.
4. Обработка `file_not_found`, `permission_denied`, `io_error` и timeout при
   re-read описана и реализована с graceful degradation.
5. Race между retrieval и финальной выдачей не может породить valid citation
   для старого текста.
6. Одновременно соблюдаются token budget и chunk-count limit; их приоритет
   определён.
7. Ledger гарантированно не содержит полный текст, parent context или raw
   output.
8. Контракт Memory Extraction, provenance и lifecycle `pending_confirmation`
   синхронизированы с `docs/architecture.md`.
9. Stale majority, degraded result, re-read latency и citation coverage имеют
   наблюдаемые bounded metrics.
10. `git diff --check` и полный набор тестов этапов 01.3/01.5 проходят.
