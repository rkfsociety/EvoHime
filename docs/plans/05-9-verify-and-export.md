# План 05.9 — Offline verification и export

Статус: этап плана [05](05-0-model-request-provenance.md).

Цель этапа — сделать provenance каждого model request проверяемым без доверия
к renderer, работающему Core, SQLite или текущему workspace. Этап расширяет
существующий `evohime-verify.exe` и receipt export так, чтобы offline verifier
получал замкнутую выборку request со всеми транзитивными зависимостями,
проверял подписи и lineage и явно различал валидные, намеренно неполные и
повреждённые данные.

`MODEL_VISIBLE_MEANS_RECONSTRUCTABLE` остаётся инвариантом для новых
dispatchable requests. `redacted`, `retention_pruned`, legacy `hash_only`,
`metadata_hash_only` и `REQUEST_EVIDENCE_EVICTED` являются наблюдаемыми
результатами удаления/сжатия, а не успешной реконструкцией и не разрешением
на новый dispatch.

## Зависимости

### Блокирующие

- [05.1](05-1-canonical-request-contract.md) — canonical bytes, hashes,
  lineage, размеры и typed errors;
- [05.2](05-2-durable-storage.md) — `model_requests`, blocks, sources,
  `context_ledger` и атомарная выборка;
- [05.4](05-4-evidence-provenance.md) — source refs и immutable evidence;
- [05.5](05-5-receipt-and-tool-linkage.md) — signed request receipts,
  `model_responses`, `tool_intents` и effect linkage;
- [05.6](05-6-compaction-shadowing.md) — shadow originals, shadow blocks и
  транзитивные source refs;
- [05.8](05-8-redaction-and-retention.md) — typed tombstones,
  `redacted`/`retention_pruned` и правила удаления хешей;
- существующие `evohime-verify.exe`, signed receipt chain и v1 key-history /
  checkpoint export.

05.6 и 05.8 здесь обязательны для выпуска: переходный verifier, который
умеет проверять только полный envelope, не закрывает контракт offline export.
Для legacy записей, созданных до 05.6/05.8, verifier возвращает их явное
typed-состояние и не объявляет их полностью реконструируемыми.

## Формат provenance bundle

### Версия и представление

Export создаёт каталог формата `evohime-provenance-export-v1`. Все JSON-файлы
— UTF-8 без BOM, canonical JSON по правилам 05.1, с завершающим LF. Все
`*.jsonl` сортируются по canonical key и имеют одну запись на строку. Captured
payload хранится как отдельный бинарный content-addressed block; JSON содержит
только `content_hash`, `byte_len`, `payload_mode` и относительный `file_path`.
Пути используют `/`, не допускают `..`, абсолютные пути, symlink и duplicate
entries.

Каталог содержит следующие обязательные секции:

```text
manifest.json
bundle.sig
key-history.jsonl
checkpoints.jsonl
receipt_records/records.jsonl
context_ledger/entries.jsonl
request_snapshots/route_policy.jsonl
model_requests/requests.jsonl
model_requests/envelopes/<request_id>.json
model_requests/block_refs.jsonl
model_requests/blocks/<content_hash>.bin
model_responses/responses.jsonl
model_responses/blocks/<content_hash>.bin
tool_intents/intents.jsonl
tool_intents/blocks/<content_hash>.bin
context_evidence/sources.jsonl
context_evidence/blocks/<content_hash>.bin
context_shadowed_originals/records.jsonl
context_shadow_source_refs/refs.jsonl
context_shadow_blocks/blocks/<content_hash>.bin
provenance_tombstones/tombstones.jsonl
```

`receipt_records/records.jsonl` содержит полные signed canonical receipt
records, включая request receipts, effect receipts, approval linkage и
terminal receipts. `key-history.jsonl` и `checkpoints.jsonl` сохраняют
совместимый с существующим receipt export формат. Старый v1 bundle без
provenance-секций продолжает проверяться старым режимом verifier, но не может
быть выдан за `provenance-export-v1`.

`model_requests/requests.jsonl` для каждой записи обязан содержать как минимум:

```text
request_id, logical_request_id, attempt, parent_request_id,
previous_request_hash, request_kind, ledger_id, provider, model,
envelope_version, payload_mode, envelope_hash,
context_projection_hash, route_snapshot_hash, policy_snapshot_hash,
route_policy_hash_shared, status, lifecycle_state, tombstone_ids
```

`model_requests/envelopes/` содержит точные canonical envelope bytes, а не
пересобранный по текущему состоянию JSON. `block_refs.jsonl` и blocks замыкают
system prompt, messages и tool schemas. `context_ledger/entries.jsonl`
содержит canonical ledger rows, необходимые для проверки `ledger_id`,
`context_ledger_hash` и projection. `request_snapshots/route_policy.jsonl`
содержит redacted canonical route/policy snapshots из 05.3, их source id,
payload mode и hashes. Если старый snapshot уже evicted, запись обязана
содержать explicit eviction marker; verifier возвращает
`REQUEST_EVIDENCE_EVICTED`, а не подставляет текущую policy.

`model_responses`, `tool_intents` и их blocks обязательны даже когда выборка
не содержит response или intent: тогда соответствующий JSONL-файл пустой, но
присутствует в manifest. Response record содержит bounded output metadata и
ссылку на output block, если output не был удалён. Intent record содержит
`origin_request_id`, `origin_request_envelope_hash`, `response_id`, ordinal,
`tool_args_hash`, payload mode и block reference при наличии сохранённых
аргументов. Credentials, Authorization headers, DPAPI plaintext и raw provider
secrets никогда не попадают в bundle; credential-bearing block экспортируется
как typed redaction/tombstone.

`context_evidence/sources.jsonl` содержит все direct
`model_request_sources`, их source kind, revision, hash, payload mode и
references на captured blocks. `context_shadowed_originals/records.jsonl`,
`context_shadow_source_refs/refs.jsonl` и `context_shadow_blocks/` содержат
все shadow rows, включая `parent_shadow_id`, summary/prune operation и
транзитивные ссылки. Таким образом экспорт включает:

```text
model_requests
model_responses
tool_intents
context_ledger
request_snapshots
context_evidence
context_shadowed_originals
context_shadow_blocks
context_shadow_source_refs
provenance_tombstones
receipt_records
```

При выборе одного request exporter рекурсивно добавляет его
`parent_request_id`/`previous_request_hash`, все requests, породившие summary
или source evidence, связанные ledger rows, route/policy snapshots, direct
sources, shadow chains, response, intents, action/approval receipts и receipt
chain predecessor до signed checkpoint/root. Нельзя экспортировать только
прямой request и объявлять его closure. До 05.6 для legacy записи в
`context_evidence/` попадают только сохранённые direct refs; verifier сообщает
`REQUEST_EVIDENCE_EVICTED` или `REQUEST_SOURCE_MISSING` по фактическому
состоянию и не сообщает `valid`.

### Manifest

`manifest.json` имеет `bundle_schema_version = 1` и содержит обязательные
поля:

```text
export_id
created_at
bundle_schema_version
schema_versions
selection
request_count
receipt_count
chain_roots
chain_checkpoints
request_states[]
files: { file_path: sha256_hex }
file_sizes: { file_path: byte_count }
bundle_content_sha256
signer: { key_id, algorithm = Ed25519, signature_path = "bundle.sig" }
```

`request_states[]` содержит для каждого request `request_id`, `payload_mode`,
`status`, `verification_state`, `tombstone_ids` и `missing_or_pruned_subjects`.
Допустимые `verification_state`:

```text
valid | redacted | retention_pruned | legacy_hash_only |
metadata_hash_only | evidence_evicted | damaged
```

`files` — явная карта каждого payload-файла bundle к lowercase SHA-256; в неё
входят все файлы секций, `key-history.jsonl` и `checkpoints.jsonl`, но не
`manifest.json` и не detached `bundle.sig`, чтобы избежать циклического
самохеширования. `bundle_content_sha256` вычисляется детерминированно:

```text
SHA-256(
  "evohime-provenance-bundle-v1\0" ||
  for file_path in UTF-8 lexical order:
    file_path || "\0" || files[file_path] || "\n"
)
```

Это checksum всего перечисленного содержимого; verifier сначала проверяет
размеры и SHA-256 каждого файла, затем `bundle_content_sha256`, количество
записей, section set и request state map. Отсутствующий файл из `files` или
файл вне allow-list — `EXPORT_MANIFEST_MISMATCH`.

`bundle.sig` — ровно 64-byte Ed25519 signature над:

```text
SHA-256("evohime-provenance-manifest-v1\0" || canonical_bytes(manifest.json))
```

Подпись создаётся тем же Core-owned receipt key lifecycle, что и signed
receipts. Verifier получает public keys из `key-history.jsonl`, проверяет
переходы и подписи key history, но trust anchor не берёт из bundle: оператор
передаёт его offline через существующий `--trust-key <key_id>` либо pinned
public key option. Неизвестный key, неподтверждённый transition или неверная
подпись дают `EXPORT_SIGNATURE_KEY_UNKNOWN`/
`EXPORT_SIGNATURE_INVALID`; bundle нельзя считать валидным по одному
`bundle_content_sha256`.

## Offline verifier

Verifier работает только с bundle и внешним trust anchor. Он не читает Core,
SQLite, renderer, workspace, memory, сеть или текущие provider settings.
Проверки выполняются в следующем порядке, чтобы намеренно удалённые bytes не
маскировались под повреждение.

1. **Границы bundle.** Проверить canonical manifest, allow-list путей,
   resource limits, `files` map, file hashes, content checksum и Ed25519
   signature manifest.
2. **Key history и receipts.** Проверить внешний trust anchor, все переходы
   key history, Ed25519 signature каждого receipt, canonical receipt hash,
   domain/type/version и receipt chain. `previous_receipt_hash` должен быть
   предыдущим receipt той же общей chain либо совпадать с подписанным
   checkpoint boundary; chain root/checkpoint из manifest должны совпадать с
   фактически экспортированной closure.
3. **Envelope canonical hash.** Разобрать envelope указанной версии, проверить
   duplicate/unknown fields, размер, canonical bytes и `envelope_hash`.
   `payload_mode = full` обязан иметь все blocks, точные `byte_len` и hash.
   Несовпадение canonical hash, blob, block bytes или порядка refs —
   `REQUEST_HASH_MISMATCH`.
4. **Ledger и projection.** Найти `ledger_id`, проверить
   `context_ledger_hash`, вычислить `context_projection_hash` по точной формуле
   05.1 и сравнить его с envelope, request row и signed request receipt.
   `route_snapshot_hash`, `policy_snapshot_hash` и `route_policy_hash_shared`
   проверяются отдельно: при `shared = true` обе ссылки обязаны указывать на
   один canonical snapshot и один source id; при `shared = false` требуются
   два независимых snapshot source id, даже если digest случайно одинаков.
   Невозможный ledger/projection/route linkage — `REQUEST_LEDGER_MISMATCH` или
   `REQUEST_RECEIPT_LINKAGE_MISMATCH`.
5. **Lineage.** Для первой attempt `parent_request_id` и
   `previous_request_hash` отсутствуют. Для каждой последующей attempt
   predecessor должен присутствовать в closure, иметь тот же
   `logical_request_id` и `ledger_id`, предыдущий `attempt` и hash, равный
   `previous_request_hash`. Несовпадение даёт `REQUEST_LINEAGE_MISMATCH`;
   пропуск predecessor — `REQUEST_RECONSTRUCTION_FAILED`.
6. **Source/evidence closure.** Разрешить каждый direct source ref, проверить
   source hash, revision, captured block hash/length и source lineage. Summary
   обязан разрешиться до `context_shadowed_originals` и далее до всех
   transitive originals; prune обязан иметь drop reason. Отсутствующий source
   в новой полной записи — `REQUEST_SOURCE_MISSING`, изменённый source —
   `REQUEST_SOURCE_CHANGED`, hash/bytes mismatch — `REQUEST_HASH_MISMATCH`.
