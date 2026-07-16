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

- `GET /health` — status, queue metrics, and supported tasks.
- `POST /v1/jobs` — submit `{ "task": "text.stats", "payload": { "text": "..." } }`.
- `GET /v1/jobs/{id}` — poll a queued, running, completed, or failed job.

The initial task set contains `echo`, `text.stats`, and the ML-oriented
`text.keywords` handler; new handlers can be
added behind the same structured job lifecycle without changing the HTTP
contract. Job state is process-local, so the Rust server remains responsible
for durable task and checkpoint storage.
