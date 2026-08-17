# Этап 01.3: Runtime integration

Этап плана [01 Подписанные hash-chain receipts](01-0-signed-hash-chain-receipts.md).

## Зависимости

Блокирующие:

- 01.1 — canonical payload/envelope v1, поля action_id, action_status,
  refusal_code, shared vectors и лимиты;
- 01.2 — trusted signer, key_id, key-transition history и
  fail-closed/recovery codes;
- существующие PermissionEngine, canonical_call_hash,
  claim_approval_for_call, policy/sandbox и Core-owned EventJournal.

Разблокирует: 03.1 — child workflows связывают действия ребёнка с approval
родителя через parent_approval_ref и тот же exact-call digest.

## Canonical Receipt Payloads (v1)

01.3 не вводит отдельный runtime-формат: payload и envelope всегда соответствуют
01.1 и shared vectors. Runtime обязан создавать следующие варианты:

| kind | Обязательные поля | Запрещённые поля | Дополнительное условие |
| --- | --- | --- | --- |
| `pre_action` | `action_id`, `action_status=prepared`, `tool_args_hash`, policy fields, chain predecessor | `result_hash`, `refusal_code` | `approval_id` только при `policy_decision=approval_required` |
| `post_action` | `action_id`, terminal status, `tool_args_hash`, `result_hash`, policy fields | `refusal_code`, `parent_approval_ref` | approval binding переносится из pre, если он был |
| `refusal` | `action_id`, `action_status=refused`, `tool_args_hash`, policy fields, `refusal_code` | `result_hash` | approval refs только для отказа существующего approval |

`policy_decision` — закрытый enum `allow | deny | approval_required`.
`allow` разрешает dispatch только после durable pre; `deny` создаёт terminal
signed refusal с `policy_denied`; `approval_required` создаёт bounded intent и
не может создавать pre или запускать tool до успешного one-shot claim.
`action_status=prepared` допустим только для `allow` или успешно claimed
`approval_required`; `deny` никогда не создаёт prepared action. Неизвестное
значение policy decision является `receipt.schema_violation`.

Канонизация receipt выполняется в два строго разделённых шага. Сначала Core
строит `canonical_payload` со всеми полями payload, включая
`previous_receipt_hash`, и вычисляет `payload_hash = SHA-256(canonical_payload)`.
Затем signer подписывает именно `payload_hash`; envelope имеет вид
`{payload, signature, key_id, signature_algorithm}`. После этого Core
канонизирует весь envelope и вычисляет `receipt_hash =
SHA-256(canonical_envelope)`. Подпись не входит в `payload_hash`, но входит в
`receipt_hash`; verifier сначала проверяет `receipt_hash`, затем подпись по
`key_id` и `payload_hash`. Таким образом, circular dependency отсутствует, а
`previous_receipt_hash` берётся из текущего chain head до подписи. Нельзя
вычислять `receipt_hash` от «подписанного payload» до того, как подпись уже
существует. `pending_recovery` и unsigned refusal не являются receipt kinds.

## Что этап отдаёт наружу

Один Core-owned execution path, который для каждого mutation action создаёт
подписанные pre/post/refusal receipts, атомарно связывает их с exact-call
approval, сохраняет pending state и после restart никогда не превращает
неизвестный результат в success.

## Канонический action digest и identity

Для каждого tool call Core один раз создаёт UUIDv7 action_id и lowercase SHA-256
tool_args_hash. tool_args_hash не является новым алгоритмом: это существующий
permissions::canonical_call_hash(tool_name, normalized_scope, input), то есть
SHA-256 от:

    tool_name + "\n" + normalized_scope + "\n" + fingerprint_input(input)

`normalized_scope` — результат единственного вызова
`PermissionEngine::normalize_scope(scope)`: UTF-8 canonical representation без
неопределённого порядка полей, с нормализованными разделителями и правилами
путей/идентификаторов, определёнными PermissionEngine. Core не trim-ит, не
lowercase-ит и не меняет scope самостоятельно; при невозможности нормализации
запрос отвергается до hash. Набор canonical scope vectors PermissionEngine
является нормативным источником. Порядок object keys и fingerprint rules берутся
из PermissionEngine; 01.3 не реализует параллельный hash. Approval request.call_hash
обязан побайтно совпадать с tool_args_hash. Этот digest связывает approval
preview, pre receipt, execution claim и post/refusal receipt.

До `fingerprint_input` Core применяет общий лимит
`canonical_call_input_max_bytes` из `contracts/receipts/v1/limits.json` (v1:
262144 bytes после нормализации). Размер считается по UTF-8 canonical input до
SHA-256; превышение даёт `receipt.call_input_too_large`, не создаёт claim/pre
receipt и не запускает tool. Один и тот же лимит обязателен для PermissionEngine,
Core и Electron adapter; renderer не может увеличить его.

result_hash вычисляется как lowercase SHA-256 от bounded canonical result
projection с domain prefix evohime-result-v1\0. Raw result никогда не попадает
в receipt, journal, IPC или diagnostics; для failed/cancelled result хешируется
только typed projection {status, error_category} без error text. Для success
projection использует `output_digest`, а не `result_hash` самого себя; точная
формула и JCS bytes нормативно определены в 01.1.

