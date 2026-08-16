# План 01: Подписанные hash-chain receipts

Обзор плана. Этапы вынесены в отдельные файлы и ревьюятся по одному.

## Цель

Сделать действия Евы проверяемыми офлайн: для каждого значимого tool call
создавать bounded receipt, подписанный Core-ключом и связанный с предыдущим
receipt. Receipt доказывает авторство ключа, целостность и порядок, но не
доказывает правильность действия.

## Модель доверия

- Private signing key создаёт и хранит supervisor/Core через Windows-protected
  storage; в source, renderer и обычные logs ключ не попадает.
- Public-key history доступна для локальной проверки и экспорта пользователем;
  доверие начинается с явно подтверждённого genesis fingerprint, а не с
  молчаливого TOFU.
- После rotation старый private key уничтожается: исторические receipts
  проверяются по сохранённому public key.
- Receipt подписывает canonical payload; raw arguments/result заменяются hash.
- Approval receipt и action receipt должны ссылаться на один action digest.

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
лимиты и правила версионирования задаёт этап 01.1.

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

## Зависимости плана

Блокирующие: нет. Context Budget Manager реализован: payload содержит `context_ledger_hash`, а ledger
определён именно там; существующие approval, exact-call hash, diagnostics и
Core-owned storage.

Опциональных интеграций нет. Что этот план обязан предоставить: этап 01.3
даёт child workflows (этап 03.1) связь действий ребёнка с approval родителя,
а 01.4 — verify-chain для их аудита.

## Критерии готовности плана

- любое mutation action имеет receipt или явный refusal с причиной;
- verifier работает без сети и без private key;
- chain break виден пользователю и в diagnostics;
- receipt не утверждает correctness/policy enforcement сверх фактически
  проверенных полей;
- текущий audit trail остаётся совместимым на переходный период.
