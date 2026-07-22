# Log Safety Design

## Goal

Закрыть roadmap 7.95: секреты не должны попадать в tracing, а повторяющиеся worker health warnings не должны зашумлять логи.

## Решение

Добавить server-local `log_safety` модуль, который переиспользует `evohime_memory::redact_secrets` и предоставляет единый `redact_for_log` helper. Динамические internal/provider/worker error details проходят через него перед `%error`/`%detail` logging; значения API keys сами по себе не логируются.

Sampling ограничивается worker health failures: одинаковая ошибка логируется сразу, затем не чаще заданного интервала `EVOHIME_LOG_SAMPLE_SECS` (по умолчанию 30 секунд). Счётчики health продолжают обновляться на каждом событии, а смена текста ошибки немедленно логируется.

## Границы

- Не менять публичный API ошибок и не скрывать ошибки от клиента сверх уже реализованного 7.93.
- Не фильтровать `error`/`warn` глобально и не терять уникальные сообщения.
- Не логировать значения model API keys, Authorization headers или полные request bodies.
- Существующая redaction regex-модель memory остаётся единым источником правил.

## Проверка

- Unit tests проверяют маскирование bearer/API-key/password и sampling одинаковых health errors.
- Server tests проверяют, что internal error logging использует redacted detail.
- Полный Rust workspace test/Clippy и frontend проверки остаются зелёными.
