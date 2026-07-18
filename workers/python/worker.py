"""Small HTTP job worker used by EvoHime's heavier processing tasks.

The worker deliberately uses only the Python standard library so it can run in
the local launcher and in a minimal container. Jobs are durable for the
process lifetime only; PostgreSQL-backed persistence belongs to the Rust
server, while this service owns execution and status reporting.
"""

from __future__ import annotations

import argparse
import difflib
import json
import logging
import os
import queue
import re
import threading
import uuid
from collections import Counter
from dataclasses import dataclass, field
from datetime import datetime, timezone
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

LOGGER = logging.getLogger("evohime.worker")
SUPPORTED_TASKS = (
    "echo",
    "text.stats",
    "text.keywords",
    "text.summarize",
    "text.chunk",
    "text.similarity",
    "text.entities",
    "text.diff",
    "text.classify",
    "text.language",
    "text.redact",
)
MAX_TEXT_LENGTH = 1_000_000
PROCESS_STARTED_AT = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
HEARTBEAT_INTERVAL_SECS = 1.0
DEFAULT_MAX_SENTENCES = 3
DEFAULT_CHUNK_SIZE = 500
DEFAULT_CHUNK_OVERLAP = 50
DEFAULT_DIFF_CONTEXT = 3
DEFAULT_MAX_DIFF_LINES = 500
REDACTION_REPLACEMENT = "[REDACTED]"


def health() -> dict[str, Any]:
    """Process liveness payload used by the Rust health watchdog."""

    return {
        "status": "ok",
        "worker": "python",
        "started_at": PROCESS_STARTED_AT,
        "pid": os.getpid(),
    }


def _require_text(task: str, payload: dict[str, Any]) -> str:
    text = payload.get("text")
    if not isinstance(text, str):
        raise ValueError(f"{task} requires a string payload.text")
    if len(text) > MAX_TEXT_LENGTH:
        raise ValueError(f"payload.text exceeds {MAX_TEXT_LENGTH} characters")
    return text


def _optional_int(
    payload: dict[str, Any],
    key: str,
    *,
    default: int,
    minimum: int,
    maximum: int | None = None,
) -> int:
    if key not in payload or payload[key] is None:
        return default
    value = payload[key]
    if isinstance(value, bool) or not isinstance(value, int):
        raise ValueError(f"payload.{key} must be an integer")
    if value < minimum or (maximum is not None and value > maximum):
        upper = f"..{maximum}" if maximum is not None else "+"
        raise ValueError(f"payload.{key} must be in {minimum}{upper}")
    return value


def validate_task_payload(task: str, payload: dict[str, Any]) -> None:
    """Reject malformed payloads before a job enters the queue."""

    if not isinstance(task, str) or not task:
        raise ValueError("task must be a non-empty string")
    if not isinstance(payload, dict):
        raise ValueError("payload must be an object")
    if task not in SUPPORTED_TASKS:
        raise ValueError(f"unsupported task: {task}")

    if task == "echo":
        return

    if task in {"text.stats", "text.keywords", "text.entities", "text.classify", "text.language", "text.redact"}:
        _require_text(task, payload)
        return

    if task == "text.summarize":
        _require_text(task, payload)
        _optional_int(
            payload,
            "max_sentences",
            default=DEFAULT_MAX_SENTENCES,
            minimum=1,
            maximum=20,
        )
        return

    if task == "text.chunk":
        _require_text(task, payload)
        chunk_size = _optional_int(
            payload,
            "chunk_size",
            default=DEFAULT_CHUNK_SIZE,
            minimum=64,
            maximum=8000,
        )
        overlap = _optional_int(
            payload,
            "overlap",
            default=DEFAULT_CHUNK_OVERLAP,
            minimum=0,
            maximum=None,
        )
        if overlap >= chunk_size:
            raise ValueError("payload.overlap must be less than payload.chunk_size")
        return

    if task == "text.similarity":
        _require_named_text(task, payload, "text_a")
        _require_named_text(task, payload, "text_b")
        return

    if task == "text.diff":
        _require_named_text(task, payload, "text_a")
        _require_named_text(task, payload, "text_b")
        _optional_int(
            payload,
            "context",
            default=DEFAULT_DIFF_CONTEXT,
            minimum=0,
            maximum=20,
        )
        _optional_int(
            payload,
            "max_diff_lines",
            default=DEFAULT_MAX_DIFF_LINES,
            minimum=1,
            maximum=2000,
        )
        return

    raise ValueError(f"unsupported task: {task}")


