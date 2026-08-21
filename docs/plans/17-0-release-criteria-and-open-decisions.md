# План 17. Общие release gates и нерешённые решения

## Цель

Свести обязательные release gates и архитектурные решения для планов 06–16 в
проверяемый cross-plan контракт. План не добавляет второй runtime или новую
feature surface: он задаёт доказательства готовности и правила, по которым
отдельные планы допускаются к поставке.

## Что уже есть в checkout

Архитектура уже фиксирует Core/SQLite как durable source of truth,
authenticated local IPC, supervisor recovery, typed errors, bounded context,
policy/approval и redaction для реализованных частей. План 17 проверяет, что
новые планы не обходят эти границы и что открытые решения записаны до их
реализации.

## Границы

Входит dependency/decision register, versioned contract and migration review,
resource/concurrency/retention limits, deterministic fixtures/replay,
security/privacy/egress/licensing/maintenance evidence, rollback/recovery и
финальная audit-приёмка.

Не входит сторонний Python/Node agent SDK, cloud control plane, обязательный
внешний telemetry backend, public HTTP вместо authenticated IPC, unrestricted
desktop/browser control, automatic transcript memory, speaker identity,
model-generated authority, production effects из simulation и unbounded
multi-agent autonomy.

## Зависимости

**Блокирующие:** планы 06–16 и их актуальные `current-state`/`architecture`
контракты; каждый реализуемый этап обязан предоставить свои fixtures,
migrations, rollback и release evidence.

**Опциональные:** конкретный browser/voice/vision backend может отсутствовать;
тогда gate должен подтвердить typed unsupported/fallback и отсутствие его
зависимостей в базовом package.

## Этапы

1. Вести register решений, зависимостей и статуса контрактов.
2. Проверить общие runtime/security/release gates и запрещённые расширения.
3. Свести rollback, recovery, observability, license и privacy evidence.
4. Выполнить финальный audit и удалить только закрытые временные материалы.

## Готово, когда

Для каждого плана есть проверяемый scope, dependency graph, contract/migration
evidence, deterministic fixtures, security/privacy/egress review, rollback и
typed failure behavior. Ни один release gate не зависит от неподтверждённого
решения или внешнего runtime.