`action_id` уникален в `receipt_actions` и имеет UNIQUE constraint. Prepare при
коллизии не переиспользует action. Если digest и binding совпадают, выполняется
идемпотентное чтение существующего состояния. Если action уже находится в
`pending_recovery` либо любое поле binding расходится, Core возвращает
`receipt.action_id_conflict` и tool не запускается. Клиент обязан создать новый
UUIDv7 `action_id` для следующей попытки; повторять тот же `action_id` с новым
`approval_id` запрещено. Старый action остаётся исторической записью, а
reconciliation использует отдельные action/approval/call hash.

После создания `pre_action` поля `action_id`, `tool_args_hash`, версия
`fingerprint_input` и `canonical_call_hash` immutable для данного action_id.
Post/refusal используют значения из durable `receipt_actions`, а не пересчитывают
их из нового input. Изменение правил fingerprint между pre и post считается
schema/configuration violation; такой action не закрывается успешным post без
явного reconciliation.

## Execution protocol

Обычный Core path выполняет следующие фазы:

1. **Prepare.** Проверить task/session/tool/permission/scope, canonicalize
   input, вычислить action_id и tool_args_hash, применить текущую policy.
2. **Approval.** При hard deny сразу создать signed refusal. При Ask сохранить
   bounded approval intent, вернуть событие approval.required с approval_id,
   action_id, call_hash, preview и expires_at_ms; pipe request не блокируется
   ожиданием решения.
3. **Retry.** Electron отправляет отдельный retry с теми же
   task/session/tool/scope/input и approval_id. Core повторно проверяет policy и
   все call-binding fields, затем атомарно claims one-shot approval. Изменение
   хотя бы одного поля даёт call_changed; stale/denied/expired даёт
   соответствующий refusal code.
4. **Pre receipt.** До входа в mutation tool append transaction записывает
   signed pre_action с action_status=prepared, action_id, tool_args_hash,
   policy decision, approval refs и chain predecessor.
5. **Execute.** Только после durable pre receipt запускается tool. Вызов
   получает тот же canonical input; второй parser или пользовательский preview
   не может заменить его.
6. **Post/refusal.** После фактического возврата tool записать signed
   post_action с result hash и status succeeded, failed или cancelled. Если
   execution не стартовал из-за claim race, записать refusal с call_changed
   или подходящим code; post не подделывается.

Approval flow строго двухфазный: approval.required → UI decision → retry с
approval_id. Синхронное ожидание в одном named-pipe request запрещено, потому
что тот же connection не может принять решение.

## Approval-Execution binding

Approval record хранит и повторно сравнивает:

- approval_id, task id, optional session id;
- tool name, permission, normalized scope;
- call_hash/tool_args_hash;
- bounded preview и created_at_ms/fixed expires_at_ms;
- state pending|granted|denied|expired|claimed.

Durable approval audit sink расширяет существующий ApprovalAuditEntry полем
call_hash и сохраняет pending/granted/denied/expired decision. In-memory approval
record может быть погашен, но эта bounded audit row остаётся для offline
проверки binding; raw input в неё не попадает.

TTL v1 — **10 минут** от создания approval. В persisted intent сохраняются
`created_wall_at_ms`, `expires_at_ms`, `clock_boot_id`, `created_monotonic_ms` и
`deadline_monotonic_ms`. В течение одного boot authorization claim использует
только `deadline_monotonic_ms`; sleep/hibernate считаются прошедшим временем.
`clock_boot_id` — стабильный идентификатор OS boot session, полученный через
platform clock API, а не случайный process id; Core проверяет его при каждом
расчёте monotonic deadline. Если платформа не предоставляет надёжный boot id,
Core не создаёт новый approval intent и возвращает fail-closed runtime error.
После restart новый monotonic epoch не сопоставляется со старым: Core проверяет
`expires_at_ms` по wall clock только как fail-closed recovery boundary. Если
wall clock ушёл назад, timestamp повреждён или boot identity не совпадает,
intent переводится в `expired/lost`; новый TTL не создаётся. Restart никогда
не продлевает и не пересоздаёт deadline. После TTL claim атомарно переводит
approval в expired и action получает signed `refusal_code=approval_expired`.
Restart инвалидирует pending in-memory approval; persisted intent получает
`recovery_pending` refusal только после recovery/authenticated closure, а не
автоматически возобновляет mutation.

Нормативное race rule: в одной короткой `BEGIN IMMEDIATE` transaction claim
проверяет `clock_boot_id` и deadline, и только если текущее monotonic время
строго меньше `deadline_monotonic_ms` условно меняет `pending` на `claimed`.
Истёкший на границе claim approval получает `approval_expired` и не запускает
tool. Wall clock нельзя использовать для authorization claim в том же boot;
после restart он используется только для fail-closed invalidation. NTP,
sleep/resume и перевод часов не должны продлить TTL. Это правило имеет
приоритет над любым описательным clock wording выше.

Перед claim Core обязан в одном execution gate заново выполнить:

