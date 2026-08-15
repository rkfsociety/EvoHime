# План 02: Подписанные hash-chain receipts

Обзор плана. Этапы вынесены в отдельные файлы и ревьюятся по одному.

## Цель

Сделать действия Евы проверяемыми офлайн: для каждого значимого tool call
создавать bounded receipt, подписанный Core-ключом и связанный с предыдущим
receipt. Receipt доказывает авторство ключа, целостность и порядок, но не
доказывает правильность действия.

## Модель доверия

- Private signing key создаёт и хранит supervisor/Core через Windows-protected
  storage; в source, renderer и обычные logs ключ не попадает.
- Public key доступен для локальной проверки и экспорта пользователем.
- Receipt подписывает canonical payload; raw arguments/result заменяются hash.
- Approval receipt и action receipt должны ссылаться на один action digest.

## Payload v1

Минимальные поля:

- `receipt_version`, `receipt_id`, `timestamp`, `task_id`, `run_id`;
- `tool_name`, `tool_args_hash`, `result_hash`, `policy_id`, `policy_decision`;
- `approval_id`/`parent_approval_ref` при необходимости;
- `previous_receipt_hash`, `context_ledger_hash`, `model_route`;
- `signature.algorithm`, `key_id`, `signature`.

## Этапы

| Этап | Файл | Что отдаёт наружу | Кто потребляет |
| --- | --- | --- | --- |
| 02.1 | [Canonical contract](02-1-canonical-contract.md) | canonical encoding payload и known-answer vectors | 02.2–02.4 |
| 02.2 | [Key lifecycle](02-2-key-lifecycle.md) | key pair, rotation и offline verification | 02.3, 02.4 |
| 02.3 | [Runtime integration](02-3-runtime-integration.md) | pre/post-action receipts и approval binding | 04.1 |
| 02.4 | [Chain storage и export](02-4-chain-storage-and-export.md) | verify-chain, IPC и UI | UI |

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
Core-owned storage. Остальные этапы плана 01 этому плану не нужны.

Опциональных интеграций нет. Что этот план обязан предоставить: этап 02.3
даёт child workflows (этап 04.1) связь действий ребёнка с approval родителя,
а 02.4 — verify-chain для их аудита.

## Критерии готовности плана

- любое mutation action имеет receipt или явный refusal с причиной;
- verifier работает без сети и без private key;
- chain break виден пользователю и в diagnostics;
- receipt не утверждает correctness/policy enforcement сверх фактически
  проверенных полей;
- текущий audit trail остаётся совместимым на переходный период.
