# План 01: Подписанные hash-chain receipts

Обзор плана. Этапы вынесены в отдельные файлы и ревьюятся по одному.

## Цель

Сделать действия Евы проверяемыми офлайн: для каждого mutation tool call и
каждого action с внешним side effect создавать bounded receipt, подписанный
Core-ключом и связанный с предыдущим receipt. Read-only actions без side effect
могут проходить детерминированный sampling по правилам 01.3; failures и refusals
никогда не sampling-ятся. Критерий значимости вычисляет Core по registry/policy,
а не renderer. Receipt доказывает авторство ключа, целостность и порядок, но не
доказывает правильность действия.

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
payload или raw JSON. Genesis единственный для `(key_id, chain)`; новый ключ
начинает новый chain только после проверки trusted rotation/checkpoint history.

## Этапы

| Этап | Файл | Что отдаёт наружу | Кто потребляет |
| --- | --- | --- | --- |
| 01.1 | [Canonical contract](01-1-canonical-contract.md) | canonical encoding payload и known-answer vectors | 01.2–01.4 |
| 01.2 | [Key lifecycle](01-2-key-lifecycle.md) | key pair, rotation и offline verification | 01.3, 01.4 |
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
ровно 64 ASCII bytes, вычисляется Context Budget Manager и валидируется
verifier независимо от upstream.

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
а verify 1000 receipts выполняется асинхронно UI и укладывается в 100 ms на
локальном SSD после прогрева кэша; превышение измеряется отдельной метрикой.

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
- если receipt нельзя надёжно записать, mutation не запускается, а
  `pending_recovery` никогда не превращается в synthetic success.