1. authenticated session/task check;
2. exact tool, permission и normalized scope comparison;
3. recompute canonical_call_hash from current input;
4. current hard-deny/policy check. Policy из claim-time является
   authoritative: policy snapshot/digest из Prepare сохраняется только для
   аудита, а при изменении policy Core повторно применяет новую версию и не
   считает старое approval разрешением обхода;
5. approval state and TTL check;
6. signer trust/availability check.

После успешного claim approval удаляется/переходит в claimed до запуска tool.
Claim, durable pre receipt и обновление `receipt_actions` выполняются в одной
короткой `BEGIN IMMEDIATE` transaction, которая завершается до dispatch tool.
Эта транзакция не охватывает permission wait, IPC, signer retry или выполнение
tool. Post receipt и перевод action в terminal state выполняются в другой
короткой `BEGIN IMMEDIATE` transaction с обновлением той же строки
`receipt_actions`; tool execution не удерживает SQLite transaction. Непосредственно
перед вызовом tool Core в отдельной короткой transaction меняет
`dispatch_state=not_started` на `dispatch_state=started` и записывает
`tool_started_at_ms`; только после durable commit этой строки выполняется
dispatch. Возврат tool меняет состояние на `returned` вместе с bounded
result marker. При
коллизии action id или lock failure до pre tool не запускается.
Повторный retry не может выполнить mutation дважды. Failure после claim не
возвращает approval: новая попытка получает новый action_id и новый approval.

### Migration existing approvals

При миграции из PermissionEngine Core не переносит in-memory approvals и не
считает старый grant действующим автоматически. Версионированный migration
step читает только pending durable records, для которых доступны task/session,
tool, permission, normalized scope, exact call hash, created/expires timestamps
и policy snapshot. Для каждой полной записи в одной transaction создаются новый
`receipt_actions` row и связанный `receipt_approval_intents` row с новым
`approval_id`; исходный идентификатор сохраняется только как
`legacy_approval_ref`, а новый claim всё равно требует
нового Core approval и повторной проверки current policy. Неполные, истёкшие
или неаутентифицированные записи помечаются `lost`, не создают intent и не
могут запускать tool. Migration сохраняет bounded count/audit marker, не
переносит raw input/secrets и является idempotent по
`migration_version + legacy_approval_ref`.

## Receipt state machine

Для каждого action_id допустимы только такие переходы:

    prepared
      ├─> succeeded
      ├─> failed
      ├─> cancelled
      ├─> refused       (claim failed before tool start)
      └─> pending_recovery (process crash / signer unavailable after external call)
    refused             ─> terminal
    succeeded/failed/cancelled ─> terminal
    pending_recovery    ─> post terminal или explicit refusal after reconciliation
    quarantined         ─> manual recovery only

`pending_recovery` — internal action-store state, не новый receipt kind или
автоматически создаваемый signed refusal. `recovery_code` — закрытый enum
`signature_failed | external_error | unknown`; неизвестные будущие значения
отвергаются fail-closed. Для него Core сохраняет только action id, pre hash,
tool args hash и bounded recovery code; raw arguments/results не сохраняются.
Recovery не повторяет mutation
автоматически. Пользователь видит tool id, время, recovery code и предупреждение,
что внешний side effect мог произойти; Core не утверждает ни успех, ни его
отсутствие. Допустимы два authenticated flow: (1) новый read-only
reconciliation action проверяет внешний ресурс и закрывает исходный action
signed post со статусом `succeeded`/`failed`/`cancelled`; (2) явное признание
неизвестного результата создаёт signed refusal с canonical refusal_code
`recovery_pending` и оставляет
исходную связь видимой. Ни один flow не повторяет исходный mutation; отмена
закрытия оставляет action pending.

Receipt может утверждать только:

- какой task/run/action/tool и exact-call digest были заявлены;
- какую policy decision и approval reference Core проверил;
- что pre receipt был durable до запуска, а post содержит конкретный status и
  result digest;
- cryptographic chain position, key id и timestamp.

Receipt не утверждает correctness результата, что tool действительно изменил
ожидаемые данные, безопасность внешнего сервиса или policy enforcement за
пределами записанных полей.

## Bounded previews, diagnostics и backpressure

`bounded preview` означает UTF-8 не более 1024 bytes, с truncation только по
границе Unicode scalar value и суффиксом `[truncated]`; исходный input никогда
не сохраняется. Перед scan входные bytes декодируются как UTF-8 в режиме
replacement: каждая некорректная последовательность заменяется одним `U+FFFD`,
а bounded diagnostic marker получает `invalid_utf8=true`. Исходные bytes не
попадают ни в preview, ни в marker. Preview проходит тот же
case-insensitive secret-like scan, а секретные значения заменяются на
`[REDACTED]` до truncation. Recovery/error
diagnostics — не более 512 bytes, только enum-коды и counters; error text,
stdout/stderr, paths и raw results запрещены. Превышение bound отбрасывает
контент, но не action.

