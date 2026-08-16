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

`receipt_hash` — SHA-256 canonical signed envelope, а `previous_receipt_hash`
берётся из текущего chain head. `pending_recovery` и unsigned refusal не
являются receipt kinds.

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

Порядок object keys, scope normalization и fingerprint rules берутся из
PermissionEngine; 01.3 не реализует параллельный hash. Approval request.call_hash
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
только typed projection {status, error_category} без error text.

`action_id` уникален в `receipt_actions` и имеет UNIQUE constraint. Prepare при
коллизии не переиспользует action: если digest и binding совпадают, выполняется
идемпотентное чтение существующего состояния; при любом расхождении возвращается
`receipt.action_id_conflict` и tool не запускается.

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

TTL v1 — **10 минут** от создания approval; clock checks используют monotonic
deadline, wall expires_at_ms служит только для UI/diagnostics. После TTL claim
атомарно переводит approval в expired и action получает signed refusal_code=
approval_expired. Restart инвалидирует pending in-memory approval; persisted
intent получает recovery_pending refusal после recovery, а не автоматически
возобновляет mutation.

Нормативное race rule: `expires_at_ms` хранится в БД вместе с intent. В одной
BEGIN IMMEDIATE transaction claim сначала проверяет wall/monotonic deadline и
только если текущее время строго меньше `expires_at_ms` условно меняет
`pending` на `claimed`; истёкший на границе claim approval получает
`approval_expired` и не запускает tool. Это правило имеет приоритет над любым
описательным clock wording выше.

Перед claim Core обязан в одном execution gate заново выполнить:

1. authenticated session/task check;
2. exact tool, permission и normalized scope comparison;
3. recompute canonical_call_hash from current input;
4. current hard-deny/policy check;
5. approval state and TTL check;
6. signer trust/availability check.

После успешного claim approval удаляется/переходит в claimed до запуска tool.
Claim, durable pre receipt и обновление `receipt_actions` выполняются в одной
BEGIN IMMEDIATE transaction. Post receipt и перевод action в terminal state
выполняются в одной BEGIN IMMEDIATE transaction с обновлением той же строки
`receipt_actions`; tool execution не удерживает SQLite transaction.
Повторный retry не может выполнить mutation дважды. Failure после claim не
возвращает approval: новая попытка получает новый action_id и новый approval.

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

pending_recovery — internal action-store state, не новый receipt enum. Для него
Core сохраняет только action id, pre hash, tool args hash и bounded recovery
code; raw arguments/results не сохраняются. Recovery не повторяет mutation
автоматически. Пользователь видит tool id, время, recovery code и предупреждение,
что внешний side effect мог произойти; Core не утверждает ни успех, ни его
отсутствие. Допустимы два authenticated flow: (1) новый read-only
reconciliation action проверяет внешний ресурс и закрывает исходный action
signed post со статусом `succeeded`/`failed`/`cancelled`; (2) явное признание
неизвестного результата создаёт signed refusal с `recovery_pending` и оставляет
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
не сохраняется. Preview проходит тот же case-insensitive secret-like scan, а
секретные значения заменяются на `[REDACTED]` до truncation. Recovery/error
diagnostics — не более 512 bytes, только enum-коды и counters; error text,
stdout/stderr, paths и raw results запрещены. Превышение bound отбрасывает
контент, но не action.

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
  tool args hash и bounded recovery code;
- receipt_chain_heads: один head на key id с последним receipt hash;
- receipt_approval_intents: bounded pending intents, TTL и recovery marker.

В одном task допускается не более **1024** одновременно pending actions или
approval intents; превышение возвращает receipt.pending_limit до запуска tool.
Размер каждой receipt ограничен 01.1, поэтому writer не принимает blobs вне
canonical envelope bounds.

Append выполняется одной SQLite BEGIN IMMEDIATE transaction: прочитать chain
head, canonicalize/sign envelope, вставить receipt, обновить head и action
index. При lock conflict выполняются три попытки с backoff 10/50/250 ms. Если
конфликт сохраняется, mutation не запускается (либо, если tool уже вернул
управление, action остаётся pending_recovery), возвращается
`receipt.chain_conflict` и создаётся durable diagnostic event; автоматический
fallback без цепочного receipt запрещён. Отдельный
Core receipt-writer mutex охватывает весь критический участок от чтения chain
head до SQLite commit, включая predecessor verification, signing, receipt
insert, action-index update и head update. Поэтому concurrent actions
получают детерминированный порядок commit, а post receipt не обязан быть
соседним с собственным pre: связь пары идёт через action_id и receipt_actions,
chain — через previous_receipt_hash.

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

