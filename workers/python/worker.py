"""Small HTTP job worker used by EvoHime's heavier processing tasks.

The worker deliberately uses only the Python standard library so it can run in
the local launcher and in a minimal container. Jobs are durable for the
process lifetime only; PostgreSQL-backed persistence belongs to the Rust
server, while this service owns execution and status reporting.
"""

from __future__ import annotations

import argparse
import json
import logging
import queue
import re
import threading
import uuid
from collections import Counter
from dataclasses import dataclass, field
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

LOGGER = logging.getLogger("evohime.worker")
SUPPORTED_TASKS = ("echo", "text.stats", "text.keywords")
MAX_TEXT_LENGTH = 1_000_000


def health() -> dict[str, str]:
    """Keep the original health helper for lightweight process checks."""

    return {"status": "ok", "worker": "python"}


@dataclass
class Job:
    id: str
    task: str
    payload: dict[str, Any]
    status: str = "queued"
    result: Any = None
    error: str | None = None
    _lock: threading.Lock = field(default_factory=threading.Lock, repr=False, compare=False)

    def snapshot(self) -> dict[str, Any]:
        with self._lock:
            return {
                "id": self.id,
                "task": self.task,
                "status": self.status,
                "result": self.result,
                "error": self.error,
            }


def run_task(task: str, payload: dict[str, Any]) -> Any:
    """Run one supported task and return JSON-serializable output."""

    if task == "echo":
        return payload

    if task == "text.stats":
        text = payload.get("text")
        if not isinstance(text, str):
            raise ValueError("text.stats requires a string payload.text")
        if len(text) > MAX_TEXT_LENGTH:
            raise ValueError(f"payload.text exceeds {MAX_TEXT_LENGTH} characters")
        return {
            "characters": len(text),
            "words": len(text.split()),
            "lines": len(text.splitlines()),
        }

    if task == "text.keywords":
        text = payload.get("text")
        if not isinstance(text, str):
            raise ValueError("text.keywords requires a string payload.text")
        if len(text) > MAX_TEXT_LENGTH:
            raise ValueError(f"payload.text exceeds {MAX_TEXT_LENGTH} characters")
        words = re.findall(r"[\w]+", text.casefold(), flags=re.UNICODE)
        counts = Counter(words)
        keywords = [
            {"word": word, "count": count}
            for word, count in sorted(counts.items(), key=lambda item: (-item[1], item[0]))
            if len(word) > 2
        ][:20]
        return {"keywords": keywords}

    raise ValueError(f"unsupported task: {task}")


class JobService:
    """Bounded in-memory queue with concurrent status-safe job snapshots."""

    def __init__(self, worker_count: int = 1, max_queue_size: int = 100) -> None:
        if worker_count < 1 or max_queue_size < 1:
            raise ValueError("worker_count and max_queue_size must be positive")
        self._jobs: dict[str, Job] = {}
        self._jobs_lock = threading.Lock()
        self._queue: queue.Queue[Job | None] = queue.Queue(maxsize=max_queue_size)
        self._active_jobs = 0
        self._active_lock = threading.Lock()
        self._workers = [
            threading.Thread(target=self._worker_loop, name=f"worker-{i}", daemon=True)
            for i in range(worker_count)
        ]
        for worker in self._workers:
            worker.start()

    def submit(self, task: str, payload: dict[str, Any]) -> Job:
        if not isinstance(task, str) or not task:
            raise ValueError("task must be a non-empty string")
        if not isinstance(payload, dict):
            raise ValueError("payload must be an object")
        job = Job(id=str(uuid.uuid4()), task=task, payload=payload)
        with self._jobs_lock:
            self._jobs[job.id] = job
        try:
            self._queue.put_nowait(job)
        except queue.Full:
            with self._jobs_lock:
                self._jobs.pop(job.id, None)
            raise RuntimeError("worker queue is full") from None
        return job

    def get(self, job_id: str) -> Job | None:
        with self._jobs_lock:
            return self._jobs.get(job_id)

    def metrics(self) -> dict[str, int]:
        with self._active_lock:
            active_jobs = self._active_jobs
        return {"queue_depth": self._queue.qsize(), "active_jobs": active_jobs}

    def close(self) -> None:
        for _ in self._workers:
            self._queue.put(None)
        for worker in self._workers:
            worker.join(timeout=5)

    def _worker_loop(self) -> None:
        while True:
            job = self._queue.get()
            if job is None:
                self._queue.task_done()
                break
            with job._lock:
                job.status = "running"
            with self._active_lock:
                self._active_jobs += 1
            try:
                result = run_task(job.task, job.payload)
                with job._lock:
                    job.result = result
                    job.status = "completed"
            except Exception as exc:  # worker errors become inspectable job state
                with job._lock:
                    job.error = str(exc)
                    job.status = "failed"
                LOGGER.warning("job %s failed: %s", job.id, exc)
            finally:
                with self._active_lock:
                    self._active_jobs -= 1
                self._queue.task_done()