`bounded result marker` — canonical JSON row не более **256 bytes** с полями
`schema_version=1`, `result_status=success|failed|cancelled`,
`result_hash` (lowercase SHA-256), `error_category` (enum или `null`),
`returned_at_ms` и `output_present` (boolean). Он хранится в
`receipt_actions`/protected action row только для recovery и не содержит raw
output, error text, paths или provider metadata. Если marker не помещается в
лимит или не проходит schema validation, Core сохраняет `pending_recovery` с
`recovery_code=external_error`, а не создаёт synthetic result.

Лимит 1024 pending actions на task ограничивает память и незавершённые
approval rows; при достижении Core возвращает `receipt.pending_limit` и
backpressure event с текущим count/limit. Лимит конфигурируется только через
Core policy migration, не меняет receipt schema и пересматривается вместе с
01.4 retention.

## Hash-chain append и concurrency

Receipts хранятся в том же Core-owned SQLite events.db, но в отдельных
append-only таблицах:

- receipt_records: schema_version=1, receipt id, action id, kind/status, task/run, key id,
  canonical payload/envelope blobs, receipt hash, previous hash, timestamp;
- receipt_actions: action id, pre receipt hash, terminal receipt hash,
  current internal state, approval id, approval call hash, approval outcome,
  tool args hash, `dispatch_state=not_started|started|returned`,
  `tool_started_at_ms`, bounded result marker и bounded recovery code;
- receipt_chain_heads: один head на key id с последним receipt hash;
- receipt_approval_intents: bounded pending intents, TTL и recovery marker;
  `action_id` — обязательный foreign key на `receipt_actions(action_id)`,
  `approval_id` уникален, а terminal decision записывается в обе строки одной
  transaction. Intent не может существовать без action и не может ссылаться
  на другой action после claim;
- receipt_runtime_guard: singleton row для фаз `recovery_in_progress` и
  `ready`, generation/owner/timestamps и последнего GC run.

В одном task допускается не более **1024** одновременно pending actions или
approval intents; превышение возвращает receipt.pending_limit до запуска tool.
Размер каждой receipt ограничен 01.1, поэтому writer не принимает blobs вне
canonical envelope bounds.

`bounded approval intent` — это одна SQLite row размером не более **4096 bytes**:
UUID `approval_id` и `action_id`, task/session/tool identifiers в пределах
256 bytes каждый, permission и normalized scope не более 2048 bytes суммарно,
64-byte lowercase `call_hash`, preview не более 1024 UTF-8 bytes, policy
snapshot/digest, clock fields из раздела TTL и enum state. Raw input, raw
arguments, secrets и result в intent запрещены. Intent живёт не дольше 10
минут, одноразово переходит `claimed|expired|denied|lost`, после чего
удаляется только ApprovalGC по правилам ниже.

Append выполняется в составе короткой SQLite `BEGIN IMMEDIATE` transaction:
прочитать chain head, canonicalize/sign envelope, вставить receipt, обновить
head и action index. Для pre эта transaction также содержит claim, но никогда
не охватывает выполнение tool; post всегда записывается отдельной transaction.
Нормативная последовательность lock retry: попытка 1 сразу, затем
повтор через 10 ms; попытка 2 через 50 ms; попытка 3 через 250 ms. Если
конфликт сохраняется, mutation не запускается (либо, если tool уже вернул
управление, action остаётся pending_recovery), возвращается
`receipt.chain_conflict` и создаётся durable diagnostic event; автоматический
fallback без цепочного receipt запрещён. Отдельный Core receipt-writer mutex —
оптимизация внутри одного Core instance, а не гарантия корректности.
Единственным владельцем events.db является Core, запущенный supervisor; второй
Core instance отклоняется по launch context до открытия writer path. Источником
межпроцессной сериализации остаётся SQLite `BEGIN IMMEDIATE`. Mutex охватывает
критический участок от чтения chain
head до SQLite commit, включая predecessor verification, signing, receipt
insert, action-index update и head update. Поэтому concurrent actions
получают детерминированный порядок commit, а post receipt не обязан быть
соседним с собственным pre: связь пары идёт через action_id и receipt_actions,
chain — через previous_receipt_hash.

Перед append writer сверяет `receipt_chain_heads` с последним durable receipt
для этого key id. Отсутствующий head для пустого сегмента допускается только
для genesis; несовпадение head/last receipt, неизвестный key id или нарушение
`previous_receipt_hash` создаёт `receipt.schema_violation`, переводит Core в
`read_only_recovery` и запрещает repair/rewrite chain. `SQLITE_BUSY` считается
временным только в пределах трёх lock retries; превышение лимита даёт
`receipt.chain_conflict` и метрику busy/retry, но не выполняет fallback без
receipt.

Receipt transaction и action state commit происходят до IPC event. События
receipt.prepared, receipt.completed, receipt.refused и
receipt.pending_recovery являются bounded projections, не источником истины.
Existing EventJournal остаётся projection/diagnostic sink: после restart replay
сверяет его с durable `receipt_actions` и исправляет только индекс/доставку
событий, но не receipt chain. Durable receipt/action tables являются источником
истины; 01.4 добавляет verify-chain/list/export поверх этих таблиц.

01.3 ничего не удаляет из receipt chain. Retention/compaction принадлежит 01.4
и может удалять только старые сегменты с signed checkpoint; pending actions
никогда не удаляются автоматически.

