# Этап 01.4: Embeddings как опциональный слой

Этап плана [01 Локальный Agentic RAG](01-0-local-agentic-rag.md).

Этап полностью опционален: без него retrieval работает на SQLite FTS5, а
остальные этапы не должны зависеть от наличия embeddings. Embeddings не
изменяют границы безопасности, workspace-фильтры, валидацию источников или
решение о допустимости действия.

## Статус и точка входа

Статус: спецификация готова к реализации после выполнения входного gate.

Блокирующие зависимости:

1. этап 01.2 завершён и его acceptance пройден: bounded lexical retrieval,
   deterministic ranking/tie-break, metadata scope, stale/redaction checks,
   score explanation и лимиты результата работают независимо от embeddings;
2. в evaluation catalog добавлены и зафиксированы retrieval fixtures для 01.2:
   версии fixtures, ожидаемые результаты, bounded context, leakage/security
   cases и baseline latency. Каталог находится в `tests/evals/`, его правила
   описаны в [`../evaluations.md`](../evaluations.md), а команда воспроизведения
   должна быть указана в самом fixture;
3. определены baseline-значения FTS5 на том же corpus, hardware profile и
   наборе запросов, на которых будет измеряться hybrid retrieval.

Этап 01.4 никого не разблокирует. До прохождения входного gate реализация не
начинается: иначе невозможно отличить улучшение ranking от регрессии FTS5.

## Что этап отдаёт наружу

Hybrid retrieval поверх контракта 01.2 с:

- локальным или явно разрешённым embedding backend;
- автоматическим fallback на FTS5 при недоступности модели, индекса или
  несовместимых метаданных;
- атомарной публикацией только полностью построенного vector index;
- детерминированным RRF-ranking и bounded объяснением вклада lexical/vector;
- диагностикой режима, ошибок, latency и состояния индекса;
- сохранением offline-режима и всех ограничений workspace/redaction.

## Контракт vector index

Для каждого поколения vector index хранить обязательные поля:

```text
index_id
workspace_id / project_id
embedding_model_id
embedding_model_version
vector_dimension
distance_metric
normalization
chunker_version
source_generation или content_snapshot
build_status
created_at / published_at
```

Индекс разрешено использовать только если совпадают как минимум
`embedding_model_id`, `embedding_model_version`, `vector_dimension`,
`distance_metric`, `normalization`, `chunker_version`, область видимости и
поколение исходных chunks. Проверка выполняется до retrieval; при
несовпадении запрос не получает частичный результат из старого индекса и
переходит на FTS5 с диагностикой `vector_index_incompatible`.

Сырые embedding-векторы не должны попадать в логи, evaluation artifacts,
экспорт событий или UI. Для чувствительных данных политика redaction из 01.2
применяется до построения вектора.

## Построение и атомарная публикация

Сборка нового индекса выполняется как отдельное поколение:

1. создать staging index с уникальным `index_id` и `build_status=building`;
2. зафиксировать snapshot/generation chunks и metadata-контракт модели;
3. построить и проверить все vectors, количество chunks и контрольные суммы;
4. выполнить consistency-check с `document_chunks` и FTS5, включая отсутствие
   deleted/stale/redacted-invalid chunks;
5. записать `build_status=ready` и провести smoke retrieval на eval fixtures;
6. в короткой транзакции заменить единственный published pointer на новый
   `index_id`; читатели либо видят старое опубликованное поколение, либо новое,
   но никогда `building`;
7. старое поколение пометить `deprecated` и удалить только после успешной
   публикации и завершения активных читателей.

При ошибке, отмене, перезапуске или нехватке ресурсов staging index помечается
`failed`/`cancelled` и удаляется безопасно. Published pointer не меняется,
старый рабочий индекс сохраняется. Повторная сборка должна быть идемпотентной
для одного snapshot и не оставлять артефакты после отмены.

## Триггеры пересборки и синхронизация

Пересборка или дозаполнение запускается по трём причинам:

- изменение конфигурации модели или любого обязательного metadata-поля;
- изменение опубликованного поколения chunks: добавление, изменение,
  удаление, redaction или смена chunker version;
- явная ручная команда пользователя/оператора.

Плановая пересборка допускается как отдельная настройка, но не является
обязательной частью первой реализации. Новые chunks во время фоновой сборки
не смешиваются молча с частично готовым vector index: они остаются доступными
через FTS5, помечаются как pending для следующего поколения и после публикации
проверяются по generation/hash. FTS5 и vector index используют один
published source generation; рассинхронизация включает fallback и diagnostic.

## Retrieval и ranking

По умолчанию hybrid retrieval сначала применяет те же metadata scope,
redaction и bounded limits, что и 01.2, затем параллельно получает lexical и
vector candidates. Ранжирование выполняется Reciprocal Rank Fusion:

```text
rrf_score = 1 / (60 + lexical_rank) + 1 / (60 + vector_rank)
```

