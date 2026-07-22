# Task Timeline and Latency Design

## Goal

Закрыть roadmap 7.94: сделать correlation id задачи видимым и копируемым в Tasks/Actions, а latency — отображаемой полосой без вычисления бизнес-метрик во frontend.

## Решение

Сервер остаётся источником telemetry. В `task.completed` и `task.failed` добавляется необязательное `duration_ms`, рассчитанное из существующего task observability. `action.logged` получает необязательные `correlation_id` и `duration_ms`, чтобы новые события могли показывать связь и длительность, а старые/служебные события сохраняли backward-compatible форму.

Frontend хранит эти значения как view-model, отображает task duration и action duration относительными полосами, а correlation id копирует через Clipboard API с локальным статусом успеха/ошибки. При отсутствии optional latency показывается нейтральное состояние без ложной длительности.

## Границы

- Изменяется общий protocol schema и Rust enum; TypeScript генерируется штатным скриптом.
- Backend не раскрывает внутренние детали ошибок и не переносит расчёты latency в браузер.
- UI показывает только данные server events; никаких новых HTTP-запросов и fake telemetry.
- Старые события без optional полей должны десериализоваться.

## Проверка

- Rust protocol test подтверждает сериализацию optional correlation/duration полей.
- Server test подтверждает, что завершение task публикует duration из observability.
- Frontend typecheck/build и тесты helper-представления подтверждают форматирование latency и copy-state.