### ApprovalGC

На старте Core сначала устанавливает в singleton `receipt_runtime_guard`
`phase=recovery_in_progress` через `BEGIN IMMEDIATE` и выполняет всю Recovery
procedure. До атомарной записи `phase=ready` ApprovalGC не запускается. Каждый
проход GC в одной короткой transaction повторно проверяет `phase=ready`,
generation и отсутствие recovery lease; поэтому GC и Recovery не могут удалить
один intent одновременно. Если Recovery стартует во время ожидания GC, она
увеличивает generation, а GC откатывает удаление и повторяет проход позже.

После успешной проверки хранилища и только при `phase=ready` Core запускает
один bounded `ApprovalGC` не чаще одного раза в минуту. В одной короткой
transaction он удаляет
только истёкшие `receipt_approval_intents` в состояниях `expired`, `lost` или
`claimed`, если с момента terminal decision прошло не менее 10 минут, и
сохраняет aggregate audit marker `{run_id, deleted_count, cutoff_ms}` без raw
данных. Pending intents, связанные с `pending_recovery`, не удаляются до
authenticated closure. При незавершённом recovery даже истёкшие intents
остаются до следующего прохода после `phase=ready`. GC не удаляет
`receipt_records`, `receipt_actions` или
protected rows: их retention/compaction и срок хранения определяет 01.4.
Сбой GC создаёт bounded diagnostic и повторяет проход на следующем интервале.

## Mutation, refusal и signer failures

### Bounded protected action row

`bounded protected action row` — отдельная Core-owned SQLite row для случая,
когда tool уже вернул управление, а post receipt ещё нельзя подписать или
сохранить. Row имеет фиксированный canonical binary/JSON-представитель не более
**512 bytes** и содержит только `schema_version`, `action_id`, `pre_receipt_hash`,
`tool_args_hash`, typed `result_status`, `result_hash`, `recovery_code`,
`created_at_ms` и `key_id`. Raw arguments, raw result, error text, stdout/stderr
и paths запрещены. Представитель защищён **AES-256-GCM** с Core-owned 256-bit
storage key; ciphertext, 12-byte nonce и 16-byte authentication tag входят в
лимит 512 bytes. Ключ хранится отдельно от events.db и защищается
платформенным хранилищем секретов согласно 01.2; plaintext row в SQLite
запрещён. При недоступном или повреждённом ключе row не принимается за
достоверную и action остаётся pending без synthetic success. Row удаляется
только после
durable terminal receipt в одной транзакции; повторная подпись использует лишь
проверенный digest/status из row.

Mutation policy:

- любой mutation с policy deny, approval deny/expiry/stale, changed call,
  untrusted key или signer failure получает signed refusal, если signer
  доступен;
- если signer недоступен до запуска, mutation не запускается. Core возвращает
  runtime error `receipt.signer_unavailable` и создаёт durable unsigned audit
  marker (bounded code/action_id/timestamp), но **не создаёт receipt**, не
  выдаёт marker за signed refusal и не продвигает chain;
- если внешний tool уже вернул управление, но post signer/storage недоступен,
  Core сохраняет pending_recovery, result hash/status только в bounded protected
  action row и не объявляет success; повторная подпись выполняется только
  recovery path;
- если signer доступен, но post-signature не удалась из-за canonicalization,
  size limit или другой детерминированной ошибки, action переходит в
  `pending_recovery` с `recovery_code=signature_failed`; повторная подпись
  разрешена только после проверки неизменности action/result digest;
- policy denial/refusal не считается успешной mutation и не продвигает
  action_status beyond refused.

`receipt.pending_recovery` (transport/runtime code 1010) и canonical
`refusal_code=recovery_pending` имеют разные роли: первый сообщает наружу о
внутреннем незакрытом состоянии, второй допустим только в явно authenticated
explicit-unknown reconciliation и является terminal signed refusal.
`pending_recovery` не блокирует chain: pending row не резервирует head, любой
следующий receipt append-ится к фактическому `receipt_chain_heads` на момент
commit. Если post исходного action подписывается позднее, его
`previous_receipt_hash` берётся из фактического head в этот момент; связь с
собственным pre сохраняется через `action_id` и `receipt_actions`, а не через
соседство в chain. Виртуальные узлы и unsigned gaps не создаются.

Stable runtime errors:

receipt.policy_denied, receipt.approval_required, receipt.approval_denied,
receipt.approval_expired, receipt.approval_stale, receipt.call_changed,
receipt.signer_unavailable, receipt.key_untrusted, receipt.chain_conflict,
receipt.pending_recovery, receipt.pending_limit, receipt.action_id_conflict.

Для совместимости внешнего API каждому stable runtime error назначается
числовой код в диапазоне 1001–1012: policy_denied=1001,
approval_required=1002, approval_denied=1003, approval_expired=1004,
approval_stale=1005, call_changed=1006, signer_unavailable=1007,
key_untrusted=1008, chain_conflict=1009, pending_recovery=1010,
pending_limit=1011, action_id_conflict=1012. В signed receipt используется
только строковый `refusal_code` из контракта 01.1; numeric code — транспортное
поле ошибки, не payload.

