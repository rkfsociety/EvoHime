# Этап 01.1: Canonical contract

Этап плана [01 Подписанные hash-chain receipts](01-0-signed-hash-chain-receipts.md).

## Зависимости

Блокирующие: нет. Context Budget Manager уже формирует
`context_ledger_hash` как lowercase SHA-256 hex длиной 64 ASCII-символа. Этап
использует готовое значение как непрозрачный digest и не повторяет алгоритм
ledger.

Разблокирует: все остальные этапы плана 01.

## Что этап отдаёт наружу

Версионированную схему payload/envelope, однозначное canonical JSON encoding,
численные лимиты, стабильные коды отказа и один набор known-answer vectors для
Rust, Electron verifier и будущего offline CLI.

## Граница контракта

Receipt состоит из двух разных объектов:

1. `payload` — данные, которые канонизируются и подписываются;
2. `envelope` — объект `{payload, key_id, signature_algorithm, signature}`.

`signature` не входит в подписываемые bytes. Ed25519 получает ровно
`canonicalize(payload)`, без префикса, завершающего LF или предварительного
SHA-256. `receipt_hash`, на который ссылается следующий receipt, равен
lowercase hex `SHA-256(canonicalize(envelope))` уже после добавления подписи.
Этап 01.1 фиксирует этот стык и тестовый вектор с открытым тестовым ключом;
генерация, защита и ротация реального ключа относятся к этапу 01.2.

### Нормативное определение `receipt_hash`

Для принятого envelope вычисляется ровно:

```text
receipt_hash := lowercase_hex(SHA-256(canonicalize(envelope)))
```

В hash-chain это значение записывается как `previous_receipt_hash` следующего
receipt после проверки и подписи текущего envelope. Хэш payload, подписи,
сырого входного JSON или их конкатенации не является `receipt_hash`. Определение
дублируется в нормативном документе, JSON vectors и формате export.

## Canonical JSON v1