def _require_named_text(task: str, payload: dict[str, Any], key: str) -> str:
    text = payload.get(key)
    if not isinstance(text, str):
        raise ValueError(f"{task} requires a string payload.{key}")
    if len(text) > MAX_TEXT_LENGTH:
        raise ValueError(f"payload.{key} exceeds {MAX_TEXT_LENGTH} characters")
    return text


def _utc_now_iso() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def _split_sentences(text: str) -> list[str]:
    parts = re.split(r"(?<=[.!?])\s+", text.strip())
    return [part.strip() for part in parts if part.strip()]


def summarize_text(text: str, max_sentences: int) -> dict[str, Any]:
    sentences = _split_sentences(text)
    if not sentences:
        return {"summary": "", "sentences_used": 0, "source_sentences": []}

    words = re.findall(r"[\w]+", text.casefold(), flags=re.UNICODE)
    counts = Counter(word for word in words if len(word) > 2)

    scored: list[tuple[int, float, str]] = []
    for index, sentence in enumerate(sentences):
        tokens = re.findall(r"[\w]+", sentence.casefold(), flags=re.UNICODE)
        score = float(sum(counts.get(token, 0) for token in tokens if len(token) > 2))
        scored.append((index, score, sentence))

    selected = sorted(scored, key=lambda item: (-item[1], item[0]))[:max_sentences]
    selected.sort(key=lambda item: item[0])
    source = [sentence for _, _, sentence in selected]
    return {
        "summary": " ".join(source),
        "sentences_used": len(source),
        "source_sentences": source,
    }


def chunk_text(text: str, chunk_size: int, overlap: int) -> dict[str, Any]:
    if not text:
        return {"chunks": [], "count": 0}

    step = chunk_size - overlap
    chunks: list[dict[str, Any]] = []
    start = 0
    index = 0
    while start < len(text):
        end = min(start + chunk_size, len(text))
        chunks.append(
            {
                "index": index,
                "text": text[start:end],
                "start": start,
                "end": end,
            }
        )
        index += 1
        if end >= len(text):
            break
        start += step
    return {"chunks": chunks, "count": len(chunks)}


def _token_counts(text: str) -> Counter[str]:
    words = re.findall(r"[\w]+", text.casefold(), flags=re.UNICODE)
    return Counter(word for word in words if len(word) > 2)


def similarity_text(text_a: str, text_b: str) -> dict[str, Any]:
    """Bag-of-words cosine similarity (stdlib, no neural model)."""

    left = _token_counts(text_a)
    right = _token_counts(text_b)
    if not left or not right:
        return {
            "score": 0.0,
            "shared_tokens": 0,
            "tokens_a": sum(left.values()),
            "tokens_b": sum(right.values()),
        }

    shared = set(left) & set(right)
    dot = float(sum(left[token] * right[token] for token in shared))
    left_norm = float(sum(value * value for value in left.values()) ** 0.5)
    right_norm = float(sum(value * value for value in right.values()) ** 0.5)
    if left_norm <= 0.0 or right_norm <= 0.0:
        score = 0.0
    else:
        score = max(0.0, min(1.0, dot / (left_norm * right_norm)))
    return {
        "score": round(score, 6),
        "shared_tokens": len(shared),
        "tokens_a": sum(left.values()),
        "tokens_b": sum(right.values()),
    }


_URL_RE = re.compile(r"https?://[^\s<>\"']+", flags=re.IGNORECASE)
_EMAIL_RE = re.compile(r"\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b", flags=re.IGNORECASE)
_PATH_RE = re.compile(
    r"(?:[A-Za-z]:\\|~/|\./|\.\./|/)(?:[^\s<>\"']+)",
)
_TICKET_RE = re.compile(r"\b[A-Z][A-Z0-9]+-\d+\b")


def extract_entities(text: str) -> dict[str, Any]:
    """Heuristic entity extraction: urls, emails, paths, ticket ids."""

    urls = _unique_preserve(_URL_RE.findall(text))
    emails = _unique_preserve(_EMAIL_RE.findall(text))
    paths = [
        path
        for path in _unique_preserve(_PATH_RE.findall(text))
        if path not in urls and "://" not in path
    ]
    tickets = _unique_preserve(_TICKET_RE.findall(text))
    return {
        "urls": urls,
        "emails": emails,
        "paths": paths,
        "tickets": tickets,
        "counts": {
            "urls": len(urls),
            "emails": len(emails),
            "paths": len(paths),
            "tickets": len(tickets),
        },
    }


