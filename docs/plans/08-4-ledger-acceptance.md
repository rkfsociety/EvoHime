# 08-4 — Acceptance, security и закрытие плана

## Цель

Подтвердить execution ledger end-to-end и перенести завершённый контракт в
канонические документы перед удалением плана как выполненного.

## Зависимости

### Блокирующие

- [08-3](08-3-ledger-ipc-and-projection.md) и через него 08-1 и 08-2:
  acceptance проверяет собранный контракт, а не отдельные слои;
- текущие release-процедуры: миграция с backup, packaging smoke и security
  review.

### Опциональные

- общий deterministic evaluation harness (`crates/evohime-core/src/evals.rs`,
  `tests/evals/`): при наличии fixtures регистрируются в нём, иначе остаются
  обычными интеграционными тестами соответствующих crate-ов.

## Deterministic acceptance fixtures

- action → tool call → observation → successful typed receipt linked to signed
  `receipts_v1`;
- legacy event mapping с воспроизводимым `event_id` и сохранённым
  `sequence_id`;
- approval approve/reject/expiry;
- timeout, cancellation, provider failure и unknown result;
- pre-dispatch restart (`interrupted`/resumable) и post-dispatch restart
  (`unknown_outcome`/blocked без blind retry);
- reconnect во время каждой промежуточной фазы;
- duplicate IPC delivery и duplicate terminal attempt;
- SQLite failure с полным rollback;
- supervisor restart с recovery decision, `dead_letter` только по bounded rule
  и `unknown_outcome` без blind retry;
- replay gap, stale `core_instance_id`/`session_epoch` и bounded typed snapshot;
- workflow `run_sequence` ↔ global ledger event linkage;
- отказ ledger при ссылке на `run_id` с несовместимым `run_scope`;
- переход через `cancelling` после миграции CHECK `workflow_run_nodes.state`;
- secret/PII/raw output redaction.

## Release checks

- `cargo test -p evohime-core -p evohime-local-storage -p evohime-desktop-ipc`
  и targeted `evohime-receipts`/`evohime-model-provenance` integration tests;
- `cargo fmt --check` и `git diff --check`;
- из `desktop/evohime-electron`: `npm run check:protocol`,
  `npm run typecheck`, `npm test`;
- migration v26→v30 и v29→v30 (включая пересоздание `workflow_run_nodes`),
  legacy fixtures, backup, rollback и package smoke;
- security review для redaction, approval binding, frame/payload limits и
  renderer isolation;
- проверка всех внутренних Markdown-ссылок и отсутствие ссылок на удаляемые
  plan files после closure.

## Закрытие

После прохождения критериев контракт переносится в `docs/architecture.md`,
подтверждённое состояние — в `docs/current-state.md`, а исполняемые файлы
плана 08 удаляются по правилу репозитория. Перед удалением обновляются
`docs/plans/README.md`, `docs/development-plan.md` и все ссылки/таблицы,
которые называют 08 незавершённым. Если хотя бы один критерий не закрыт,
план остаётся в каталоге с секцией о фактически реализованном и оставшемся
поведении; частичное наличие кода не считается closure.
