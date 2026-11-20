# План 198.1 — Core boundary, schema и storage

## Текущий прогресс

Добавлен Core-домен `eva_pulse_boundary` с версионированными signal/result
envelopes, bounded payload, TTL, identity/idempotency и проверкой того, что
`HiveTaskOrigin` не превращается в capability. Интеграция с durable storage и
замена прямого dispatch в `EvaPulseDaemon` остаются следующими подэтапами.
Перед созданием recovery/evolution-задачи `CoordinatorPulseTaskSink` уже
прогоняет bounded metadata envelope через эту валидацию. `HiveSignalReceiver`
теперь владеет переходом `accepted → Hive task` и является compatibility
adapter над существующим `TaskCoordinator`; durable idempotency/outbox и полное
разделение bootstrap остаются следующими подэтапами. До перезапуска Core
receiver удерживает bounded fence из 512 `idempotency_key` и возвращает уже
созданный `task_id` при повторной доставке; при ошибке dispatch reservation
освобождается. При подключённом `EventJournal` receiver восстанавливает
последние 512 acceptance-записей после перезапуска Core и сохраняет новый
результат как redacted `eva_pulse.signal.accepted`; prompt в journal не пишется.
Непринятые сигналы получают bounded `eva_pulse.signal.dead_letter` с
детерминированным reason code; исходный текст ошибки и payload не сохраняются.

## Контракт

- Ввести версионированные typed envelopes для сообщений EvaPulse и ответов
  Улья с bounded payload, TTL, source identity, correlation и idempotency.
- Зафиксировать `HiveTaskOrigin` (`hive_internal`, `eva_pulse`) и не смешивать
  origin с полномочиями: origin только provenance.
- Добавить durable записи для входящих сообщений, delivery outcome и
  dead-letter; secrets, prompts и raw model output в envelope не передаются.
- Отделить статус EvaPulse от `HiveStateSnapshot`; snapshot Улья содержит только
  Hive-owned состояние и безопасный внешний-signal summary.

## Работа

1. Описать Rust domain types и serde/proto representation.
2. Добавить миграцию EventJournal/SQLite с idempotency и bounded retention.
3. Удалить из публичного seam EvaPulse зависимости от `TaskCoordinator` и
   прямые `dispatch_recovery`/`dispatch_evolution`.
4. Сохранить legacy reader и диагностическое событие для старых записей.

## Verification

- schema round-trip/version rejection;
- duplicate, expired, oversized и unknown-kind messages;
- origin не расширяет capability и не обходится через IPC;
- migration/restart/replay tests.
