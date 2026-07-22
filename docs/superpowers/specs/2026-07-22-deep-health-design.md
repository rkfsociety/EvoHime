# Deep Health Design

## Goal

Закрыть roadmap 7.96: дать отдельный диагностический health endpoint для PostgreSQL, Python worker и workspace disk, не ломая быстрый launcher liveness `/health`.

## Решение

Добавить `GET /health/deep`. Три проверки выполняются параллельно и ограничены коротким timeout. PostgreSQL проверяется `SELECT 1`, worker — существующим `/health`, disk — доступностью и directory metadata `WORKSPACE_ROOT`. Ответ содержит общий `ok|degraded|failed`, component statuses и latency; внутренние ошибки заменяются безопасными кодами.

`/health` остаётся без dependency checks. `/health/deep` публичен для локального launcher/monitoring, как и `/health`.

## Статусы

- `ok`: database, worker и disk доступны.
- `degraded`: база и disk доступны, но worker недоступен или вернул не-`ok`.
- `failed`: база или disk недоступны/истёк timeout; HTTP status `503`.

## Проверка

- Unit tests проверяют агрегацию component statuses и безопасный payload.
- Server tests проверяют публичность deep endpoint и сохранение liveness endpoint.
- Полный Rust workspace test/Clippy и frontend typecheck/build остаются зелёными.
