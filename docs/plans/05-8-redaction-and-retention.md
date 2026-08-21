# План 05.8 — Удаление и retention

Статус: этап плана [05](05-0-model-request-provenance.md).

Цель этапа — согласовать provenance с уже данными пользователю гарантиями удаления и ограничить рост локального хранилища. Этот же этап снимает hash-only ограничение [05.2](05-2-durable-storage.md) и включает полное хранение model-visible payload.

## Зависимости

### Блокирующие

- [05.2](05-2-durable-storage.md) — `model_requests`, `model_request_sources`, блоки;
- [05.4](05-4-evidence-provenance.md) — ссылки на источники, которые надо тумбстоунить;
- существующие `forget` памяти, удаление ambient-эпизода и `forget_window`;
- существующий retention receipts как образец согласованных сроков.

### Опциональные

- [05.5](05-5-receipt-and-tool-linkage.md) — `model_responses` и
  `tool_intents`. Если 05.5 ещё не реализован, child rows отсутствуют и
  retention обрабатывает request/sources/shadowing; после его появления он
  обязан подключить эти rows к описанной общей транзакции.
- [05.9](05-9-verify-and-export.md) — offline verifier. Без него состояния `redacted` и `retention_pruned` наблюдаемы только внутри Core через typed errors; отличать их от повреждения offline станет возможно вместе с 05.9.

## Снятие hash-only ограничения

05.8 делает аддитивную миграцию `v27 -> v28`. Мигратор не пытается
реконструировать старые строки `model_requests` с
`payload_mode = hash_only`: у них сохраняются исходные `payload_mode`,
`status`, metadata, block hashes и `envelope_hash = NULL`. Backfill
`envelope_blob`, полный payload, `envelope_hash` или request receipt для них
запрещён. Повторный запуск миграции идемпотентен, а fixture проверяет, что
такая строка осталась byte-for-byte неполной и не стала dispatchable.

После этой миграции `FullForDispatch` — единственный режим записи новых
dispatchable requests. `HashOnlyStorage` остаётся только для чтения старых
fixture/migration-строк; checkpoint отвергает новую hash-only запись до
provider dispatch с `REQUEST_PROVENANCE_COMMIT_FAILED`. При чтении
проверяется `payload_mode` до обычной проверки полного payload: старая
hash-only строка возвращает `REQUEST_RETENTION_PRUNED`, а не
`REQUEST_HASH_MISMATCH` и не успешную реконструкцию. Это правило не меняет
её исходный `status`.

Миграция также добавляет Core-owned `provenance_tombstones`:

```text
provenance_tombstones
- tombstone_id PK
- request_id NOT NULL FK model_requests(request_id)
- subject_kind (request_block | response_output | tool_args |
  shadow_original | shadow_block)
- subject_ordinal NULL
- subject_id NULL
- state (redacted | retention_pruned)
- source_disposition (digest_kept | hash_removed)
- marker_version = 1
- created_at
- UNIQUE(request_id, subject_kind, subject_ordinal, subject_id, state)
```

Для `request_block`/`shadow_block` tombstone идентифицирует opaque ref или
bounded subject id, а не physical `content_hash`; physical hash удаляется
вместе с последней допустимой storage-ссылкой. Это не позволяет tombstone
самому стать обходом правила удаления хеша.

Это typed tombstone, а не новая версия envelope. `envelope_blob` и
`envelope_hash` никогда не переписываются: blob остаётся неизменяемым
описателем исходного request, hash — его исходным canonical proof digest.
Реконструктор сначала читает `model_requests.status` и tombstones; для
помеченного subject отсутствие bytes/refs является ожидаемым состоянием, а
не hash mismatch. Неполный или подменённый tombstone, наоборот, считается
повреждением. По правилам 05.1/05.2 blob содержит только opaque block/source
refs, поэтому физические `source_hash` и `content_hash` можно удалить из
repository rows и экспорта, не оставляя их внутри immutable blob. Правило
удаления чувствительного digest применяется к
`model_request_sources.source_hash`, output/tool-args digest и shadow hash;
`envelope_hash` остаётся нерасширяемым proof commitment и не используется для
реконструкции удалённого текста.

## Два режима удаления

Reconstructability не отменяет уже данных пользователю гарантий удаления. В [`../architecture.md`](../architecture.md) зафиксированы два разных режима, и envelope обязан различать их:

- `forget` памяти — logical deletion с tombstone из одних metadata **и digest**: хеш остаётся;
- удаление ambient-эпизода и `forget_window` — metadata-only tombstone **без текста и без хеша**, плюс физическое удаление высказываний, производных memory-кандидатов и `ambient.%`-строк журнала в одной транзакции.

Разница не косметическая. Для ambient хеш не сохраняют намеренно: там же зафиксировано, что короткую фразу перебирают по хешу за секунды, поэтому хеш приравнивается к содержимому.

Envelope с полным system prompt и messages по умолчанию стал бы вторым местом, где стёртый текст лежит вечно. Это запрещено.

