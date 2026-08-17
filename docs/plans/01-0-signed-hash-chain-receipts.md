# План 01: Подписанные hash-chain receipts

Обзор плана. Этапы вынесены в отдельные файлы и ревьюятся по одному.

## Цель

Сделать действия Евы проверяемыми офлайн: для каждого mutation tool call и
каждого action с внешним side effect создавать bounded receipt, подписанный
Core-ключом и связанный с предыдущим receipt. Read-only actions без side effect
могут проходить детерминированный sampling по правилам 01.3; failures и refusals
никогда не sampling-ятся. Критерий значимости вычисляет Core по registry/policy,
а не renderer. Действие считается значимым, если оно изменяет состояние
EvoHime/workspace/локального ресурса, вызывает внешний сервис или иной внешний
side effect, либо является refusal, failure, cancellation или recovery-событием
для такого действия. Только read-only вызов без side effect и без ошибки может
попасть в sampling; renderer не может понизить значимость. Receipt доказывает
авторство ключа, целостность и порядок, но не доказывает правильность действия.

## Модель доверия

- Private signing key создаёт и хранит supervisor/Core через Windows-protected
  storage; в source, renderer и обычные logs ключ не попадает.
- Public-key history доступна для локальной проверки и экспорта пользователем;
  доверие начинается с явно подтверждённого genesis fingerprint, а не с
  молчаливого TOFU.
- После rotation старый private key уничтожается: исторические receipts
  проверяются по сохранённому public key, а новый key-chain начинается с
  доверенного genesis/checkpoint контекста rotation.
- Receipt подписывает canonical payload; raw arguments/result заменяются hash.
- Approval и action используют один `action_digest`, равный существующему
  `tool_args_hash = permissions::canonical_call_hash(...)`; отдельное поле не
  дублирует digest в Receipt v1.

## Receipt v1

Подписываемый canonical payload содержит:

- `receipt_version`, `receipt_id`, `action_id`, `timestamp`, `task_id`, `run_id`;
- `tool_name`, `tool_args_hash`, `result_hash`, `policy_id`, `policy_decision`;
- `action_status` (`prepared`, `succeeded`, `failed`, `cancelled`, `refused`);
- `refusal_code` для refusal (`policy_denied`, `approval_denied`,
  `approval_expired`, `approval_stale`, `call_changed`, `signer_unavailable`,
  `key_untrusted`, `recovery_pending`);
- `approval_id`/`parent_approval_ref` при необходимости;
- `previous_receipt_hash`, `context_ledger_hash`, `model_route`.

Подпись не может входить в подписываемый payload. Signed envelope отдельно
содержит `payload`, `key_id`, `signature_algorithm` и `signature`; точный формат,
лимиты и правила версионирования задаёт этап 01.1. `previous_receipt_hash` —
lowercase hex SHA-256 от canonical signed envelope предыдущего receipt, а не от
payload или raw JSON. `chain` — это последовательность receipts, подписанных
одним `key_id` и связанных через `previous_receipt_hash`, начиная с genesis
receipt. Каждый `(key_id, chain)` имеет ровно один genesis; новый ключ начинает
новую chain только после проверки trusted rotation/checkpoint history.

## Этапы

| Этап | Файл | Что отдаёт наружу | Кто потребляет |
| --- | --- | --- | --- |
| 01.1 | [Canonical contract](../security/receipt-canonical-v1.md) | canonical encoding payload и known-answer vectors | 01.2–01.4 |
| 01.2 | [Key lifecycle contract](../security/receipt-key-lifecycle-v1.md) | key pair, rotation и offline verification | 01.3, 01.4 |
| 01.3 | [Runtime integration](01-3-runtime-integration.md) | pre/post-action receipts и approval binding | 03.1 |
| 01.4 | [Chain storage и export](01-4-chain-storage-and-export.md) | verify-chain, IPC и UI | UI |

## IPC и UI

- Read-only `ListReceipts`, `VerifyReceipts`, `ExportReceipts` с limit и date
  bounds.
- UI показывает status `verified`, `broken`, `unverified`, key id и hashes;
  не показывает секретный payload автоматически.
- Approval preview должен показывать action digest, чтобы пользователь видел,
  что именно он подтверждает.
- При chain break UI получает bounded diagnostic с `error_code`, expected и
  actual hash; mutation path блокируется до signer/trust/chain readiness.

## Зависимости плана

Блокирующие: нет. Context Budget Manager реализован: payload содержит `context_ledger_hash`, а ledger
определён именно там; существующие approval, exact-call hash, diagnostics и
Core-owned storage.

Опциональных интеграций нет. Что этот план обязан предоставить: этап 01.3
даёт child workflows (этап 03.1) связь действий ребёнка с approval родителя,
а 01.4 — verify-chain для их аудита.

## Canonical contract, threat model и failure modes

Полный canonical JSON/JCS, Ed25519 envelope, SHA-256 domains, limits, vectors,
negative cases и version dispatch нормативно определены в 01.1; этот обзор не
создаёт второй источник правил. `context_ledger_hash` — lowercase hex SHA-256
ровно 64 ASCII bytes, вычисляется Context Budget Manager; нормативная схема
ledger hash находится в `docs/architecture.md` и не дублируется этим планом.
Verifier валидирует его независимо от upstream.