7. **Response/tool closure.** Проверить `model_responses` к конкретному
   request attempt, output hash/block и status. Для каждого intent проверить
   `origin_request_id`, `origin_request_envelope_hash`, response/ordinal,
   args hash, approval exact `call_hash`, pre-action receipt, effect receipt и
   terminal result hash. Разрыв этой связи — `REQUEST_TOOL_LINKAGE_MISMATCH`.
8. **Lifecycle classification.** После immutable checks проверить
   `provenance_tombstones` и только затем классифицировать request как
   `valid`, `redacted`, `retention_pruned`, `legacy_hash_only`,
   `metadata_hash_only`, `evidence_evicted` или `damaged`.

Каждый receipt signature проверяется обязательно; «подпись не нужна, потому
что manifest подписан» недопустимо. Для request receipt дополнительно
сверяются `request_id`, `logical_request_id`, `attempt`, `ledger_id`,
provider/model, `request_envelope_hash`, `context_projection_hash`,
`route_snapshot_hash`, `policy_snapshot_hash` и `previous_receipt_hash` с
request row/envelope.

### Ошибки verifier

05.9 добавляет эти стабильные коды в offline verifier contract; существующие
коды `receipts.*` переиспользуются для общего receipt chain:

| Сбой | Код |
| --- | --- |
| отсутствующая/лишняя секция, path traversal, неверная карта файлов или schema version | `EXPORT_MANIFEST_MISMATCH` |
| превышены лимиты bundle, record, line или graph depth | `EXPORT_BUNDLE_TOO_LARGE` / `REQUEST_PROVENANCE_TOO_LARGE` |
| неверная detached manifest signature | `EXPORT_SIGNATURE_INVALID` |
| public key не найден или key history не подтверждена внешним trust anchor | `EXPORT_SIGNATURE_KEY_UNKNOWN` |
| неверный receipt JSON/canonical hash/signature | `receipts.invalid_json` / `receipts.hash_mismatch` / `receipts.signature_invalid` |
| неверный `previous_receipt_hash`, root или checkpoint boundary | `receipts.previous_mismatch` / `receipts.chain_incomplete` |
| envelope/block/source hash или canonical bytes не сходятся | `REQUEST_HASH_MISMATCH` |
| ledger, projection или route/policy snapshot не связан с request | `REQUEST_LEDGER_MISMATCH` / `REQUEST_RECEIPT_LINKAGE_MISMATCH` |
| predecessor, attempt или `previous_request_hash` не сходится | `REQUEST_LINEAGE_MISMATCH` |
| source/ref/shadow closure не разрешается | `REQUEST_SOURCE_MISSING` / `REQUEST_SOURCE_CHANGED` / `REQUEST_RECONSTRUCTION_FAILED` |
| response, intent, approval или effect receipt не связан с request | `REQUEST_TOOL_LINKAGE_MISMATCH` |
| корректное lifecycle-состояние | `REQUEST_REDACTED` / `REQUEST_RETENTION_PRUNED` / `REQUEST_SHADOW_CONTENT_COMPACTED` / `REQUEST_EVIDENCE_EVICTED` |

Verifier возвращает JSON-результат по каждому request и общий exit status;
ошибка одного request не скрывается успешной проверкой другого. Для
`redacted`, `retention_pruned` и других намеренно неполных состояний общий
bundle integrity может быть `verified`, а request outcome остаётся typed
non-complete.

## Классификация неполных данных

Verifier обязан возвращать машинно-читаемый результат на каждый request, а не
сводить всё к одному `invalid`:

| Состояние | Обязательное доказательство | Результат verifier |
| --- | --- | --- |
| `valid` | все payload и refs полны, hashes/signatures/lineage сходятся | `valid` |
| `redacted` | `status = redacted`, полные typed tombstones и допустимый `source_disposition` | `redacted` / `REQUEST_REDACTED` |
| `retention_pruned` | `status = retention_pruned`, полные retention tombstones и сохранённая receipt proof | `retention_pruned` / `REQUEST_RETENTION_PRUNED` |
| `hash_only` | legacy `payload_mode = hash_only`, `envelope_hash = NULL`, block hashes без bytes | `legacy_hash_only` / `REQUEST_RETENTION_PRUNED` |
| `metadata_hash_only` | shadow `source_state = metadata_hash_only`, hash/metadata row и compaction reason | `metadata_hash_only` / `REQUEST_SHADOW_CONTENT_COMPACTED` |
| `REQUEST_EVIDENCE_EVICTED` | explicit evidence-eviction tombstone, сохранённая linkage и причина eviction | `evidence_evicted` / `REQUEST_EVIDENCE_EVICTED` |
| damaged | bytes отсутствуют без tombstone, tombstone неполный, файл/hash/signature/lineage изменён | `damaged` / соответствующая `REQUEST_*` ошибка |

`redacted` и `retention_pruned` никогда не принимаются только по отсутствию
bytes. Tombstone обязан точно указывать request, subject kind/id/ordinal,
state, marker version, source disposition и timestamp; лишний, дублирующий или
неполный tombstone — повреждение. `REQUEST_SOURCE_MISSING` разрешён только
для действительно отсутствующего legacy immutable source; он не используется
для `metadata_hash_only`, `redacted`, `retention_pruned` или explicit
`REQUEST_EVIDENCE_EVICTED`. Для evicted route/policy или legacy evidence
обязателен сохранённый source identity, hash если privacy policy его допускает,
и explicit eviction reason; отсутствие такой metadata является повреждением.

## Atomic export и resource limits

Exporter читает все секции в одном bounded SQLite read snapshot. Он создаёт
временный каталог рядом с destination, пишет только allow-listed files,
проверяет canonical bytes и hashes, ограничивает closure до завершения записи,
затем выполняет flush/fsync файлов и каталога, записывает manifest и detached
signature, снова делает fsync и атомарно переименовывает временный каталог в
новый destination на том же volume. Destination, существующий до начала
операции, не перезаписывается. Ошибка, отмена, превышение лимита или сбой
подписи не публикует частичный bundle; временный каталог удаляется или
остаётся только как явно помеченный staging artifact, который verifier не
принимает.

Нормативные лимиты этапа:

```text
MAX_PROVENANCE_EXPORT_BYTES       = 256 MiB
MAX_PROVENANCE_EXPORT_REQUESTS    = 4096
MAX_PROVENANCE_EXPORT_RECEIPTS    = 16384
MAX_PROVENANCE_EXPORT_FILES       = 100000
MAX_PROVENANCE_FILE_BYTES         = 16 MiB
MAX_VERIFIER_INPUT_BYTES          = 256 MiB
MAX_VERIFIER_RECORDS              = 100000
MAX_VERIFIER_LINE_BYTES           = 16 MiB
MAX_VERIFIER_PROVENANCE_DEPTH     = 128
```

Размер считается по фактическим байтам staging bundle до публикации; base64 в
JSON для captured payload запрещён, чтобы лимит не обходился encoding overhead.
Verifier проверяет те же лимиты до выделения памяти и использует bounded
recursion/visited sets для lineage и shadow graph. Превышение даёт
`REQUEST_PROVENANCE_TOO_LARGE` для request либо `EXPORT_BUNDLE_TOO_LARGE` для
bundle.

## Тесты

### Unit

- canonical envelope hash и `context_projection_hash`, включая
  `context_ledger_hash` и exact JCS formula;
- signed request receipt: Ed25519 signature, внешний trust anchor,
  `previous_receipt_hash`, request/ledger/provider/model/projection linkage;
- каждый receipt signature, key transition, checkpoint boundary и общий chain
  root;
- lineage для первой attempt, retry/fallback и replan;
- `route_policy_hash_shared = true` для общего source и `false` для двух
  независимых sources с одинаковым digest;
- source hash references, direct evidence, summary/prune shadow traversal,
  `context_shadow_source_refs` и `context_shadow_blocks`;