class WorkerHandler(BaseHTTPRequestHandler):
    service: JobService

    def do_GET(self) -> None:  # noqa: N802 - stdlib HTTP handler API
        if self.path == "/health":
            self._send_json(HTTPStatus.OK, {**health(), **self.service.metrics(), "supported_tasks": SUPPORTED_TASKS})
            return
        if self.path.startswith("/v1/jobs/"):
            job = self.service.get(self.path.rsplit("/", 1)[-1])
            if job is None:
                self._send_json(HTTPStatus.NOT_FOUND, {"error": "job not found"})
            else:
                self._send_json(HTTPStatus.OK, job.snapshot())
            return
        self._send_json(HTTPStatus.NOT_FOUND, {"error": "not found"})

    def do_POST(self) -> None:  # noqa: N802 - stdlib HTTP handler API
        if self.path != "/v1/jobs":
            self._send_json(HTTPStatus.NOT_FOUND, {"error": "not found"})
            return
        try:
            body = self._read_json()
            job = self.service.submit(body.get("task"), body.get("payload", {}))
        except RuntimeError as exc:
            self._send_json(HTTPStatus.SERVICE_UNAVAILABLE, {"error": str(exc)})
        except (ValueError, TypeError, json.JSONDecodeError) as exc:
            self._send_json(HTTPStatus.BAD_REQUEST, {"error": str(exc)})
        else:
            self._send_json(HTTPStatus.ACCEPTED, job.snapshot())

    def log_message(self, format: str, *args: Any) -> None:
        LOGGER.info("%s - %s", self.address_string(), format % args)

    def _read_json(self) -> dict[str, Any]:
        length = int(self.headers.get("Content-Length", "0"))
        if length > 2_000_000:
            raise ValueError("request body is too large")
        body = json.loads(self.rfile.read(length))
        if not isinstance(body, dict):
            raise ValueError("request body must be an object")
        return body

    def _send_json(self, status: HTTPStatus, payload: dict[str, Any]) -> None:
        encoded = json.dumps(payload, separators=(",", ":")).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)


def create_server(host: str = "127.0.0.1", port: int = 8090, worker_count: int = 1) -> ThreadingHTTPServer:
    service = JobService(worker_count=worker_count)

    class Handler(WorkerHandler):
        pass

    Handler.service = service
    server = ThreadingHTTPServer((host, port), Handler)
    server.service = service  # type: ignore[attr-defined]
    return server


def main() -> None:
    parser = argparse.ArgumentParser(description="EvoHime Python job worker")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8090)
    parser.add_argument("--workers", type=int, default=1)
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
    server = create_server(args.host, args.port, args.workers)
    LOGGER.info("python worker listening on %s:%s", args.host, args.port)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        LOGGER.info("shutting down")
    finally:
        server.shutdown()
        server.service.close()  # type: ignore[attr-defined]
        server.server_close()


if __name__ == "__main__":
    main()