Adversary model включает скомпрометированный renderer, подменённый IPC input,
повтор старого approval/receipt, повреждение SQLite и попытку подменить key
history. Контракт предотвращает raw-payload leakage, canonicalization/key
confusion, replay через unique action/receipt ids, chain forks и unsigned
mutation. Он не доказывает correctness внешнего сервиса, не лечит уже
совершённый внешний side effect и не защищает private key после компрометации
машины.

При signer/storage failure до запуска mutation Core fail-closed: выдаёт runtime
error и unsigned audit marker, но не создаёт receipt и не запускает tool. При
неизвестном результате после tool действие остаётся `pending_recovery`; успех
не синтезируется. При повреждении chain verifier возвращает стабильный error,
показывает diagnostic и блокирует новые mutations до reconciliation/checkpoint.
Потеря ключа делает соответствующую chain unverified до восстановления trusted
public-key history; private key не восстанавливается из receipts.

Производительность и retention задаются этапами: 01.3 ограничивает runtime
preview/diagnostics и транзакционные retries, 01.4 задаёт signed checkpoints,
retention и offline verify. Целевой smoke-budget: sign/append не блокирует IPC,
а verify 1000 receipts выполняется асинхронно UI и укладывается в 2 секунды p95
на локальном SSD после прогрева кэша; превышение измеряется отдельной
метрикой.

## Критерии готовности плана

- любое mutation action имеет receipt или явный refusal с причиной;
- verifier работает без сети и без private key;
- chain break виден пользователю и в diagnostics;
- receipt не утверждает correctness/policy enforcement сверх фактически
  проверенных полей;
- текущий audit trail остаётся совместимым на переходный период;
- replay/duplicate action проверяется unique constraints, chain head и offline
  verifier; nonce не добавляется в v1, потому что receipt_id/action_id и
  одноразовый approval claim обеспечивают identity/anti-replay binding;
- public-key history и rotation checkpoints экспортируются вместе с chain и
  защищаются тем же trusted verification path;
- verifier перед разбором отклоняет неизвестную или неподдерживаемую версию
  schema/manifest с exit code `4`; duplicate `transition_id` всегда является
  ошибкой истории, даже если подписи отдельных строк корректны;
- `continuity=genesis` разрешён только с `reason=initial` и `actor=system`,
  `continuity=broken` — только с `reason=recovery`, а
  `continuity=compromised` — только с `reason=compromise`; иные комбинации
  отклоняются как invalid transition;
- `key.history_export_failed` имеет приоритет над результатом математической
  проверки snapshot и даёт offline verifier exit code `2`, а не `0`;
- все mutation-записи receipt, action и chain head выполняются в одной
  `BEGIN IMMEDIATE`-транзакции; частичный commit этих таблиц запрещён;
- SQLite Core является источником истины, а JSONL — только атомарный экспортный
  снимок; расхождение не исправляется по JSONL и классифицируется как ошибка
  экспорта/целостности;
- timestamp skew применяется при runtime-записи относительно часов Core, а при
  offline-проверке экспортированного архива не применяется по умолчанию и может
  быть включён только явным режимом проверки;
- если receipt нельзя надёжно записать, mutation не запускается, а
  `pending_recovery` никогда не превращается в synthetic success.

## Сквозные неизменяемые инварианты

Эти правила имеют приоритет над удобством отдельного этапа и должны быть
закреплены общими schemas/vectors:

1. Только 01.1 определяет canonical signed bytes, `receipt_hash` и v1 limits.
2. Private key существует только внутри Core/key manager и никогда не выходит
   в renderer, IPC, export или обычные diagnostics.
3. SQLite — единственный mutable source of truth; JSONL не импортируется для
   repair и не исправляет SQLite.
4. Mutation не получает dispatch до durable pre receipt либо durable refusal.
5. После uncertain external outcome tool не вызывается автоматически повторно.
6. Один `action_id` имеет не более одного terminal receipt.
7. Rotation никогда не связывает receipt chains через `previous_receipt_hash`:
   первый receipt нового key имеет отсутствующий predecessor, а continuity
   между сегментами доказывается только KeyTransition/Checkpoint history.
8. Математически корректная signature не равна trust без explicit pinned root.
9. Verifier не repair-ит malformed, non-canonical, broken или incomplete input.
10. Любой terminal receipt требует matching pre receipt, который находится
    раньше него по durable sequence; `pending_recovery` не является terminal.
11. Terminal transition — единственный transition lineage, для которого в
    проверяемом наборе нет successor по `previous_key_id`; verifier обязан
    найти ровно один такой transition, сверить его `new_key_id` с
    экспортированным `active-key-v1.json` metadata (или с подписанным
    active-key checkpoint, если active metadata входит в export) и отклонить
    набор при отсутствии, дубликате или несовпадении terminal transition.
12. `stale_key` вычисляется по sequence boundary из checkpoint/transition
    metadata, а не по wall-clock и не по порядку строк JSONL; checkpoint
    sequence имеет приоритет, а при его отсутствии verifier возвращает
    `key.history_incomplete`.

Версии схем, лимиты и все stable error codes публикуются единым manifest
`contracts/receipts/v1/version-manifest.json`; реализации не поддерживают
раздельные локальные mapping-файлы.
