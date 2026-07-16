# Python worker — Rust bridge

## Цель

Подключить изолированный Python worker к Rust server через устойчивый HTTP-контракт, чтобы сервер владел жизненным циклом jobs, попытками и результатами, а worker — только выполнением.

## Дизайн

- `crates/server/src/worker.rs` содержит HTTP-клиент submit/poll с ограниченным таймаутом и структурированными ошибками.
- `migrations/0010_worker_jobs.sql` хранит server job id, worker job id, task, payload, status, attempts, result/error и timestamps.
- Server API `POST /api/worker/jobs`, `GET /api/worker/jobs/:id`, `POST /api/worker/jobs/:id/retry` создаёт, читает и повторяет jobs. Retry увеличивает attempts и создаёт новый worker job.
- `workers/python/worker.py` получает первый ML-oriented handler `text.keywords`, сохраняющий стандартный JSON lifecycle.
- При недоступном worker сервер возвращает понятный `503`, не падает и не теряет durable запись.

## Проверка

Python unit/integration tests проверяют handler и HTTP lifecycle. Rust tests проверяют сериализацию запроса/ответа и storage SQL shape; full `cargo test` и Python unittest выполняются перед коммитом.
