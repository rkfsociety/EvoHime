# План: Подписанные hash-chain receipts

Статус: draft для ревью.

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

### 1. Canonical contract

- Зафиксировать canonical JSON encoding и versioned field rules.
- Ограничить payload/receipt size и запретить свободные raw strings.
- Добавить shared known-answer vectors для Rust, Electron verifier и будущего
  offline CLI.

### 2. Key lifecycle

- Генерировать key pair при первом запуске через supervisor.
- Защитить private key DPAPI/ACL, поддержать key id и rotation.
- Не менять ключ молча: rotation создаёт chain metadata и audit event.
- Public verification command не требует Core или сети.

### 3. Runtime integration

- Создавать pre-action receipt после policy/approval decision, но до mutation.
- Создавать post-action receipt с result hash и status после выполнения.
- Связывать action с human approval digest; изменение args между approval и
  execution должно блокироваться.
- Для read-only действий применять configurable sampling, для mutations —
  полный audit.

### 4. Chain storage и export

- Хранить bounded receipt metadata в SQLite и append-only JSONL export.
- Данные arguments/results хранить отдельно только по существующим privacy и
  retention правилам.
- Добавить verify-chain: signature, canonical bytes, previous hash и approval
  binding.
- Отдельно диагностировать broken chain, stale key, digest mismatch и missing
  receipt.

## IPC и UI

- Read-only `ListReceipts`, `VerifyReceipts`, `ExportReceipts` с limit и date
  bounds.
- UI показывает status `verified`, `broken`, `unverified`, key id и hashes;
  не показывает секретный payload автоматически.
- Approval preview должен показывать action digest, чтобы пользователь видел,
  что именно он подтверждает.

## Проверки

- Rust Ed25519 known-answer and cross-implementation vectors;
- tamper tests для каждого payload field;
- deletion/reordering test для hash chain;
- approval digest substitution/stale approval/expired approval;
- key rotation and offline verification after Core shutdown;
- bounded size and secret-redaction tests;
- recovery test: crash между pre и post receipt оставляет verifiable pending
  state, а не поддельный success.

## Критерии готовности

- любое mutation action имеет receipt или явный refusal с причиной;
- verifier работает без сети и без private key;
- chain break виден пользователю и в diagnostics;
- receipt не утверждает correctness/policy enforcement сверх фактически
  проверенных полей;
- текущий audit trail остаётся совместимым на переходный период.

## Зависимости

Блокирующие: Context Budget Manager (план 01) — payload содержит
`context_ledger_hash`, а ledger владеет им; существующие approval,
exact-call hash, diagnostics и Core-owned storage.

Опциональных интеграций нет. Что этот план обязан предоставить: receipts для
child workflows (план 06), которые связывают действия ребёнка с approval
родителя.