def diff_text(
    text_a: str,
    text_b: str,
    *,
    context: int = DEFAULT_DIFF_CONTEXT,
    max_diff_lines: int = DEFAULT_MAX_DIFF_LINES,
) -> dict[str, Any]:
    """Line-oriented unified diff via stdlib difflib."""

    lines_a = text_a.splitlines()
    lines_b = text_b.splitlines()
    matcher = difflib.SequenceMatcher(None, lines_a, lines_b, autojunk=False)
    ratio = matcher.ratio()

    lines_equal = 0
    lines_added = 0
    lines_removed = 0
    for tag, i1, i2, j1, j2 in matcher.get_opcodes():
        if tag == "equal":
            lines_equal += i2 - i1
        elif tag == "insert":
            lines_added += j2 - j1
        elif tag == "delete":
            lines_removed += i2 - i1
        elif tag == "replace":
            lines_removed += i2 - i1
            lines_added += j2 - j1

    unified = list(
        difflib.unified_diff(
            lines_a,
            lines_b,
            fromfile="text_a",
            tofile="text_b",
            lineterm="",
            n=context,
        )
    )
    truncated = len(unified) > max_diff_lines
    if truncated:
        unified = unified[:max_diff_lines]

    return {
        "ratio": round(ratio, 6),
        "lines_a": len(lines_a),
        "lines_b": len(lines_b),
        "lines_equal": lines_equal,
        "lines_added": lines_added,
        "lines_removed": lines_removed,
        "unified_diff": unified,
        "diff_truncated": truncated,
    }


def classify_text(text: str) -> dict[str, Any]:
    """Classify common agent text intents with deterministic lexical rules."""

    normalized = text.strip().casefold()
    if not normalized:
        category = "empty"
    elif re.search(r"\b(error|exception|traceback|bug|fail(?:ed|ure)?)\b|ошибк|падени|сломал", normalized):
        category = "bug_report"
    elif normalized.endswith(("?", "？")) or re.search(r"\b(how|what|why|как|что|почему|где)\b", normalized):
        category = "question"
    elif re.search(r"\b(fix|implement|add|remove|change|run|сделай|добавь|исправь|запусти)\b", normalized):
        category = "instruction"
    elif re.search(r"\b(done|completed|status|готово|заверш|статус)\b", normalized):
        category = "status_update"
    else:
        category = "general"
    return {"category": category, "confidence": 0.85 if category != "general" else 0.55}


def detect_language(text: str) -> dict[str, Any]:
    """Detect Russian/English/mixed text from Cyrillic and Latin letter counts."""

    cyrillic = len(re.findall(r"[А-Яа-яЁё]", text))
    latin = len(re.findall(r"[A-Za-z]", text))
    total = cyrillic + latin
    if total == 0:
        language = "unknown"
        confidence = 0.0
    elif cyrillic and latin and min(cyrillic, latin) / total >= 0.2:
        language = "mixed"
        confidence = round(min(cyrillic, latin) / total, 6)
    elif cyrillic:
        language = "ru"
        confidence = round(cyrillic / total, 6)
    else:
        language = "en"
        confidence = round(latin / total, 6)
    return {"language": language, "confidence": confidence, "characters": {"cyrillic": cyrillic, "latin": latin}}


_REDACTION_PATTERNS = (
    re.compile(r"(?i)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z0-9 ]*PRIVATE KEY-----"),
    re.compile(r"(?i)\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9_]{20,}\b"),
    re.compile(r"(?i)\b(?:sk|lr)[-_][A-Za-z0-9_-]{16,}\b"),
    re.compile(r"(?i)\b(?:xox[baprs]-)[A-Za-z0-9-]{10,}\b"),
    re.compile(r"(?i)\bAKIA[0-9A-Z]{16}\b"),
    re.compile(r"(?i)\bBearer\s+[A-Za-z0-9\-._~+/]+=*\b"),
    re.compile(r"(?i)\b(?:api[_-]?key|token|secret|password|passwd|cookie)\s*[:=]\s*\S{6,}"),
)


def redact_text(text: str) -> dict[str, Any]:
    """Apply the same secret-oriented redaction policy as structured memory."""

    redacted = text
    matches = 0
    for pattern in _REDACTION_PATTERNS:
        redacted, count = pattern.subn(REDACTION_REPLACEMENT, redacted)
        matches += count
    return {"text": redacted, "redacted": matches > 0, "matches": matches}


