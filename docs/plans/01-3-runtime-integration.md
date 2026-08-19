# Этап 01.3: Runtime integration

Этап плана [01 Подписанные hash-chain receipts](01-0-signed-hash-chain-receipts.md).

Реализовано в `crates/evohime-receipts/src/runtime.rs`. Этот документ фиксирует
уже существующий контракт, чтобы 01.4 и последующие этапы могли на него
опираться, не заглядывая в реализацию за нормативным описанием.

## Зависимости

Блокирующие: 01.1 (canonical payload/envelope, `receipt_hash`, лимиты) и 01.2
(key lifecycle, `ReceiptSigner`, offline-проверяемая public-key history).

## Что делает этот этап

`ReceiptRuntime` — единственная точка, через которую Core создаёт pre/terminal
receipt и привязывает approval к action. Он работает поверх Core-owned SQLite
(`events.db`, через `LocalDatabase`/`EventJournal`), устанавливает собственную
схему при каждом `ReceiptRuntime::new(...)` (`install_schema`, идемпотентно —
`CREATE TABLE IF NOT EXISTS` + point ALTER) и не зависит от отдельного файла
базы.

### Таблицы (owned by этот этап)

- `receipt_records` — подписанные receipt-строки: `receipt_id`, `action_id`,
  `receipt_kind` (`pre_action`/`post_action`/`refusal`), `action_status`,
  `key_id`, `canonical_payload`, `canonical_envelope`, `receipt_hash` UNIQUE,
  `previous_receipt_hash`, `created_at_ms`, `source` (`'signed'`, добавлено
  этапом 01.4).
- `receipt_actions` — состояние action: `state`
  (`awaiting_approval`/`prepared`/`refused`/`succeeded`/`failed`/`cancelled`/
  `pending_recovery`/`quarantined`), `dispatch_state`
  (`not_started`/`started`/`returned`), `approval_id`/`approval_call_hash`,
  `pre_receipt_hash`/`terminal_receipt_hash`, `recovery_code`,
  `tool_args_hash` (= `canonical_call_hash`), `parent_approval_ref`,
  `reconciliation_action_id`/`reconciles_action_id`.
- `receipt_approval_intents` — pending approval с monotonic-boot deadline
  (`clock_boot_id`, `created_monotonic_ms`, `deadline_monotonic_ms`) отдельно
  от wall-clock `expires_at_ms`, чтобы claim не зависел от перевода часов в
  рамках одного OS boot.
- `receipt_chain_heads` — `key_id -> (receipt_hash, updated_at_ms)`, читается и
  обновляется только внутри append-транзакции `signed_receipt`.
- `receipt_runtime_guard` — единственная строка (`id=1`), fail-closed фаза
  (`recovery_in_progress`/`ready`/`read_only_recovery`); mutation-пути идут
  через `require_ready`, который отклоняет любую запись вне `ready`.
- `receipt_protected_actions`, `receipt_runtime_config`,
  `receipt_audit_markers`, `receipt_sampling_changes`, `receipt_runtime_metrics`,
  `receipt_runtime_diagnostics`, `receipt_storage_rotation*` — вспомогательные
  таблицы для protected input-снапшотов, deterministic read-only sampling,
  bounded runtime-метрик и key-storage rotation batches.

## Основной API (`ReceiptRuntime<'a>`)

- `prepare` / `prepare_existing_approval` / `import_legacy_approval` /
  `migrate_legacy_approvals` — создают pre-receipt (`Allow`), refusal
  (`Deny`) или pending approval intent (`ApprovalRequired`); отклоняют
  неверный `action_id` (не UUIDv7), дубликат `action_id`, превышение
  `MAX_PENDING_ACTIONS` на task.
- `mark_started` / `mark_returned` — dispatch-state переходы вокруг реального
  вызова инструмента; сам вызов инструмента остаётся вне SQLite-транзакции.
- `claim_approval` / `claim_approval_checked` / `grant_approval` — approval
  claim по monotonic deadline, не по wall-clock.
- `complete` / `complete_reconciliation` — terminal receipt
  (`succeeded`/`failed`/`cancelled`), с bounded `output_digest` (64 lowercase
  hex) и без raw result.
- `refuse` — terminal refusal receipt с одним из `refusal_code` из 01.0/01.1.
- `mark_pending_recovery` / `link_reconciliation` / `unquarantine` — explicit
  recovery path; `pending_recovery` никогда не превращается в synthetic
  success (инвариант плана 01.0).
- `recover_on_startup` / `recover_database` — recovery state machine на
  старте: expire только intents, никогда не синтезирует success; покрыта
  матрицей всех 8 комбинаций pre/started/post в `runtime::tests`.
- `store_protected_action` / `load_protected_action*` /
  `rewrap_protected_batch` — bounded (≤512 bytes) authenticated envelope
  исходного input для recovery, шифруется storage-ключом; rotation переносит
  строки без потери при сбое посередине.
- `approval_gc` — периодическая очистка terminal approval intents (01.3
  ApprovalGC), безопасна к повторному вызову на любой guard-фазе.
- `counts` / `metrics` / `diagnostic_counts` / `storage_rotation_job` /
  `audit_sampling_config` — bounded read-only snapshots для
  `GetReceiptKeyStatus` и диагностики; не отдают payload или raw preview.

## Ошибки и коды

`RuntimeError::Code(&'static str)` — стабильные строковые коды
(`schema_violation`, `action_id_conflict`, `approval_stale`, `pending_limit`,
`chain_conflict`, `storage_key_unavailable`, и т.д.), зарегистрированы в
`transport_runtime_error_codes`/`canonical_refusal_aliases` версии-манифеста
01.1 (`contracts/receipts/v1/version-manifest.json`). Runtime никогда не
возвращает произвольный текст ошибки наружу.

## Гарантии, проверенные тестами (`runtime::tests`, 30+ тестов)

- fail-closed guard: recovery-in-progress блокирует каждый mutation entry
  point, включая chain write;
- один `action_id` — не более одного terminal receipt;
- approval claim использует monotonic deadline, а не wall-clock, в рамках
  одного OS boot;
- concurrent file-backed `prepare` держат единую проверяемую chain head;
- sustained writer contention укладывается в retry budget и завершается
  стабильным `chain_conflict`, не тихим успехом;
- secret-like preview всегда redact-ится перед truncation;
- unsigned runtime marker никогда не создаёт receipt или chain head;
- protected-row rotation переносит строки без потери при обрыве посередине.

## Что этот этап отдаёт наружу

`receipt_records`/`receipt_actions`/`receipt_chain_heads`/
`canonical_call_hash` — контракт, на котором построены 01.4 (chain
verification, listing, export) и 03.1 (child workflow receipts).

## Известные ограничения (зафиксировать явно, не как TODO без владельца)

- Этот документ — retroactive нормативная фиксация уже слитого кода, а не
  предшествующий ему план; при расхождении реализация в `runtime.rs` и её
  тесты остаются источником истины, документ обновляется вслед за ней.
