# План 184.2 — Core capture, evaluation lifecycle и recovery

Статус: active implementation contract. Этап следует после overview 184.0 и предыдущих этапов этого направления.

## Scope и изменяемые контракты

Подключить existing journal/ledger events к Core normalizer, partition по run/attempt, freeze trace и запускать pure evaluator; report связать с trace+contract hashes.

## Зависимости

### Блокирующие

Блокирующие: этап 184.1, sequence, benchmark attempt identity, cancellation/recovery.

### Опциональные

Интеграции, обозначенные опциональными в overview, не блокируют базовый контракт этого этапа.

## Current checkout audit (2026-11-14)

До подключения normalizer проверены фактические источники событий:

- Обычный ToolAgent loop публикует `CoreEvent::ToolStarted`/`ToolOutput`
  и модельные diagnostic events, но не пишет typed `ExecutionEventV1` для
  каждого вызова инструмента. Эти payloads нельзя напрямую копировать в
  redacted trace.
- Receipt-tracked terminal path записывает typed `ToolCall`/`ToolReceipt`
  events, но использует `RunScope::Standalone` и `attempt_id=None`; этого
  недостаточно для benchmark attempt partition.
- `workflow_node_attempts` и `workflow_run_events` уже содержат attempt ids
  и монотонную per-run sequence. Однако поиск call sites
  `append_ledger_event_with_node_transition` на текущем checkout находит
  определение и storage tests, но не production integration.

Следующий безопасный шаг — добавить metadata-only capture на существующей
Core tool/effect boundary и связать его с typed execution ledger, сохранив
redaction до hash и существующую recovery/unknown-outcome семантику. Только
после появления authoritative run/attempt/action/approval/effect ids следует
строить normalizer и evaluator lifecycle. Не добавлять отдельный trace store,
не переиспользовать raw tool output и не синтезировать отсутствующие ids.

## Recovery и rollback

Frozen trace переоценивается идемпотентно; partial trace remains Interrupted; evaluator error typed failure; no unknown-effect retry.

Rollback/disable сохраняет действующие policy и ранее записанные данные; destructive data cleanup требует отдельного migration contract.

## Verification

Integration: observed/wrong sequence/args, approval denied then effect, retries, cancellation, report idempotency, child partition.

## Release evidence

Зафиксировать schema/contract versions, focused test/CI evidence, migration/compatibility result, bounded diagnostics и подтверждение recovery/security invariants. Не выдавать плановые проверки за выполненные.

## Критерии выхода

- [ ] Verdict вычисляется только из observed normalized trace и не изменяет policy.
- [ ] Внутренние ссылки разрешаются и git diff --check проходит.
- [ ] Canonical docs обновляются только после подтверждённого поведения.
