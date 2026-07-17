# Python workers

Isolated HTTP workers for heavier AI/ML tasks. The worker uses only the Python
standard library and is intentionally independent from the Rust server.

## Layout

```text
workers/python/
├── README.md
├── worker.py
└── test_worker.py
```

## Run

```powershell
python workers/python/worker.py --host 127.0.0.1 --port 8090
python -m unittest discover -s workers/python -p "test_*.py"
```

## API

- `GET /health` — status, `started_at`, `pid`, queue metrics, and supported tasks.
- `POST /v1/jobs` — submit `{ "task": "...", "payload": { ... } }`.
  Invalid task/payload shapes are rejected with `400` before queueing.
- `GET /v1/jobs/{id}` — poll a queued, running, completed, or failed job.
  Running/completed snapshots include `heartbeat_at` for Rust stall detection.

## Supported tasks

| Task | Payload | Result |
| --- | --- | --- |
| `echo` | any object | same payload |
| `text.stats` | `{ text }` | characters / words / lines |
| `text.keywords` | `{ text }` | top keyword frequencies |
| `text.summarize` | `{ text, max_sentences? }` | extractive summary (`1..20` sentences, default 3) |
| `text.chunk` | `{ text, chunk_size?, overlap? }` | overlapping character chunks |
| `text.similarity` | `{ text_a, text_b }` | bag-of-words cosine score + token counts |
| `text.entities` | `{ text }` | urls / emails / paths / ticket ids |

Job state is process-local. The Rust server owns durable `worker_jobs` rows,
process health watchdog, per-job stall retries, backoff, retention, and
observability (`GET /api/worker/status`, `GET /api/worker/jobs`, nested
`worker` block in `GET /api/metrics`).

## Server env knobs

| Variable | Default | Meaning |
| --- | --- | --- |
| `PYTHON_WORKER_URL` | `http://127.0.0.1:8090` | Worker base URL |
| `WORKER_HEALTH_INTERVAL_SECS` | `5` | Health poll interval |
| `WORKER_HEALTH_STALE_SECS` | `15` | Unhealthy window before recover |
| `WORKER_JOB_STALL_SECS` | `30` | Stale `heartbeat_at` → retry |
| `WORKER_JOB_RETENTION_DAYS` | `7` | Prune completed/failed rows |