Алгоритм — JSON Canonicalization Scheme (JCS),
[RFC 8785](https://www.rfc-editor.org/rfc/rfc8785), поверх I-JSON:

- кодировка — UTF-8 без BOM; пробелы и завершающий перевод строки отсутствуют;
- имена свойств сортируются по UTF-16 code units, как требует JCS;
- строки сериализуются и экранируются строго по JCS; Unicode-нормализация не
  выполняется, поэтому NFC и NFD остаются разными значениями и разными bytes;
- lone surrogates, duplicate object keys, invalid UTF-8, `NaN`, `Infinity` и
  значения вне I-JSON отклоняются до канонизации;
- в payload v1 единственное JSON number — `receipt_version` со значением `1`;
  float и остальные числа запрещены схемой, чтобы Rust и JavaScript не
  расходились из-за IEEE-754; время и digest представлены строками;
- optional-поля отсутствуют целиком: `null` запрещён в объектах и массивах;
  `undefined` не является JSON-значением и отвергается typed-конструктором;
  неизвестные поля в v1 запрещены; порядок элементов массивов, если они
  появятся в следующей версии, считается значимым.

Verifier принимает JSON только после проверки исходного размера, разбирает его
парсером с обнаружением duplicate keys, валидирует схему, заново канонизирует и
сравнивает bytes. Для хранимого canonical receipt несовпадение является
`receipt.non_canonical`, а не молчаливым исправлением.

## Схема payload v1

Полный JSON Schema является артефактом этапа. Следующие правила обязательны и
не могут быть ослаблены реализациями:

| Поле | Представление и условие |
| --- | --- |
| `receipt_version` | JSON number, строго `1` |
| `receipt_id` | lowercase UUIDv7 |
| `action_id` | lowercase UUIDv7; один call связывает pre/post/refusal receipts |
| `receipt_kind` | enum: `pre_action`, `post_action`, `refusal` |
| `action_status` | enum: `prepared` для pre, `succeeded`/`failed`/`cancelled` для post, `refused` для refusal |
| `refusal_code` | enum только для `refusal`: `policy_denied`, `approval_denied`, `approval_expired`, `approval_stale`, `call_changed`, `signer_unavailable`, `key_untrusted`, `recovery_pending` |
| `timestamp` | UTC RFC 3339 с ровно тремя долями секунды, например `2026-08-16T12:34:56.789Z` |
| `task_id`, `run_id` | typed identifier: 1–128 ASCII bytes, regex `[A-Za-z0-9][A-Za-z0-9._:-]{0,127}` |
| `tool_name` | зарегистрированный tool id, тот же regex и лимит; обязателен для action/refusal |
| `tool_args_hash` | lowercase SHA-256 hex, 64 ASCII; обязателен для action/refusal |
| `result_hash` | lowercase SHA-256 hex, 64 ASCII; только и обязательно для `post_action` |
| `policy_id` | typed identifier; обязателен для action/refusal |
| `policy_decision` | результат policy evaluation: `allowed`, `denied`, `approval_required`, `approved`; не является approval state |
| `approval_id`, `parent_approval_ref` | typed identifier; допускаются только когда решение или parent binding требует approval |
| `previous_receipt_hash` | lowercase SHA-256 hex; отсутствует только у genesis receipt данного key/chain |
| `context_ledger_hash` | готовый lowercase SHA-256 hex из Context Budget Manager; обязателен при model call и запрещён без model call |
| `model_route` | объект из двух typed identifiers `provider_id` и `model_id`; отсутствует без model call |

Для `context_ledger_hash` verifier независимо от upstream проверяет regex
`^[0-9a-f]{64}$`; поле не принимается как непрозрачная произвольная строка.
Конструктор отклоняет как отсутствие hash при созданном model call, так и
hash/route у action, который model call не создавал.

### Conditional rules

Таблица является нормативной и отражается без расширений в `oneOf` JSON Schema:

| `receipt_kind` | `action_status` | `refusal_code` | `result_hash` | `approval_id` / `parent_approval_ref` |
| --- | --- | --- | --- | --- |
| `pre_action` | `prepared` | запрещён | запрещён | при `policy_decision != approval_required` оба отсутствуют; при `policy_decision=approval_required` `approval_id` обязателен, `parent_approval_ref` запрещён |
| `post_action` | `succeeded`, `failed` или `cancelled` | запрещён | обязателен | `approval_id` обязателен, если pre-receipt имел approval; `parent_approval_ref` запрещён |
| `refusal` | `refused` | обязателен | запрещён | `approval_id` допускается только для отказа уже созданного approval; `parent_approval_ref` допускается только как binding к этому approval |

Для `pre_action` с `policy_decision=approval_required` сначала создаётся
`approval_id`; после успешной проверки approval runtime-переходом становится
`approved`, и это связывается с post-receipt. Отказ policy создаёт `refusal` до
запуска action; post-receipt создаётся только после завершения action, включая
`failed` и `cancelled`. Этап 01.3 может уточнять orchestration, но не bytes.

Для `refusal` допустимы только такие пары `refusal_code` и
`policy_decision`: `policy_denied` → `denied`; `approval_denied`,
`approval_expired`, `approval_stale` и `call_changed` → `approval_required`.
Здесь `policy_decision=approval_required` сохраняет факт исходной policy
оценки; результат пользовательского approval хранится в approval state/action
projection и не подменяет policy decision. Для отказа без созданного approval поля `approval_id` и
`parent_approval_ref` отсутствуют. `parent_approval_ref` — это `approval_id`
родительского approval, к которому привязывается child action; формат —
lowercase UUIDv7.

Envelope v1 имеет ровно четыре поля в нормативном порядке документации
(`payload`, `key_id`, `signature_algorithm`, `signature`): `payload`, `key_id`,
`signature_algorithm` и `signature`. `signature` — unpadded base64url по RFC
4648 §5 (символ `=` запрещён). Любое другое поле запрещено.

UUID имеют канонический RFC 9562 UUIDv7 с дефисами и lowercase hex. Timestamp
проверяется синтаксически как UTC RFC 3339. При runtime-записи Core применяет
политику skew в 5 минут относительно своих часов и отклоняет нарушение с кодом
`receipt.timestamp_skew`. Offline verifier по умолчанию проверяет только
синтаксис, порядок и cryptographic trust экспортированного архива: часы другой
машины не должны сделать архив ложноповреждённым. Режим
`--enforce-timestamp-skew` явно включает ту же 5-минутную проверку; это не
меняет canonical bytes.

`previous_receipt_hash` отсутствует только у единственного genesis receipt для
конкретной `(key_id, chain)` в хранилище. У любого следующего receipt поле
обязательно: отсутствие даёт `receipt.chain_incomplete`, неверное значение —
`receipt.hash_mismatch`. Ротация ключа из этапа 01.2 начинает новую key-chain и
создаёт новый genesis; граница принимается только из доверенного
rotation/checkpoint контекста.

Для вычисления `result_hash` runtime сначала строит bounded canonical result
projection и только затем применяет SHA-256. Projection не содержит raw
результат, prompt, stdout/stderr, path или error text:

```json
{"status":"succeeded","tool_name":"<typed tool id>","output_digest":"<64 lowercase hex>"}
```

`output_digest` — digest bounded typed result projection, вычисленный до
receipt projection; сам `result_hash` в projection не входит. Для неуспешного
terminal action projection имеет вид `{"status":"failed","error_category":"<bounded enum>"}`
или `{"status":"cancelled","error_category":"<bounded enum>"}`. Во всех
случаях `result_hash = lowercase_hex(SHA-256("evohime-result-v1\\0" ||
JCS(result_projection)))`; domain prefix входит в digest input, но не в JSON.

## Что означает запрет свободных raw strings

Запрещены не все JSON strings, а **неограниченный пользовательский или runtime
контент без отдельного типа и bound**. Payload/envelope не содержат raw
arguments, result, prompt, response, command, path, stdout/stderr, error text,
provider response или произвольную map metadata. Они содержат только enum,
typed identifiers, timestamp, digest и signature из схемы выше.

Конструктор receipt принимает typed Rust/TypeScript структуры, а не
произвольный JSON. Secret-field scan выполняется на typed input до
JSON-сериализации; raw JSON verifier не выполняет этот семантический scan, а
отвергает неизвестные поля схемой. Попытка передать поля, чьё имя
case-insensitive содержит
`secret`, `token`, `password`, `api_key`, `apikey`, `authorization`, `cookie`
или `private_key`, отклоняется как `receipt.secret_field`; замена на
`[REDACTED]` внутри подписываемого объекта не применяется. Фактические
arguments/results связываются только digest-полями. Existing redaction до
логирования остаётся отдельным защитным слоем и не является частью canonical
encoding.

## Численные лимиты

Лимиты измеряются в bytes, не в Unicode code points:

- входной serialized envelope: не более **8192 bytes**, проверка до JSON parse;
- `canonicalize(payload)`: не более **4096 bytes**, проверка до подписи и при
  верификации;
- canonical envelope: не более **8192 bytes**;
- максимальная вложенность JSON: **4** уровня от корневого JSON-документа до
  самого глубокого leaf, включая корневой envelope;
- typed identifier: не более **128 ASCII bytes**; `provider_id` и `model_id` —
  не более **128 ASCII bytes** каждый.
- `canonical_call_input_max_bytes`: не более **262144 UTF-8 bytes** после
  нормализации PermissionEngine; превышение отклоняется до hash/claim.

Размер ровно на границе принимается, на один byte больше — отклоняется. Одни и
те же константы публикуются в `contracts/receipts/v1/limits.json` (UTF-8 без
BOM) и импортируются Rust/Electron; generated bindings не редактируются
вручную. Shared manifest является единственным источником численных лимитов.
Лимиты выбраны как bounded operational boundaries: envelope 8192 bytes
покрывает payload, ключ и подпись; payload 4096 ограничивает audit metadata;
128 bytes достаточно для локальных идентификаторов; depth 4 исключает скрытые
вложенные контейнеры.

## Версионирование

- Dispatch выполняется по обязательному `payload.receipt_version` до проверки
  version-specific schema.
- V1 закрыта: добавление/удаление поля, изменение типа, canonical encoding,
  regex, enum, conditional rule или смысла подписываемых bytes требует v2.
- Исправление документации, не меняющее принимаемые значения/bytes, не требует
  новой версии.
- Verifier обязан продолжать проверять все поддержанные старые версии. Для
  неизвестной версии он возвращает `receipt.unsupported_version`, не
  `receipt.signature_invalid` и не объявляет цепочку повреждённой.
- Экспорт хранит исходные canonical bytes, поэтому более новый verifier не
  пересериализует старую запись по новой схеме.

Миграция v1→v2 выполняется отдельным versioned adapter/export path: v1 receipt
не переписывается и не получает новые поля, а v2 verifier обязан сохранить
v1-проверку. Это не разрешает backport изменения в v1.

## Security considerations

Контракт защищает от canonicalization confusion (разные key order/escaping),
duplicate-key и surrogate атак, подмены алгоритма или key id, signature/hash
tampering, replay через action/approval binding и downgrade через обязательный
version dispatch. `key_id` выбирается только из доверенного key registry этапа
01.2; `signature_algorithm` фиксирован и не negotiable. Секретоподобные поля
отбрасываются до сериализации, raw arguments/results не входят в receipt, а
bounded sizes/depth ограничивают memory/CPU abuse. Timestamp не является
источником криптографической свежести: replay/TTL проверяются runtime и
approval policy этапа 01.3.

## Ошибки и reject logic

Для typed-конструктора secret-field scan выполняется до сериализации. Для
входного envelope первая ошибка определяется детерминированно: raw size →
UTF-8/JSON/duplicate key → version → schema → canonical bytes/size →
key/signature/hash.
Публичные стабильные codes:

- `receipt.too_large`, `receipt.payload_too_large`;
- `receipt.invalid_utf8`, `receipt.invalid_json`, `receipt.duplicate_key`;
- `receipt.unsupported_version`, `receipt.schema_violation`;
- `receipt.secret_field`, `receipt.non_canonical`, `receipt.chain_incomplete`;
- `receipt.key_unknown`, `receipt.signature_invalid`, `receipt.hash_mismatch`.
- `receipt.timestamp_skew` — только для runtime и явно включённого offline
  режима; обычная offline verification проверяет timestamp синтаксически.

Verifier не падает, не исправляет вход и не возвращает частично verified
результат. Diagnostics может добавить bounded локализованное описание, но code
и факт отказа не зависят от текста.

## Артефакты этапа

- `docs/security/receipt-canonical-v1.md` — нормативное описание этого
  контракта и правила добавления версии;
- `contracts/receipts/v1/receipt.schema.json` — JSON Schema 2020-12;
- `contracts/receipts/v1/limits.json` — shared manifest лимитов, regex и
  canonical constants;
- `contracts/receipts/v1/version-manifest.json` — версии schemas, limits и
  единый registry stable error codes для Rust/Electron/Core/verifier;
- `contracts/receipts/v1/vectors.json` — единый manifest positive/negative
  vectors;
- `scripts/check-receipt-vectors.ps1` — запускает Rust и Electron consumers
  одного manifest.

Каждый positive vector хранит source JSON, ожидаемые `canonical_payload_hex`,
`canonical_envelope_hex`, `payload_sha256_hex`, `receipt_hash_hex`, public key
и `signature_base64url`. Private test seed допустим только в manifest и явно
помечается non-production. Negative vectors содержат вход, ожидаемый error code
и фазу отказа. Бинарные `.bin` не являются источником истины: ожидаемые bytes
хранятся hex, чтобы manifest оставался переносимым и ревьюемым.

Будущий CLI обязан читать тот же manifest. Его отсутствие не блокирует 01.1;
добавление CLI без прохождения всех существующих vectors запрещено.

## Проверки

- Rust и Electron читают один `vectors.json` и побайтно сравнивают canonical
  payload/envelope, SHA-256 и fixed-key Ed25519 result;
- permutation test меняет порядок object keys, но сохраняет ожидаемые bytes;
- Unicode vectors покрывают UTF-8, escaping, NFC/NFD distinction и lone
  surrogate rejection;
- boundary vectors покрывают 8192/8193 bytes для envelope и 128/129 bytes для
  identifiers; отдельные unit vectors canonicalizer primitive покрывают
  4096/4097 bytes и depth 4/5 независимо от более узкой v1 schema;
- tamper test отдельно меняет каждое поле payload и требует signature failure;
- negative vectors покрывают invalid JSON, duplicate keys, `null`, float,
  unknown field/version, raw content и secret-like field names;
- compatibility test фиксирует `context_ledger_hash` как непрозрачные 64
  lowercase hex и отклоняет любое другое представление;
- `scripts/check-receipt-vectors.ps1` падает, если хотя бы одна реализация
  пропускает vector или получает другой code/hash/bytes.

## Критерии готовности

- все пять артефактов существуют и являются единственным источником правил
  v1 для Rust и Electron;
- обе реализации проходят все shared positive/negative vectors и дают
  идентичные canonical bytes;
- численные лимиты и граничные значения совпадают в документации, schema и
  коде;
- version dispatch и stable error codes покрыты тестами;
- vector с `receipt_version=2` возвращает `receipt.unsupported_version` и не
  помечает chain broken;
- подписываются ровно canonical payload bytes, а receipt hash считается от
  canonical signed envelope;
- receipt невозможно сконструировать с raw arguments/results, неизвестным
  полем или secret-like field; тесты подтверждают отсутствие незардактированных
  секретов;
- known-answer проверка запускается одной командой и обязательна в CI.
- `oneOf` conditional rules, genesis/chain-incomplete semantics, shared limits
  manifest и security considerations покрыты нормативным документом и
  vectors.