## Правила

1. Удаление источника (`forget` памяти, удаление эпизода, `forget_window`)
   сначала строит замкнутый набор затронутых requests по
   `model_request_sources`, projection-to-block refs, reverse
   `model_request_block_refs`, shadow refs, responses и intents, а затем в одной
   `BEGIN IMMEDIATE` транзакции: (a) вставляет typed tombstones, (b) переводит
   каждый затронутый `model_requests.status` в `redacted`, (c) обрабатывает
   sources, block refs, `model_responses`, `tool_intents` и shadow rows,
   (d) уменьшает refcount и удаляет ставшие бесхозными blocks, и только после
   всех invariant checks делает `COMMIT`. Сбой оставляет и источник, и
   request lifecycle в прежнем состоянии. Metadata (`request_id`, `provider`,
   `model`, времена, counters, linkage) сохраняются.
2. `model_request_sources.source_hash` удалённого источника тумбстоунится по тому же правилу, что и сам источник: для ambient-высказывания и `forget_window` хеш удаляется, для `forget` памяти сохраняется digest. Оставлять хеш короткого удалённого текста в provenance запрещено — это восстановимость перебором, ровно та, ради которой ambient-tombstone его не хранит. `envelope_hash` остаётся whole-request proof commitment, но verifier не использует его как digest источника и не выдаёт по нему удалённый текст.
3. Typed tombstone не записывается поверх `envelope_blob` и не меняет
   `envelope_hash`. Для затронутого request удаляются его недействительные
   `model_request_block_refs`; `refcount` уменьшается ровно на число
   удалённых refs, `last_referenced_at` пересчитывается по оставшимся refs, а
   block физически удаляется только при `refcount = 0`. Если block содержит
   удалённый source, в closure включаются все request refs этого block: ни один
   живой request не может сохранить его bytes. Shared block без удалённого
   source остаётся у живых requests; источник нельзя смешивать с таким block
   без явной source-attribution, иначе commit блокируется.
4. `model_responses` и связанные `tool_intents` входят в ту же транзакцию.
   Response с затронутым output получает `status = redacted`, output/его
   content-addressed bytes удаляются, а `output_hash` получает
   `hash_removed` для ambient/`forget_window` и `digest_kept` для `forget`
   памяти. У response без output исходный `failed`/`interrupted` status не
   подменяется. Связанные intents сохраняют `intent_id`, request/response
   linkage и ordinal, получают `state = redacted`, а `tool_args_hash`
   обрабатывается тем же правилом; immutable effect receipts не переписываются.
5. Такой request и затронутые response/intents переходят в состояние
   `redacted` и перестают быть полностью реконструируемыми. Это явное
   наблюдаемое состояние, а не тихая потеря данных: Core verifier обязан
   отличать `redacted` от повреждения и от несовпадения хеша.
6. Canonical hash оригинала и подписанный request/effect receipts остаются.
   Цепочка receipts не переписывается: доказательство того, что request был
   именно такой, сохраняется, восстановимость текста — нет. Это тот же приём,
   что уже применён к `verified_pruned` в receipts.
7. Provenance retention использует продовую константу
   `PROVENANCE_RETENTION_DAYS = 90`, совпадающую с возрастным окном signed
   receipt retention. Request, response output, captured evidence и shadow
   rows старше cutoff переходят в `retention_pruned` в той же транзакции, что
   и `model_requests.status`, их blocks/refs и child lifecycle states. В
   `retention_pruned` остаются metadata,
   неизменяемые `envelope_blob`/`envelope_hash`, разрешённые digest-ы и
   tombstone; bytes model blocks, response output и captured shadow blocks
   не остаются восстановимыми.
8. Для `context_shadowed_originals` и `context_shadow_blocks` из [05.6](05-6-compaction-shadowing.md)
   отдельный refcount не добавляется: retention в транзакции считает живые
   ссылки через `context_shadow_source_refs` и
   `context_shadowed_originals.content_block_hash`. Кандидаты выбираются по
   `created_at < now - PROVENANCE_RETENTION_DAYS`; поля `last_referenced_at` в
   05.6 нет, поэтому несуществующий timestamp не имитируется. Shadow row
   получает `source_state = retention_pruned`, его captured bytes удаляются,
   а `context_shadow_blocks` удаляется только если после этого не осталось
   живой ссылки. Hash shadow/source сохраняется только когда это допускают
   все source refs; ambient/`forget_window` дают `hash_removed`.
9. Остаточное окно encrypted backups после `forget` фиксировано существующей
   продовой константой `FORGET_BACKUP_RETENTION_MS = 7 * DAY_MS` из
   `memory_extraction`; backup rotation не может сохранять снимок с удалённым
   ambient payload дольше 7 суток. Пользовательский ambient retention
   (`1..90` дней, default 7) не продлевает это backup-окно и не меняет
   provenance retention.
10. Удаление блока по `refcount = 0` не считается редактированием envelope:
    block исчезает только после удаления refs, успешного уменьшения refcount
    и проверки, что на него не ссылается ни один живой request.
