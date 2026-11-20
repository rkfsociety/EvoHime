# План 198.1 — Core boundary, schema и storage

## Текущий прогресс

Добавлен Core-домен `eva_pulse_boundary` с версионированными signal/result
envelopes, bounded payload, TTL, identity/idempotency и проверкой того, что
`HiveTaskOrigin` не превращается в capability. Интеграция с durable storage и
замена прямого dispatch в `EvaPulseDaemon` остаются следующими подэтапами.

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
