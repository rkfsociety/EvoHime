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
- optional-поля отсутствуют целиком: `null` запрещён; неизвестные поля в v1
  запрещены; порядок элементов массивов, если они появятся в следующей версии,
  считается значимым.

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
| `receipt_kind` | enum: `pre_action`, `post_action`, `refusal` |
| `timestamp` | UTC RFC 3339 с ровно тремя долями секунды, например `2026-08-16T12:34:56.789Z` |
| `task_id`, `run_id` | typed identifier: 1–128 ASCII bytes, regex `[A-Za-z0-9][A-Za-z0-9._:-]{0,127}` |
| `tool_name` | зарегистрированный tool id, тот же regex и лимит; обязателен для action/refusal |
| `tool_args_hash` | lowercase SHA-256 hex, 64 ASCII; обязателен для action/refusal |
| `result_hash` | lowercase SHA-256 hex, 64 ASCII; только и обязательно для `post_action` |
| `policy_id` | typed identifier; обязателен для action/refusal |
| `policy_decision` | enum: `allowed`, `denied`, `approval_required`, `approved` |
| `approval_id`, `parent_approval_ref` | typed identifier; допускаются только когда решение или parent binding требует approval |
| `previous_receipt_hash` | lowercase SHA-256 hex; отсутствует только у genesis receipt данного key/chain |
| `context_ledger_hash` | готовый lowercase SHA-256 hex из Context Budget Manager; отсутствует только если model call не создавался |
| `model_route` | объект из двух typed identifiers `provider_id` и `model_id`; отсутствует без model call |

Envelope v1 имеет ровно четыре поля: `payload` по схеме выше, `key_id` в формате
`ed25519:<64 lowercase hex>` (SHA-256 raw public key), константу
`signature_algorithm: "Ed25519"` и `signature` как unpadded base64url от
64-byte Ed25519 signature. Любое другое поле запрещено.

Условные требования между `receipt_kind`, result и approval фиксируются через
`oneOf` в JSON Schema и отдельными negative vectors. Этап 01.3 может уточнить
момент создания этих видов receipt, но не может менять их байтовое
представление без новой версии.

## Что означает запрет свободных raw strings

Запрещены не все JSON strings, а **неограниченный пользовательский или runtime
контент без отдельного типа и bound**. Payload/envelope не содержат raw
arguments, result, prompt, response, command, path, stdout/stderr, error text,
provider response или произвольную map metadata. Они содержат только enum,
typed identifiers, timestamp, digest и signature из схемы выше.

Конструктор receipt принимает typed Rust/TypeScript структуры, а не
произвольный JSON. Попытка передать поля, чьё имя case-insensitive содержит
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
- максимальная вложенность JSON: **4** уровня, включая корневой envelope;
- typed identifier: не более **128 ASCII bytes**; `provider_id` и `model_id` —
  не более **128 ASCII bytes** каждый.

Размер ровно на границе принимается, на один byte больше — отклоняется. Одни и
те же константы экспортируются из Rust и дублируются в Electron только под
тестом shared manifest, чтобы drift ломал CI.

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

## Ошибки и reject logic

Первая ошибка определяется в порядке: raw size → UTF-8/JSON/duplicate key →
version → secret-field scan → schema → canonical bytes/size →
key/signature/hash.
Публичные стабильные codes:

- `receipt.too_large`, `receipt.payload_too_large`;
- `receipt.invalid_utf8`, `receipt.invalid_json`, `receipt.duplicate_key`;
- `receipt.unsupported_version`, `receipt.schema_violation`;
- `receipt.secret_field`, `receipt.non_canonical`;
- `receipt.key_unknown`, `receipt.signature_invalid`, `receipt.hash_mismatch`.

Verifier не падает, не исправляет вход и не возвращает частично verified
результат. Diagnostics может добавить bounded локализованное описание, но code
и факт отказа не зависят от текста.

## Артефакты этапа

- `docs/security/receipt-canonical-v1.md` — нормативное описание этого
  контракта и правила добавления версии;
- `contracts/receipts/v1/receipt.schema.json` — JSON Schema 2020-12;
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

- все четыре артефакта существуют и являются единственным источником правил
  v1 для Rust и Electron;
- обе реализации проходят все shared positive/negative vectors и дают
  идентичные canonical bytes;
- численные лимиты и граничные значения совпадают в документации, schema и
  коде;
- version dispatch и stable error codes покрыты тестами;
- подписываются ровно canonical payload bytes, а receipt hash считается от
  canonical signed envelope;
- receipt невозможно сконструировать с raw arguments/results, неизвестным
  полем или secret-like field; тесты подтверждают отсутствие незардактированных
  секретов;
- known-answer проверка запускается одной командой и обязательна в CI.