`k=60` фиксирован для первой версии и не настраивается пользователем. При
равенстве применяются deterministic tie-break из 01.2: canonical path,
document id, ordinal, byte start. Векторный результат не может расширить
разрешённую область workspace или вернуть chunk, отфильтрованный FTS5.

Каждый результат получает bounded `ranking_explanation` без сырых score:

```json
{
  "algorithm": "rrf",
  "lexical_rank": 3,
  "vector_rank": 1,
  "rrf_rank": 1,
  "sources": ["lexical", "vector"]
}
```

В explanation допускаются максимум два источника и фиксированные числовые
поля; размер и количество объяснений ограничены теми же bounded limits, что и
контекст. При fallback используется `algorithm=fts5` и объяснение 01.2.

Selective enablement задаётся конфигурацией Core с allow-list языков и
канонических path-prefixes. Если правило не совпало, используется FTS5.
Renderer не выбирает backend и не может обойти это ограничение через запрос.

## Жизненный цикл модели и fallback

Модель проверяется при инициализации retrieval-модуля и перед первым запросом
после смены конфигурации. На каждый запрос повторно проверяется только
доступность опубликованного индекса и совместимость metadata; тяжёлая загрузка
модели не должна происходить заново на каждом запросе.

Любая ошибка загрузки, таймаут, OOM, несовместимый backend или повреждённый
index переводит запрос в FTS5:

- результат возвращается без ожидания повторной загрузки модели;
- записывается WARNING с кодом, `index_id`/model id без секретов;
- выставляются `retrieval.mode=fts5` и причина fallback;
- Core не скрывает деградацию от UI и диагностических журналов.

Ограничения RAM/VRAM, размер batch, timeout и cancellation должны быть
явными. При превышении лимита загрузка/сборка отменяется, а не продолжается
до OOM процесса.

## Offline, безопасность и хранение

Offline-режим обязан работать с локальной моделью без внешнего сервиса. Если
выбран удалённый backend или он недоступен, retrieval деградирует в FTS5 и не
повторяет сетевые попытки бесконечно. Сетевой вызов embedding backend требует
отдельного явного разрешения и не является скрытым поведением поиска.

Хранение vector index использует тот же защищённый data directory и backup /
retention policy, что и SQLite. В план реализации входят:

- контроль доступа к файлам индекса;
- исключение ключей и содержимого vectors из логов;
- retention: не более одного published и ограниченного числа deprecated
  поколений, с безопасной очисткой после readers drain;
- тест на отсутствие secret/PII markers в диагностических артефактах.

## Наблюдаемость

Для каждого retrieval запроса и build operation экспортировать bounded
диагностические поля в существующий Core JSONL log/event journal:

- `retrieval.mode`: `fts5`, `hybrid` или `fallback_fts5`;
- `fallback_reason`, если применён fallback;
- `index_id`, model id/version и source generation;
- candidate counts, cache hit/miss, build status;
- latency для embedding, lexical, fusion и общего запроса;
- cancellation, timeout, OOM и compatibility errors.

Содержимое запросов, chunks и vectors в этих полях не логируется. Метрики
должны позволять сравнить hybrid с FTS5 на одном eval run и определить, был ли
fallback вызван моделью, индексом или лимитом ресурсов.

## Проверки и acceptance criteria

В `tests/evals/` добавить versioned synthetic fixtures для happy-path и
failure-path. Минимальный набор проверок:

- [ ] входной gate 01.2 подтверждён ссылкой на конкретные fixtures и baseline;
- [ ] ошибка загрузки/timeout/OOM модели автоматически возвращает FTS5,
      пишет WARNING и фиксирует `retrieval.mode=fts5`;
- [ ] незавершённый staging index невидим для retrieval;
- [ ] atomic switch публикует только `ready` index и сохраняет старый при
      ошибке/отмене;
- [ ] отменённая сборка не оставляет staging-артефактов;
- [ ] несовместимые model/version/dimension/metric/normalization/chunker или
      source generation блокируют vector retrieval;
- [ ] FTS5 и vector используют одну разрешённую scope/redaction policy;
- [ ] RRF использует `k=60`, deterministic tie-break и bounded explanation;
- [ ] selective enablement для language/path работает, вне allow-list — FTS5;
- [ ] offline retrieval не требует внешней сети и корректно деградирует для
      удалённого backend;
- [ ] latency: `P99(hybrid) <= 2 * P99(FTS5)` на зафиксированном baseline;
- [ ] качество: NDCG@K и precision@K hybrid не ниже FTS5 на retrieval fixtures;
- [ ] память/batch/timeout ограничены, OOM-сценарий не завершает Core;
- [ ] retention удаляет только неиспользуемые deprecated поколения;
- [ ] логи и artifacts не содержат secrets, PII, raw vectors или raw chunks;
- [ ] проходят Rust tests, deterministic eval/security gates, `git diff --check`
      и проверки сборки затронутых компонентов.

Если для конкретного corpus качество или latency не достигают порога, hybrid
не включается автоматически: published режим остаётся FTS5, а причина и
измерения сохраняются в evaluation verdict.