- `model_responses`/`tool_intents`/approval/effect receipt linkage;
- manifest `files` map, file sizes, content checksum, schema versions,
  request/receipt counts, chain roots/checkpoints и detached signature;
- отсутствующая секция, отсутствующая запись, лишний файл, path traversal,
  duplicate JSON key, неизвестная version и изменённый byte дают typed
  `damaged` результат;
- отдельные fixtures для `valid`, `redacted`, `retention_pruned`, legacy
  `hash_only`, `metadata_hash_only`, `REQUEST_EVIDENCE_EVICTED`,
  `REQUEST_SOURCE_MISSING` и повреждения;
- credentials в envelope, block, response или tool args отвергаются и не
  попадают в staging.

### Integration

1. **Полный путь:** Core export → закрытый bundle → offline verify на машине
   без Core, SQLite, workspace и сети.
2. **Замкнутая реконструкция:** request с summary, prune, response и tool
   intent экспортирует все транзитивные originals/blocks/receipts и полностью
   проходит verifier.
3. **Redaction:** пользовательское удаление сохраняет linkage/signatures,
   удаляет запрещённые bytes и даёт `redacted`, а не hash mismatch.
4. **Retention:** возрастное сжатие даёт `retention_pruned`; отсутствие bytes
   с корректным tombstone не считается повреждением.
5. **Legacy states:** `hash_only`, `metadata_hash_only` и
   `REQUEST_EVIDENCE_EVICTED` получают свои typed results; старый действительно
   отсутствующий source получает только `REQUEST_SOURCE_MISSING`.
6. **Tampering:** изменение envelope, любого block, receipt, key history,
   checkpoint, manifest map или section record обнаруживается как damaged с
   конкретным кодом; ни один tampered bundle не проходит по одной подписи
   manifest.
7. **Atomicity:** сбой в каждой точке staging/flush/signature/rename не
   публикует каталог, который verifier может принять.
8. **Limits:** export и verifier отклоняют bundle/request/line/depth выше
   нормативных лимитов без unbounded allocation.

### Property tests

Для каждого сгенерированного accepted full request:

```text
reconstruct(export(request)) == original logical request
hash(reconstruct(export(request))) == envelope_hash
resolve_all(source_refs, shadow_refs, lineage) == closure
```

Генераторы обязаны включать retry/fallback, multi-level summary, prune,
deduplicated blocks, redaction, retention tombstones и mixed response/tool
intents. Для намеренно неполных состояний property test проверяет не полную
реконструкцию, а точное typed-различение от corruption.

## Критерии готовности

1. Offline-проверка не требует доверия к renderer, работающему Core, SQLite,
   workspace, сети или provider settings.
2. Bundle имеет фиксированный versioned format, замкнутую транзитивную
   выборку, manifest с `{file_path: sha256_hash}`, schema versions, request /
   receipt counts, chain roots/checkpoints и состояниями всех requests.
3. Export атомарен (временный каталог + flush/fsync + rename), ограничен
   `MAX_PROVENANCE_EXPORT_BYTES = 256 MiB`, подписан Ed25519 и не содержит
   credentials.
4. Verifier проверяет canonical envelope hash, `context_projection_hash`,
   lineage, `route_policy_hash_shared`, source hashes, tool linkage, каждый
   receipt signature и общий receipt chain.
5. `valid`, `redacted`, `retention_pruned`, legacy `hash_only`,
   `metadata_hash_only`, `REQUEST_EVIDENCE_EVICTED` и `damaged` имеют разные
   machine-readable outcomes; отсутствие bytes без корректного tombstone
   всегда считается повреждением.
6. Bundle содержит `model_responses`, `tool_intents`,
   `context_shadowed_originals`, `context_shadow_blocks`,
   `context_shadow_source_refs`, `provenance_tombstones`, `context_evidence`
   и `receipt_records` для полной замыкаемой реконструкции.
7. Property/integration tests зелёные на полном, redacted, retention-pruned,
   hash-only, metadata-hash-only и tampered fixtures; verifier соблюдает все
   resource limits.