Канонические `refusal_code` из 01.1 имеют стабильные numeric aliases только
для diagnostics/metrics: `policy_denied=2001`, `approval_denied=2002`,
`approval_expired=2003`, `approval_stale=2004`, `call_changed=2005`,
`signer_unavailable=2006`, `key_untrusted=2007`, `recovery_pending=2008`.
Эти aliases не сериализуются в payload и не заменяют строковый enum.

Terminal signed refusal codes для state machine: `policy_denied`,
`approval_denied`, `approval_expired`, `approval_stale`, `call_changed`,
`key_untrusted` и `recovery_pending`. Они закрывают action в `refused` и
повторный mutation по тому же action запрещают. `signer_unavailable`,
`chain_conflict`, `pending_recovery` и `receipt.schema_violation` являются
runtime/diagnostic codes, а не terminal signed refusal: при их возникновении
без durable terminal receipt action остаётся `pending_recovery` либо
quarantined, и Core не утверждает результат.

Оба mapping-а являются частью общего
`contracts/receipts/v1/version-manifest.json`; Rust, Electron и offline
verifier импортируют один источник и не имеют локальных числовых таблиц.
Тест manifest обязан проверять непрерывность transport range 1001–1012 и
canonical alias range 2001–2008 без пропусков или дубликатов.

## Read-only sampling

Sampling никогда не применяется к mutations, refusals, approvals, failures,
key/trust events или recovery. Для successful read-only actions v1 хранит
deterministic sample:

- configuration — Core-owned audit_sampling_v1 с rate **10%** по умолчанию,
  integer 0–100, применяемый ко всем successful read-only actions Core и
  изменяемый только authenticated Core command `SetAuditSamplingRate`;
  команда проверяет role/session и пишет bounded audit event до применения
  нового значения; marker
  всегда содержит `sampling_policy_version`, поэтому изменение rate не меняет
  интерпретацию старых действий;
- decision — SHA-256 от UTF-8 строки
  `evohime-sample-v1\0` + `action_id` + `\0` + `tool_name` modulo 100 < rate;
  оба идентификатора уже bounded typed strings, разделитель обязателен и
  входит в canonical input, поэтому конкатенация недвусмысленна и одинаково
  реализуется Rust/Electron без locale-зависимого кодирования;
- rate change создаёт audit event с old/new rate; renderer не меняет rate
  напрямую;
- sampled read-only action получает обычный signed pre/post pair; unsampled
  action получает durable unsigned audit marker `action.sampled=false` с
  action id, digest, timestamp и sampling policy version. Marker не является
  receipt, не входит в chain и не содержит raw arguments/results; он доступен
  через audit diagnostics и не может отображаться как cryptographically
  verified;
- failed/cancelled read-only actions всегда записываются, чтобы sampling не
  скрывал ошибки.

При rate `0` mutations, refusals, failures и cancellations всё равно получают
полные signed receipts; sampling может влиять только на успешные read-only
actions.

## Monitoring

Core публикует только bounded counters/histograms без task input, arguments или
result text: `receipt_pre_latency_ms`, `receipt_post_latency_ms`,
`receipt_append_latency_ms`, `receipt_append_busy_retries`,
`receipt_chain_conflicts`, `receipt_schema_violations`,
`approval_pending_count`, `pending_recovery_count`, `quarantined_count`,
`approval_gc_deleted_count`, `recovery_duration_ms`, `recovery_safe_mode` и
`read_only_sampled_count`. Метрики имеют labels только для stable enum
`policy_decision`, `action_status`, `refusal_code`, `recovery_code` и bounded
tool category. Core/IPC diagnostics возвращают текущие counts, а не raw rows;
превышение лимита pending actions, рост recovery/quarantine или вход в
`read_only_recovery` создают bounded alert event. Мониторинг не является
источником истины: receipts и action rows остаются в SQLite.

## Recovery procedure

При старте Core до принятия новых mutations:

1. атомарно захватить singleton recovery guard, установить
   `phase=recovery_in_progress` и увеличить generation; GC до этого момента
   отключён;
2. открыть SQLite journal и выполнить bounded integrity check: `PRAGMA
   quick_check(100)` в read-only connection с лимитом **2 секунды** и максимумом
   100 диагностических строк; любое отличие от ровно `ok`, timeout или ошибка
   чтения переводит Core в `read_only_recovery` safe mode: новые mutations,
   ApprovalGC и chain writes блокируются, но разрешены status/diagnostics,
   export и backup/restore commands. Core не чинит SQLite на месте и не
   переписывает chain; выход из safe mode возможен только после успешной
   повторной проверки восстановленной копии;
3. найти actions в prepared/internal pending_recovery;
4. сопоставить receipt rows по action id, проверить signature/canonical bytes/
   chain predecessor;
5. если post terminal уже durable, закрыть только index (idempotent replay);
6. если post отсутствует, оставить pending, создать recovery diagnostic и
   никогда не synthesise succeeded;
7. восстановить persisted approval intents как expired/lost; для каждого
   `pending_recovery` показать выбор reconciliation/read-only check или explicit
   unknown-result refusal, а signed refusal создать только после нового
   authenticated command и при доступном trusted signer;