## Mutation, refusal и signer failures

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

## Read-only sampling

Sampling никогда не применяется к mutations, refusals, approvals, failures,
key/trust events или recovery. Для successful read-only actions v1 хранит
deterministic sample:

- configuration — Core-owned audit_sampling_v1 с rate **10%** по умолчанию,
  integer 0–100, изменяемый только authenticated settings command;
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

## Recovery procedure

При старте Core до принятия новых mutations:

1. открыть SQLite journal и выполнить bounded integrity check;
2. найти actions в prepared/internal pending_recovery;
3. сопоставить receipt rows по action id, проверить signature/canonical bytes/
   chain predecessor;
4. если post terminal уже durable, закрыть только index (idempotent replay);
5. если post отсутствует, оставить pending, создать recovery diagnostic и
   никогда не synthesise succeeded;
6. восстановить persisted approval intents как expired/lost; для каждого
   `pending_recovery` показать выбор reconciliation/read-only check или explicit
   unknown-result refusal, а signed refusal создать только после нового
   authenticated command и при доступном trusted signer;
7. разрешить mutation path только после signer/trust/chain readiness.

### Recovery state matrix

| pre durable | tool started | post durable | Recovery guarantee |
| --- | --- | --- | --- |
| нет | нет | нет | action не запускался; unsigned marker или signed refusal, новый запуск только новой action |
| нет | нет | да | orphan post отвергается как `receipt.schema_violation`; chain не переписывается |
| нет | да | нет | external side effect неизвестен; `pending_recovery`, повтор запрещён |
| нет | да | да | проверить signature/chain, закрыть action idempotently |
| да | нет | нет | pre остаётся durable; claim/intent expired, tool не запускается повторно |
| да | нет | да | закрыть index idempotently после проверки post pairing |
| да | да | нет | `pending_recovery`; reconciliation только новым явным action |
| да | да | да | проверить post и chain, ровно один terminal state |

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
- exact-call tests: изменение tool, task, session, permission, scope, input,
  policy или approval id блокируется; key order equivalent input сохраняет
  call hash;
- two-phase IPC test: approval.required не блокирует pipe, retry с exact
  approval_id проходит один раз, повторный retry отклоняется;
- TTL tests на 10 минут, monotonic expiry, restart invalidation и
  approval_expired;
- pre-before-mutation test: при storage/signer failure tool не вызывается;
- hash-chain tests: concurrent append, deletion/reorder/tamper, wrong previous
  hash, duplicate action id и chain-head conflict;
- crash/restart matrix из восьми состояний recovery не создаёт fake success и не
  запускает mutation повторно;
- result hash tests не оставляют raw output/error/secret в receipt, SQLite,
  IPC, audit или diagnostics;
- sampling tests: deterministic 10%, rate change audit, 100% failures/refusals/
  mutations;
- signer rotation test: active key change между pre/post не ломает chain; each
  receipt verifies by its own envelope key id and public history;
- child handoff vector: parent_approval_ref связывает child action с
  родительским approval без нового raw payload;
- observability test confirms bounded events and no claim of correctness/
  policy enforcement beyond fields.
- post-signature failure test сохраняет `pending_recovery` с
  `signature_failed` и исключает повторное выполнение tool;

## Критерии готовности

- mutation execution path реализует durable signed pre → tool → signed post
  или signed refusal; при signer unavailable до execution создаются только
  runtime error и unsigned audit marker, но не unsigned receipt;
- approval binding использует существующий exact call_hash, TTL 10 минут,
  current policy recheck, one-shot claim и двухфазный IPC;
- SQLite receipt/action tables, chain-head transaction и recovery procedure
  задокументированы и проходят crash/concurrency tests;
- recovery vectors покрывают внешний side effect до/после crash, оба
  authenticated reconciliation flow и отсутствие автоматического повторного
  mutation;
- action_id, action_status, refusal_code, pre/post pairing и
  previous_receipt_hash проверяются shared vectors/schema из 01.1;
- старый/новый key id и rotation history корректно проверяются offline;
- read-only sampling детерминирован, настраиваем только Core command, а
  mutations/refusals/failures всегда полностью аудируются;
- после restart ни один pending action не объявляется success и mutation не
  повторяется автоматически;
- recovery matrix, bounded diagnostics, schema_version и key-rotation boundary
  проверены shared crash/concurrency tests;
- критерий correctness ограничен фактически подписанными полями, а raw
  arguments/results/error text отсутствуют в receipt и обычных diagnostics.