def _unique_preserve(values: list[str]) -> list[str]:
    seen: set[str] = set()
    out: list[str] = []
    for value in values:
        key = value.casefold()
        if key in seen:
            continue
        seen.add(key)
        out.append(value)
    return out


@dataclass
class Job:
    id: str
    task: str
    payload: dict[str, Any]
    status: str = "queued"
    result: Any = None
    error: str | None = None
    heartbeat_at: str | None = None
    _lock: threading.Lock = field(default_factory=threading.Lock, repr=False, compare=False)
    _stop_heartbeat: threading.Event = field(
        default_factory=threading.Event, repr=False, compare=False
    )

    def snapshot(self) -> dict[str, Any]:
        with self._lock:
            payload = {
                "id": self.id,
                "task": self.task,
                "status": self.status,
                "result": self.result,
                "error": self.error,
            }
            if self.heartbeat_at is not None:
                payload["heartbeat_at"] = self.heartbeat_at
            return payload


def run_task(task: str, payload: dict[str, Any]) -> Any:
    """Run one supported task and return JSON-serializable output."""

    validate_task_payload(task, payload)

    if task == "echo":
        return payload

    if task == "text.stats":
        text = payload["text"]
        return {
            "characters": len(text),
            "words": len(text.split()),
            "lines": len(text.splitlines()),
        }

    if task == "text.keywords":
        text = payload["text"]
        words = re.findall(r"[\w]+", text.casefold(), flags=re.UNICODE)
        counts = Counter(words)
        keywords = [
            {"word": word, "count": count}
            for word, count in sorted(counts.items(), key=lambda item: (-item[1], item[0]))
            if len(word) > 2
        ][:20]
        return {"keywords": keywords}

    if task == "text.summarize":
        max_sentences = _optional_int(
            payload,
            "max_sentences",
            default=DEFAULT_MAX_SENTENCES,
            minimum=1,
            maximum=20,
        )
        return summarize_text(payload["text"], max_sentences)

    if task == "text.chunk":
        chunk_size = _optional_int(
            payload,
            "chunk_size",
            default=DEFAULT_CHUNK_SIZE,
            minimum=64,
            maximum=8000,
        )
        overlap = _optional_int(
            payload,
            "overlap",
            default=DEFAULT_CHUNK_OVERLAP,
            minimum=0,
            maximum=None,
        )
        return chunk_text(payload["text"], chunk_size, overlap)

    if task == "text.similarity":
        return similarity_text(payload["text_a"], payload["text_b"])

    if task == "text.entities":
        return extract_entities(payload["text"])

    if task == "text.diff":
        context = _optional_int(
            payload,
            "context",
            default=DEFAULT_DIFF_CONTEXT,
            minimum=0,
            maximum=20,
        )
        max_diff_lines = _optional_int(
            payload,
            "max_diff_lines",
            default=DEFAULT_MAX_DIFF_LINES,
            minimum=1,
            maximum=2000,
        )
        return diff_text(
            payload["text_a"],
            payload["text_b"],
            context=context,
            max_diff_lines=max_diff_lines,
        )

    if task == "text.classify":
        return classify_text(payload["text"])

    if task == "text.language":
        return detect_language(payload["text"])

    if task == "text.redact":
        return redact_text(payload["text"])

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
        validate_task_payload(task, payload)
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
                job.heartbeat_at = _utc_now_iso()
            heartbeat = threading.Thread(
                target=self._heartbeat_loop, args=(job,), name=f"heartbeat-{job.id}", daemon=True
            )
            heartbeat.start()
            with self._active_lock:
                self._active_jobs += 1
            try:
                result = run_task(job.task, job.payload)
                with job._lock:
                    job.result = result
                    job.status = "completed"
                    job.heartbeat_at = _utc_now_iso()
            except Exception as exc:  # worker errors become inspectable job state
                with job._lock:
                    job.error = str(exc)
                    job.status = "failed"
                    job.heartbeat_at = _utc_now_iso()
                LOGGER.warning("job %s failed: %s", job.id, exc)
            finally:
                job._stop_heartbeat.set()
                heartbeat.join(timeout=2)
                with self._active_lock:
                    self._active_jobs -= 1
                self._queue.task_done()

    def _heartbeat_loop(self, job: Job) -> None:
        while not job._stop_heartbeat.wait(HEARTBEAT_INTERVAL_SECS):
            with job._lock:
                if job.status != "running":
                    return
                job.heartbeat_at = _utc_now_iso()


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