8. только после успешного завершения шагов 1–7 атомарно установить
   `phase=ready`; при любой ошибке guard остаётся `recovery_in_progress`, GC и
   mutation path остаются заблокированными.

### Recovery state matrix

| pre durable | tool started | post durable | Recovery guarantee |
| --- | --- | --- | --- |
| нет | нет | нет | action не запускался; unsigned marker или signed refusal, новый запуск только новой action |
| нет | нет | да | orphan post отвергается как `receipt.schema_violation`; chain не переписывается |
| да | нет | нет | pre остаётся durable; claim/intent expired, tool не запускается повторно |
| да | нет | да | закрыть index idempotently после проверки post pairing |
| да | да | нет | `pending_recovery`; reconciliation только новым явным action |
| да | да | да | проверить post и chain, ровно один terminal state |

Комбинации `pre durable=нет` и `tool started=да` отсутствуют из матрицы:
они нарушают durable invariant, потому что `tool_started_at_ms` может быть
записан только после durable pre и dispatch-state transition. Если такая строка
обнаружена, Recovery создаёт bounded diagnostic/runtime error
`receipt.schema_violation`, переводит action в `quarantined`, запрещает любой
повторный dispatch и блокирует mutation path до ручного восстановления из
backup/checkpoint. `schema_violation` не является signed receipt и не может
закрыть action как success или refusal.

`pending_recovery` публикуется наружу как bounded event
`receipt.pending_recovery` с `action_id`, state, reason_code, timestamp и
`requires_reconciliation=true`; raw input/result отсутствуют. UI показывает
состояние «требуется сверка», recovery code и действие «Проверить/закрыть»;
закрытие требует нового authenticated reconciliation command, не повторяет
tool автоматически и не может объявить success без terminal receipt. Event
доставляется
через Core IPC после durable commit и может быть повторно прочитан через 01.4
diagnostics. Export обязан сохранить для action marker в manifest или отдельной
`actions.jsonl` записью `state=pending_recovery`, `recovery_code` и
`requires_reconciliation=true`; это не signed receipt payload и не может быть
истолковано verifier как success.

`ReconcilePendingAction` — отдельная authenticated command. Она всегда требует
новый `action_id`, новый approval и новый exact-call hash; старый pending row
остаётся историческим фактом. Команда может выполнить только read-only
reconciliation capability либо явно закрыть состояние как
`reconciled_manually`/`unknown_result`; она никогда не dispatch-ит исходный
mutation автоматически и не превращает pending в synthetic success.

Persisted receipt/action rows имеют `schema_version=1`; миграция повышает версию
транзакционно с backup, не меняет canonical blobs и не смешивает версии в одной
chain segment. Если ключ отозван или ротирован между pre и post, каждый receipt
проверяется собственным `key_id`; pending action не claim-ится retired key и
переходит в `pending_recovery` до доверенного rotation checkpoint.

Cancellation между pre и tool return записывается как `cancelled` только если
tool подтвердил отмену; если подтверждения нет, состояние остаётся
`pending_recovery`, а повторный запуск запрещён.

Crash injection должен покрывать точки до/после pre append, approval claim,
tool dispatch, tool return, post append и head update. В каждой точке restart
даёт либо ровно один terminal receipt, либо verifiable pending state без
повторного tool execution.

## Проверки

- schema/vector tests: pre, post success/failure/cancelled, refusal для каждого
  refusal_code, action id pairing и status conditions;
- envelope/hash tests: одинаковый canonical payload даёт одинаковый
  `payload_hash`, подпись проверяется по `key_id`, изменение подписи меняет
  `receipt_hash`, а попытка вычислить receipt hash до формирования envelope
  отвергается;
- exact-call tests: изменение tool, task, session, permission, scope, input,
  policy или approval id блокируется; key order equivalent input сохраняет
  call hash;
- execution-marker tests: `tool_started_at_ms` и `dispatch_state` durable до
  dispatch, повторный startup не запускает action повторно, а отсутствие pre
  при `started` переводит action в `quarantined`;
- two-phase IPC test: approval.required не блокирует pipe, retry с exact
  approval_id проходит один раз, повторный retry отклоняется;
- TTL tests на 10 минут, monotonic-only expiry under NTP/sleep-resume, restart invalidation и
  approval_expired;
- restart clock tests: совпадающий boot использует monotonic deadline, новый
  boot проверяет только persisted wall-clock boundary, rollback/битый timestamp
  даёт `expired/lost` и никогда не создаёт новый TTL;
- pre-before-mutation test: при storage/signer failure tool не вызывается;
- hash-chain tests: concurrent append, deletion/reorder/tamper, wrong previous
  hash, duplicate action id и chain-head conflict;
- long-running tool + parallel new action test: новый action может append-иться к
  текущему head, пока первый tool выполняется; pending recovery не создаёт
  виртуальный узел и не блокирует chain;
- crash/restart matrix из восьми состояний recovery не создаёт fake success и не
  запускает mutation повторно;
- result hash tests не оставляют raw output/error/secret в receipt, SQLite,
  IPC, audit или diagnostics;
