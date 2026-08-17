# Этап 01.2: Key lifecycle

Этап плана [01 Подписанные hash-chain receipts](01-0-signed-hash-chain-receipts.md).

## Зависимости

Блокирующая: этап 01.1 — Ed25519 подписывает ровно canonical payload, а
`key_id`, encoding public key и signature уже являются частью envelope v1.
Изменение этих bytes после реализации этого этапа требует новой receipt
version и новых shared vectors, а не скрытого обновления key manager.

Разблокирует: 01.3 (подпись действий) и 01.4 (offline verification, export и
UI diagnostics).

## Что этап отдаёт наружу

- Windows key manager для генерации, загрузки, подписи, ротации и fail-closed
  recovery;
- защищённый active private key и подписанную append-only историю public keys;
- явную модель доверия с pinned genesis fingerprint;
- отдельный `evohime-verify.exe`, которому не нужны Core, сеть или private key.

## Целевая платформа и границы компонентов

EvoHime — локальный Windows-продукт с минимальной версией из Agent Guide.
Linux `libsecret` и macOS Keychain не входят в этот этап и не являются
критерием готовности. Криптографический backend скрывается за узким
`ReceiptSigner`/`ReceiptVerifier` interface, чтобы будущий platform backend
или TPM не менял receipt v1.

Supervisor работает под учётной записью текущего пользователя, не является
привилегированной службой и не получает private key. Он:

1. создаёт `%LOCALAPPDATA%\EvoHime\receipts\keys\` с теми же проверенными
   owner-only DACL primitives, что runtime launch context;
2. до запуска Core обнаруживает незавершённый rotation journal;
3. запускает Core только с путём к data directory, без key bytes в arguments,
   environment или `session.json`.

Rust Core остаётся единственным владельцем runtime-состояния: его key manager
генерирует/decrypts key, подписывает payload и выполняет rotation transaction.
Electron main и renderer получают только `key_id`, public metadata и
диагностический status. Renderer не читает key files.

## Threat model

Этап защищает от:

- чтения скопированного data directory без Windows user profile;
- чтения key files другим локальным пользователем;
- случайного попадания private material в renderer, IPC, environment, crash
  diagnostics, обычные logs, audit или export;
- подмены, удаления и перестановки public-key transition records;
- crash/power loss в любой точке rotation;
- молчаливой генерации нового genesis при потере или повреждении active key.

Не обещается защита от administrator/kernel compromise, malware с кодом под той
же Windows identity, memory dump уже запущенного Core или подменённого бинарника
EvoHime. DPAPI CurrentUser не создаёт границу между процессами одного
пользователя; renderer sandbox, IPC authentication и signed transitions
остаются отдельными слоями.

## Криптографическая спецификация

- Алгоритм подписи: Ed25519 по RFC 8032.
- Генерация: 32-byte seed из Windows CSPRNG через системный `getrandom`/
  `BCryptGenRandom`; custom PRNG и seed из времени/UUID запрещены.
- Private encoding до защиты: PKCS#8 DER для Ed25519 по RFC 8410. PEM и
  незашифрованный DER на диске запрещены.
- Public encoding: raw 32-byte Ed25519 public key; в JSON — unpadded base64url.
- `key_id`: `ed25519:` + lowercase hex SHA-256 от raw 32 public bytes, как
  зафиксировано в 01.1. UUID или mutable alias не допускаются.
- Signature: raw 64 bytes, в JSON — unpadded base64url; Ed25519 получает ровно
  canonical bytes из 01.1, без pre-hash.
- Все операции, принимающие private key или signature material, используют
  cryptographic library primitives с constant-time key/signature handling;
  project code не выполняет secret-dependent branching, indexing или сравнение.
  Verification обязана быть constant-time; это не обещает защиту от
  administrator/kernel compromise из threat model. Private seed/key type не
  реализует `Debug`, `Display`, `Serialize` или unrestricted `Clone`;
  временные plaintext buffers обёрнуты в zeroizing storage и очищаются после
  построения signer. Signer очищается при controlled shutdown.

Ed25519 не имеет установленного проектом лимита подписей на один key. Rotation
через 90 дней ограничивает период экспозиции, а не исправляет nonce reuse:
Ed25519 deterministic и не принимает внешний nonce.

Все JSON в этом этапе используют один canonical encoder: поля следуют порядку
соответствующей нормативной schema, отсутствуют пробелы и завершающий перевод
строки, кодировка UTF-8 без BOM, duplicate keys запрещены. Для signed object
сначала строится canonical object без поля `signature`, затем подписываются его
bytes; `signature` добавляется только в envelope. Timestamp — RFC 3339 UTC с
ровно миллисекундами, например `2025-01-15T12:34:56.789Z`; offsets, local time и
отсутствующие миллисекунды запрещены.

## Защищённое хранение

Private file:
`%LOCALAPPDATA%\EvoHime\receipts\keys\active-key-v1.json`.

Он содержит только bounded metadata и DPAPI blob. `bounded metadata` означает
только поля таблицы ниже, schema-defined length limits, один key id и один
public key; произвольные поля, comments, paths, labels и private material
запрещены:

| Поле | Правило |
| --- | --- |
| `storage_version` | integer `1` |
| `key_id` | формат из 01.1 |
| `public_key` | raw public key как unpadded base64url |
| `created_at` | canonical UTC timestamp из 01.1 |
| `protected_pkcs8` | base64 от результата DPAPI, не plaintext key |

PKCS#8 защищается Windows DPAPI в **CurrentUser** scope с UI forbidden. Scope
`LocalMachine` запрещён: он позволил бы decrypt любому подходящему процессу
на машине и был бы шире локальной user-модели EvoHime. Directory, active file,
temporary file и rotation journal получают explicit DACL только для current
user SID и SYSTEM, с отключённым inheritance; после каждого atomic replace DACL
проверяется заново. Ошибка DPAPI или DACL не приводит к fallback на plaintext.

Public history:
`%LOCALAPPDATA%\EvoHime\receipts\keys\public-history-v1.jsonl`. Public keys
не секретны, но файл лежит в том же protected directory для уменьшения риска
локальной подмены. Его криптографическая целостность определяется signed
transition records, а не ACL.

`public-history-v1.jsonl` не является runtime source of truth и не участвует в
dual-write. Core коммитит transition, active-key metadata и audit reference в
SQLite одной транзакцией; после commit строит JSONL snapshot из read
transaction, записывает временный файл, flush/fsync-ит файл и directory, затем
делает atomic replace и повторно проверяет DACL. Ошибка export не откатывает
SQLite и не блокирует read/verify Core, но публикует
`key.history_export_failed`, оставляет предыдущий snapshot и помечает offline
export как `stale`. Runtime не импортирует и не исправляет SQLite по JSONL.
Offline verifier работает только с явно переданным snapshot/export и при
missing, truncated или damaged history возвращает `key.history_incomplete`.

История имеет bounded compaction policy. После достижения лимита transition
records Core создаёт signed `KeyHistoryCheckpointV1`, содержащий hash полного
префикса, последний transition hash, key id checkpoint и границу retained
suffix; checkpoint fsync-ится и экспортируется вместе с history до удаления
старых строк. Offline verifier принимает сокращённую историю только при наличии
доверенного checkpoint и проверяет suffix от него. Без checkpoint удаление
истории классифицируется как `key.history_incomplete`, а не как новая genesis.

Private key не экспортируется и не резервируется в v1. Старые private keys
после успешной rotation уничтожаются: для проверки старых receipts нужны
старые **public** keys. Архивирование retired private keys запрещено, поскольку
оно увеличивает последствия компрометации.

## Key transition и chain metadata

Каждая строка `public-history-v1.jsonl` — canonical JSON `KeyTransitionV1`
не более 2048 bytes:

| Поле | Представление |
| --- | --- |
| `transition_version` | integer `1` |
| `transition_id` | lowercase UUIDv7 |
| `created_at` | canonical UTC timestamp |
| `reason` | `initial`, `scheduled`, `manual`, `compromise` или `recovery` |
| `actor` | `system` или `user` |
| `previous_key_id` | отсутствует только для initial genesis; при recovery содержит последний известный key id |
| `new_key_id` | fingerprint нового public key |
| `new_public_key` | raw public key как unpadded base64url |
| `continuity` | `genesis`, `chained`, `compromised` или `broken` |
| `signed_by_key_id` | old key для chained transition, new key для self-signed genesis/broken recovery |
| `signature` | Ed25519 signature от canonical объекта без поля `signature` |
| `previous_transition_hash` | lowercase SHA-256 hex от полной canonical transition record; отсутствует только для initial genesis |

Для обычной scheduled/manual rotation новый public key подписывается старым и
`continuity=chained`. Initial genesis self-signed только для proof of
possession; self-signature не делает его trusted. При reason `compromise`
trust не наследуется даже при доступном old key: continuity становится
`compromised`, и новый fingerprint должен быть подтверждён отдельно. Если old
key потерян, recovery создаёт self-signed transition с `continuity=broken`.

Удаление или частичное повреждение `public-history-v1.jsonl` обнаруживается по
невозможности построить и проверить непрерывную цепочку
`previous_transition_hash` между pinned genesis и key receipt; файл проверяется
построчно, с обнаружением обрыва, дубликата и trailing partial line. Подмена
полей ломает signature; перестановка строк не меняет граф доверия, но duplicate
transition id, fork от одного active key и cycle отклоняются. Active key определяется единственным terminal
transition независимо от его continuity и обязан совпадать с
`active-key-v1.json`.

История ограничена максимум 100 transition records на key lineage. При
достижении лимита rotation блокируется с `key.rotation_limit`; pruning
истории без отдельного подписанного `KeyHistoryCheckpointV1` запрещён.
Checkpoint — canonical JSON не более 4096 bytes с полями `checkpoint_version`,
`checkpoint_id`, `created_at`, `genesis_key_id`, `lineage_id`,
`covered_first_sequence`, `covered_last_sequence`, `covered_prefix_hash`,
`last_transition_hash`, `retained_from_sequence`, `signed_by_key_id` и
`signature`. Signature строится по canonical object без `signature`, а hash —
lowercase SHA-256 от полной canonical record. Verifier требует pinned genesis,
проверяет checkpoint signature, prefix hash и retained suffix; history нельзя
удалять или compact-ить без этого checkpoint.
`ReceiptCheckpointV1` из 01.4 не заменяет его: receipt prefix и
key-transition lineage — независимые структуры и компактифицируются раздельно.

SQLite — единственный mutable source of truth. Dual-write в SQLite и JSONL
запрещён: `public-history-v1.jsonl` строится только как snapshot export после
commit в read transaction.

Нормативные schema и vectors этапа:

- `contracts/receipts/v1/key-transition.schema.json`;
- `contracts/receipts/v1/key-history-checkpoint.schema.json`;
- `contracts/receipts/v1/rotation-state-v1.schema.json`;
- `contracts/receipts/v1/trusted-roots.schema.json`;
- `contracts/receipts/v1/key-transition-vectors.json`;
- `docs/security/receipt-key-lifecycle-v1.md`.

## Модель доверия offline verifier

Криптографически корректная signature не равна доверенной identity. Trust root
— явно pinned genesis `key_id`:

- на исходной машине `%LOCALAPPDATA%\EvoHime\receipts\keys\trusted-roots-v1.json`
  хранит впервые показанный genesis fingerprint только после authenticated
  approved команды `TrustReceiptGenesis`; до подтверждения status остаётся
  `untrusted`, а этап 01.3 не должен разрешать mutation signing;
- при переносе export verifier требует `--trust-key <genesis-key-id>` либо
  отдельный public trust-store, полученный доверенным каналом;
- public key рядом с receipt без pin считается untrusted (TOFU не происходит
  молча);
- нормальная chained rotation наследует trust; `compromised` и `broken`
  требуют новый explicit pin;
- старые receipts проверяются старым public key и не требуют старого private
  key.

`trusted-roots-v1.json` имеет versioned schema, owner-only DACL и пишется через
temporary file → flush/fsync → atomic replace. На старте Core проверяются
version, duplicate roots, schema, DACL и revoked/superseded markers. Удалённый,
повреждённый или отсутствующий trust store при наличии key history даёт
`key.trust_required`: read/verify разрешены, mutation signing запрещён, а
автоматический повторный TOFU запрещён. Self-signed genesis/transition — это
только proof of possession и никогда не trust без pin.

Нормативная `trusted-roots.schema.json` ограничивает root запись полями
`root_version`, `root_id`, `genesis_key_id`, `pinned_at`, `source`, `status` и
`superseded_by`; private/protected key, receipt payload и произвольный channel
text запрещены. `continuity=broken` и `continuity=compromised` в terminal
transition всегда переводят Core в `key.trust_required`: Core разрешает
diagnostics и read-only verification, но запрещает любую подпись, включая
terminal receipt, пока authenticated approved `TrustReceiptGenesis` не создаст
новый explicit trust anchor `TrustReceiptGenesis` для нового сегмента. Сам
transition или self-signature не может снять этот запрет.

Результаты различаются как минимум на `verified`, `untrusted`, `broken` и
`unsupported`. Verifier никогда не преобразует `untrusted` в `verified`
только потому, что registry и receipts лежат в одном каталоге.

## Offline verification command

Этап создаёт отдельные library crate и binary без agent runtime:

- `evohime-receipt-verifier` — canonical, key-transition и signature checks;
- `evohime-verify.exe verify --receipts <path> --key-history <path>`
  `--trust-key <key-id> [--format text|json]`.

Binary не линкует model gateway, tool runtime, SQLite writer или HTTP client,
никогда не открывает сеть и не читает `active-key-v1.json`. Exit codes:

- `0` — вся выбранная цепочка verified;
- `2` — broken/invalid signature/hash/transition;
- `3` — signatures корректны, но trust anchor отсутствует;
- `4` — invalid arguments, unreadable input или unsupported version;
- `5` — цепочка математически проверяема, но содержит `stale_key` boundary;
  это warning-as-failure для offline verification, чтобы revoked/compromised
  segment нельзя было принять как полностью verified.

До этапа 01.4 команда проверяется на shared/synthetic receipts. 01.4 добавляет
производственный JSONL export и упаковку соответствующего public history.

## Rotation journal schema и crash recovery

`%LOCALAPPDATA%\EvoHime\receipts\keys\rotation-state-v1.json` — owner-only
bounded journal, не runtime source of truth. Его schema `rotation-state-v1`
имеет ровно следующие поля: `state_version` (integer `1`), `rotation_id`
(UUIDv7), `phase`, `old_key_id`, `new_key_id`, `transition_hash`, `error_code`,
`created_at`, `updated_at`, `reason`, `actor`, `active_key_observed` и
`audit_event_id`. `phase` принимает только `prepared`, `transition_durable`,
`audit_durable`, `active_key_replaced`, `cleanup_required` и `complete`.
`old_key_id`/`new_key_id`/`transition_hash` обязательны после `prepared`,
`error_code` обязателен только для error phase; private key, DPAPI blob и
arbitrary paths запрещены. Timestamps используют RFC 3339 UTC milliseconds.

Recovery сверяет journal с SQLite transition/audit и active-key metadata, затем
идемпотентно продолжает ровно следующую фазу. `active_key_replaced` допускается
только если durable transition и audit уже подтверждены; при
`continuity=broken` или `compromised` journal не может разрешить подпись новым
ключом. Несогласованный или повреждённый journal даёт
`key.rotation_incomplete` и блокирует mutation/rotation до manual recovery.

## Rotation policy

- Scheduled rotation: key age **90 календарных дней**. Проверка выполняется при
  старте Core и затем не чаще раза в 24 часа; пропущенный срок вызывает rotation
  при следующем доступном check, а не background service.
- Manual rotation: authenticated Core command `RotateReceiptKey` с approval и
  обязательным reason `manual` или `compromise`.
- Значение 90 дней фиксировано для v1. Настройка срока и TPM/HSM backend —
  отдельное последующее решение; скрытый environment override в production
  запрещён.

Rotation — journaled transaction:

1. сгенерировать new key, DPAPI-protect его во temporary file и проверить
   decrypt/sign/verify round trip;
2. старым key подписать transition, выполнить SQLite commit и записать
   owner-only `rotation-state-v1.json` с phase и hashes;
3. durable append transition и bounded audit event;
4. atomic replace active key, повторно проверить DACL и соответствие key id;
5. удалить old protected private material и journal, затем выполнить
   self-verification public history.

До завершения шага 4 Core подписывает только old key. После шага 4
cryptographic signer — только new key, но Core разрешает receipt signing только
для `continuity=chained` с действующим trust; для `broken` и `compromised` он
немедленно остаётся в `key.trust_required` до explicit pin. Crash recovery
идемпотентно продолжает либо откатывает transaction по
journal; состояние, где active key сменился без transition и audit, не
допускается. Ошибка удаления old private material оставляет status
`cleanup_required`, блокирует следующую rotation и не объявляется успехом.
После успешной rotation `rotation-state-v1.json` удаляется только после
повторной проверки active key, signed transition, audit event и DACL. Если
очистка не завершилась, файл сохраняется с bounded phase/error code и recovery
идемпотентно повторяет только cleanup.

Каждая запись или обновление `rotation-state-v1.json` выполняется как
temporary file → flush/fsync → atomic replace, с повторной проверкой owner-only
DACL. Core обязан прочитать и проверить journal при старте. Повреждённый journal
не удаляется молча: если active key, transition и audit нельзя однозначно
сопоставить, rotation блокируется с `key.rotation_incomplete` до ручного
вмешательства; если они согласованы, journal можно удалить после bounded audit.

## Audit и diagnostics

Rotation пишет bounded structured event в существующий Core audit trail
`%LOCALAPPDATA%\EvoHime\logs\audit.jsonl`:

`bounded` означает не более 4096 bytes, UTF-8 без BOM, один event object,
поля только из schema, без prompt, paths, public/private key bytes, signatures,
DPAPI blob или arbitrary error text. Поля `old_key_id`, `new_key_id` и
`transition_hash` — bounded identifiers/digests; подробности остаются в
стабильном `error_code`.

- `event_type`: `key.generated`, `key.rotated`,
  `key.recovery_required` или `key.rotation_failed`;
- `timestamp`, `old_key_id` при наличии, `new_key_id` при наличии;
- `reason`, `actor`, `transition_hash`, `outcome`, `error_code`.

`transition_hash` — lowercase SHA-256 hex от полной canonical transition record
с signature. Повторная запись audit при crash recovery использует тот же
`transition_id`/hash и дедуплицируется, а не создаёт вторую rotation.

Public key bytes, signature и private/protected bytes в audit не пишутся.
Authoritative proof rotation — signed transition; audit нужен для локальной
диагностики и будущего UI этапа 01.4 и сам по себе не является trust root.
Rotation считается завершённой только после durable transition и успешной
записи audit event.

Стабильные error codes: `key.not_initialized`, `key.dpapi_failed`,
`key.dacl_invalid`, `key.corrupt`, `key.public_mismatch`,
`key.rotation_incomplete`, `key.rotation_fork`, `key.cleanup_required`,
`key.trust_required`, `key.rotation_limit`, `key.history_export_failed`,
`key.history_incomplete`.

## Потеря, повреждение и компрометация

- Missing/corrupt/undecryptable active key при существующей history — hard
  `key.recovery_required`. Core не создаёт replacement автоматически и не
  подписывает receipts.
- Пользователь может выполнить отдельную approved recovery
  `CreateNewReceiptGenesis`. Она сохраняет forensic metadata без key bytes,
  создаёт reason `recovery`/continuity `broken`, новый пока untrusted genesis и
  audit event. Отдельная команда `TrustReceiptGenesis` подтверждает новый
  fingerprint. Старые receipts остаются проверяемыми по public history, но
  новый сегмент не выдаётся за продолжение старой identity.
- При подозрении на компрометацию выполняется manual rotation с reason
  `compromise`; old public key и старые receipts не удаляются. Verifier
  показывает compromised boundary по порядку transition commit/sequence, а не
  по wall-clock timestamp; receipts старого key до boundary остаются
  `verified`, после boundary получают `stale_key`. Новый key до отдельного pin
  остаётся `unverified`, после pin проверяется как новый сегмент. Receipts
  с математически валидной подписью отозванного сегмента получают
  `stale_key`; v1 не вводит per-receipt CRL, поэтому отзыв отдельного receipt
  достигается только компрометацией/ротацией соответствующего key segment.
- Backup/export содержит receipts и public history, но никогда private key или
  DPAPI blob. Импорт private key и автоматическое восстановление из cloud не
  входят в v1.

После этапа 01.3 недоступный signer блокирует mutation до создания pre-action
receipt; read-only диагностика и offline verification продолжают работать.

## Проверки

- shared Ed25519/RFC 8410 known-answer vectors, malformed key/signature и
  `key_id` mismatch;
- first-run test проверяет CSPRNG key, DPAPI CurrentUser round trip, explicit
  DACL и отсутствие plaintext PKCS#8 на диске;
- scan source, renderer bundle, environment capture, IPC frames, audit,
  diagnostics, crash dump fixture и export fixtures на test seed/private-key
  markers; число совпадений равно нулю вне test vectors;
- scheduled/manual/compromise rotation и old-receipt verification после Core
  shutdown без private file и с network denied;
- crash injection после каждого шага transaction: после restart существует
  ровно один active signer, transition/audit согласованы, fork отсутствует;
- crash/partial-write tests для rotation journal и trust store проверяют
  fsync/atomic replace, DACL, fail-closed recovery и отсутствие автоматического
  TOFU;
- удаление/reorder/tamper/fork/cycle public transition records даёт отдельный
  deterministic error;
- missing/corrupt DPAPI blob не создаёт key молча; approved recovery создаёт
  broken boundary и новый untrusted до pin сегмент;
- compromise vectors различают old receipts до transition sequence
  (`verified`), после boundary (`stale_key`) и новый key до/после explicit pin;
- verifier возвращает exit code `5` для `stale_key`, а stale export после
  неудачного JSONL export не объявляется verified;
- rotation-state schema и crash tests покрывают каждую phase, а checkpoint
  vectors покрывают prefix hash, retained suffix и signature;
- другой Windows user и скопированный data directory не decrypt private key;
- shutdown/restart очищает plaintext buffers; secret types не поддерживают
  debug/serialization и покрыты compile-time/API tests;
- release x64 verifier проверяет 1000 synthetic receipts не более чем за 2
  секунды p95 на локальном SSD после прогрева кэша; это regression budget, а не
  криптографическая гарантия.

## Критерии готовности

- алгоритм, форматы, storage paths, DACL/DPAPI scope и key id совпадают в
  документации, коде и shared vectors;
- verifier после shutdown Core и при заблокированной сети проверяет receipts
  только по public history и explicit trust anchor;
- нормальная rotation сохраняет проверяемость старых receipts, но не сохраняет
  old private key;
- genesis, chained, compromised и broken trust boundaries различаются и
  тестируются;
- rotation атомарна относительно active key, signed transition и audit либо
  восстанавливается из journal после crash;
- потеря key fail-closed и никогда не маскируется автоматическим replacement;
- private material отсутствует в renderer, IPC, environment, logs, audit,
  diagnostics и export;
- offline verifier, key-transition, rotation-state и trusted-roots schemas,
  checkpoint/key-transition vectors и threat-model document существуют и
  входят в packaging/CI.