11. Core запускает фоновый `spawn_model_provenance_retention`: первый bounded
    прогон выполняется при старте, затем worker запускается каждые 6 часов,
    аналогично receipt retention. Worker использует ту же SQLite-транзакцию и
    не работает через renderer или внешний scheduler; повторный прогон
    идемпотентен.

Статусы и коды ошибок берутся из [05.1](05-1-canonical-request-contract.md): `REQUEST_REDACTED`, `REQUEST_RETENTION_PRUNED`.

## Состав retention и порядок verifier

`redacted` означает целевое удаление по пользовательскому source rule;
`retention_pruned` — возрастное сжатие без пользовательского запроса. В обоих
случаях сохраняются request metadata, immutable envelope proof, linkage,
receipt chain и typed tombstones. В `redacted` source/output/tool/shadow hash
удаляется или сохраняется digest по правилу источника; в
`retention_pruned` применяется то же правило к source refs, а bytes всегда
удаляются. `metadata_hash_only` из 05.6 и старый `payload_mode = hash_only`
не переименовываются в эти состояния и классифицируются своими legacy
typed-ответами.

Core verifier проверяет lifecycle до content hash: сначала валидирует enum,
версию и полноту tombstone, затем receipt/linkage и immutable
`envelope_hash`. Для `redacted`/`retention_pruned` он запрещает выдачу
model-visible текста и возвращает typed result `REQUEST_REDACTED` или
`REQUEST_RETENTION_PRUNED`; отсутствие bytes при корректном tombstone не
является `REQUEST_HASH_MISMATCH`. Для `payload_mode = full` без tombstone
отсутствующий block, изменённый blob или несовпадающий hash остаётся
`REQUEST_HASH_MISMATCH`. Эта классификация действует внутри Core до 05.9;
05.9 лишь переносит те же typed результаты в offline bundle.

## Тесты

### Unit

- тумбстоун `source_hash` по правилу источника: ambient — без хеша, память — digest;
- аддитивная миграция v27 -> v28 сохраняет старые hash-only строки без
  backfill, а новые checkpoint commits требуют `payload_mode = full`;
- typed tombstone и переход в `redacted` не меняют `envelope_blob` или
  `envelope_hash`;
- status request, response и intent обновляются в одной транзакции с refs,
  source hash и tombstones;
- shared model block не удаляется, пока живой request всё ещё ссылается на
  него; refcount и `last_referenced_at` уменьшаются атомарно;
- shadow rows и их captured blocks обрабатываются в той же транзакции, что и `model_request_sources`/envelope; временный `metadata_hash_only` из 05.6 не маскируется под `REQUEST_SOURCE_MISSING`;
- retention сжимает request/response/shadow payload до metadata + hash и
  выставляет `retention_pruned`; shadow blocks удаляются только при нулевом
  производном числе живых ссылок;
- verifier возвращает `REQUEST_REDACTED`/`REQUEST_RETENTION_PRUNED` для
  корректного tombstone и `REQUEST_HASH_MISMATCH` только для повреждения.

### Integration

1. **Удаление:** `forget` памяти и удаление ambient-эпизода редактируют затронутые envelope; текст не восстанавливается, linkage и signed receipt сохраняются, verifier сообщает `redacted`.
2. **Ambient-удаление:** после удаления эпизода в provenance не остаётся ни текста высказывания, ни его хеша; `envelope_hash` и receipt сохраняются.
3. **Retention:** envelope старше срока сжат до metadata + hash; цепочка receipts по-прежнему проверяется.
4. **Одна транзакция:** сбой в середине удаления не оставляет источник
   удалённым, status обновлённым частично или refs/refcount в промежуточном
   состоянии.
5. **Legacy migration:** старый hash-only request не реконструируется, не
   получает receipt и не блокирует чтение других requests; новый dispatchable
   request без полного payload не доходит до provider.
6. **Response/tool linkage:** redaction одного source атомарно классифицирует
   response output и связанные intents, сохраняя их bounded linkage и
   immutable receipts.

## Критерии готовности

1. Удалённый пользователем источник не остаётся восстановимым ни в одном committed envelope — ни как текст, ни как хеш там, где хеш приравнивается к содержимому.
2. `redacted` и `retention_pruned` отличимы от повреждения и от hash mismatch.
3. Рост provenance-хранилища ограничен `PROVENANCE_RETENTION_DAYS = 90`,
   согласованным с retention receipts; backup residual window явно равен
   7 суткам.
4. Полное хранение payload включено для всех новых dispatchable записей;
   старые hash-only строки сохраняют статус, не реконструируются и явно
   классифицируются verifier.
5. `model_responses`, `tool_intents`, `context_shadowed_originals` и
   `context_shadow_blocks` обрабатываются по тем же source/redaction и
   retention правилам в одной транзакции с request status и refcount.
6. Фоновый Core worker выполняет retention при старте и далее с bounded
   шестичасовым интервалом.