- sampling tests: deterministic 10%, versioned marker, rate=0, rate change
  audit, 100% failures/refusals/mutations;
- signer rotation test: active key change между pre/post не ломает chain; each
  receipt verifies by its own envelope key id and public history;
- child handoff vector: parent_approval_ref связывает child action с
  родительским approval без нового raw payload;
- observability test confirms bounded events and no claim of correctness/
  policy enforcement beyond fields.
- post-signature failure test сохраняет `pending_recovery` с
  `signature_failed` и исключает повторное выполнение tool;
- protected-action-row tests проверяют размер 512 bytes, authenticated
  AES-256-GCM protection, повреждённый/отсутствующий storage key и удаление row только
  вместе с terminal receipt;
- approval GC tests проверяют TTL, повторяемость прохода, сохранение pending
  recovery rows и отсутствие удаления receipt chain/action rows;
- reconciliation test требует новый action/approval/call hash, проверяет
  read-only reconciliation capability и оставляет исходный pending action
  исторически видимым;
- preview vectors покрывают emoji и combining characters на границе 1024 bytes;
- preview vectors с invalid UTF-8 проверяют замену на `U+FFFD`, отсутствие
  исходных bytes в marker/preview и сохранение лимита 1024 bytes;
- policy-gate tests меняют policy между Prepare и claim и подтверждают, что
  claim использует current policy, а старый approval не обходит новый deny;
- policy/scope vectors проверяют только `allow|deny|approval_required`,
  canonical `normalize_scope` PermissionEngine и отказ при неизвестном decision;
- approval migration tests проверяют idempotent versioned import, новый
  approval_id, `legacy_approval_ref`, discard неполных/истёкших grants и
  отсутствие автоматического dispatch;
- head-integrity tests проверяют genesis-only empty head, mismatch head/last
  receipt, unknown key id, busy retry budget и переход в read-only safe mode;
- monitoring tests проверяют bounded latency/throughput, pending/recovery/
  quarantine counters и отсутствие raw labels;

## Критерии готовности

- mutation execution path реализует durable signed pre → tool → signed post
  или signed refusal; при signer unavailable до execution создаются только
  runtime error и unsigned audit marker, но не unsigned receipt;
- approval binding использует существующий exact call_hash, TTL 10 минут,
  current policy recheck, one-shot claim и двухфазный IPC;
- `receipt_hash` строится после подписания canonical payload envelope, а
  `payload_hash` и envelope signature не смешиваются в циклической формуле;
- `policy_decision`, `normalized_scope` и bounded result marker имеют
  фиксированные enum/форматы, лимиты и shared vectors;
- SQLite receipt/action tables, chain-head transaction и recovery procedure
  задокументированы и проходят crash/concurrency tests;
- `receipt_approval_intents.action_id` имеет обязательную связь с
  `receipt_actions.action_id`, а terminal approval decision фиксируется
  атомарно в обеих таблицах;
- bounded protected action row имеет фиксированный формат, лимит 512 bytes,
  authenticated protection и удаляется только вместе с terminal receipt;
- bounded recovery integrity check использует `PRAGMA quick_check(100)` с
  двухсекундным лимитом, не выполняет in-place repair и переводит Core в
  read-only safe mode при failure;
- recovery vectors покрывают внешний side effect до/после crash, оба
  authenticated reconciliation flow и отсутствие автоматического повторного
  mutation;
- startup ordering tests подтверждают recovery guard до ApprovalGC,
  generation/lease protection от race и отсутствие удаления intent до
  `phase=ready`;
- action_id, action_status, refusal_code, pre/post pairing и
  previous_receipt_hash проверяются shared vectors/schema из 01.1;
- старый/новый key id и rotation history корректно проверяются offline;
- read-only sampling детерминирован, настраиваем только Core command, а
  mutations/refusals/failures всегда полностью аудируются;
- restart не продлевает persisted monotonic deadline, а chain append при
  pending_recovery использует фактический head и не создаёт виртуальных узлов;
- claim+pre и post используют отдельные короткие SQLite transactions, tool
  никогда не выполняется под удерживаемой transaction;
- `recovery_code` ограничен enum `signature_failed | external_error | unknown`,
  protected action row использует AES-256-GCM и ApprovalGC очищает только
  истёкшие intents по описанному TTL;
- post receipt использует фактический head на момент собственного append,
  проверяется как самостоятельный chain segment, а связь с pre всегда
  подтверждается `action_id`/action row без требования соседства;
- bounded monitoring покрывает latency, throughput/busy retries,
  pending/recovery/quarantine counts и safe-mode state без raw данных;
- после restart ни один pending action не объявляется success и mutation не
  повторяется автоматически;
- recovery matrix, bounded diagnostics, schema_version и key-rotation boundary
  проверены shared crash/concurrency tests;
- Recovery явно создаёт и обрабатывает `receipt.schema_violation`,
  `dispatch_state/tool_started_at_ms` являются durable invariant, а
  `pre=нет, tool_started=да` не считается обычным recovery-состоянием;
- критерий correctness ограничен фактически подписанными полями, а raw
  arguments/results/error text отсутствуют в receipt и обычных diagnostics.
