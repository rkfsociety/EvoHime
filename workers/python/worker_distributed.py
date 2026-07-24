"""Distributed HTTP job worker for horizontal scaling (Stage 7.54).

This worker polls the server for queued jobs via /api/worker/queue/claim,
executes them independently, and reports results back. Supports multiple
concurrent worker processes without coordination.

Uses only Python standard library for portability.
"""

from __future__ import annotations

import argparse
import json
import logging
import os
import threading
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone
from typing import Any

from worker import (
    HEARTBEAT_INTERVAL_SECS,
    LOGGER,
    PROCESS_STARTED_AT,
    SUPPORTED_TASKS,
    health,
    run_task,
    validate_task_payload,
)

CLAIM_POLL_INTERVAL = 0.5  # seconds between claim attempts
HEARTBEAT_INTERVAL = 1.0   # seconds between heartbeats
CLAIM_TIMEOUT = 30         # seconds to claim from server


def http_json_post(url: str, data: dict[str, Any], timeout: int = 5) -> Any:
    """POST JSON to URL and return parsed response."""
    body = json.dumps(data).encode('utf-8')
    req = urllib.request.Request(
        url,
        data=body,
        headers={'Content-Type': 'application/json'},
        method='POST'
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as response:
            if response.status in (204, 304):
                return None
            return json.loads(response.read().decode('utf-8'))
    except urllib.error.HTTPError as e:
        if e.code == 204:
            return None
        raise


def http_json_patch(url: str, data: dict[str, Any], timeout: int = 5) -> Any:
    """PATCH JSON to URL and return status code."""
    body = json.dumps(data).encode('utf-8')
    req = urllib.request.Request(
        url,
        data=body,
        headers={'Content-Type': 'application/json'},
        method='PATCH'
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as response:
            return response.status
    except urllib.error.HTTPError as e:
        return e.code


class DistributedWorkerClient:
    """Worker that polls server for jobs and reports results."""

    def __init__(self, server_url: str, worker_count: int = 1):
        self.server_url = server_url.rstrip('/')
        self.worker_count = worker_count
        self.active_jobs: dict[str, dict[str, Any]] = {}
        self.active_lock = threading.Lock()

    def claim_job(self) -> tuple[str, dict[str, Any], str] | None:
        """Claim next queued job from server. Returns (job_id, job_data, claim_token) or None."""
        url = f"{self.server_url}/api/worker/queue/claim"
        try:
            result = http_json_post(url, {})
            if result is None:
                return None
            job = result.get("job", {})
            claim_token = result.get("claim_token", "")
            job_id = str(job.get("id", ""))
            return (job_id, job, claim_token)
        except Exception as e:
            LOGGER.warning("claim_job failed: %s", e)
            return None

    def heartbeat(self, job_id: str, claim_token: str) -> bool:
        """Send heartbeat for running job. Returns True if claim is still valid."""
        url = f"{self.server_url}/api/worker/queue/{job_id}/heartbeat"
        try:
            status = http_json_patch(url, {"claim_token": claim_token})
            return status in (200, 204)
        except Exception as e:
            LOGGER.warning("heartbeat failed for %s: %s", job_id, e)
            return False

    def complete(self, job_id: str, claim_token: str, result: Any = None, error: str | None = None) -> bool:
        """Complete job. Returns True if successful."""
        url = f"{self.server_url}/api/worker/queue/{job_id}/complete"
        try:
            status = http_json_patch(
                url,
                {
                    "claim_token": claim_token,
                    "result": result,
                    "error": error,
                }
            )
            return status == 200
        except Exception as e:
            LOGGER.warning("complete failed for %s: %s", job_id, e)
            return False

    def health(self) -> dict[str, Any]:
        """Get worker health."""
        with self.active_lock:
            active_count = len(self.active_jobs)
        return {
            "status": "ok",
            "worker": "distributed-python",
            "started_at": PROCESS_STARTED_AT,
            "pid": os.getpid(),
            "active_jobs": active_count,
            "supported_tasks": list(SUPPORTED_TASKS),
        }

    def worker_loop(self):
        """Main worker loop: claim → execute → complete."""
        while True:
            claimed = self.claim_job()
            if claimed is None:
                time.sleep(CLAIM_POLL_INTERVAL)
                continue

            job_id, job_data, claim_token = claimed
            with self.active_lock:
                self.active_jobs[job_id] = {"status": "running"}

            heartbeat_thread = threading.Thread(
                target=self._heartbeat_loop,
                args=(job_id, claim_token),
                daemon=True,
                name=f"heartbeat-{job_id}"
            )
            heartbeat_thread.start()

            try:
                task = job_data.get("task", "")
                payload = job_data.get("payload_json", {})

                validate_task_payload(task, payload)
                result = run_task(task, payload)

                self.complete(job_id, claim_token, result=result)
                LOGGER.info("job %s completed successfully", job_id)
            except Exception as e:
                error_msg = str(e)
                self.complete(job_id, claim_token, error=error_msg)
                LOGGER.warning("job %s failed: %s", job_id, error_msg)
            finally:
                with self.active_lock:
                    self.active_jobs.pop(job_id, None)

    def _heartbeat_loop(self, job_id: str, claim_token: str):
        """Send periodic heartbeats while job is running."""
        while True:
            time.sleep(HEARTBEAT_INTERVAL)
            with self.active_lock:
                if job_id not in self.active_jobs:
                    return
            if not self.heartbeat(job_id, claim_token):
                LOGGER.warning("heartbeat failed for %s; claim may be lost", job_id)
                return


class HealthHandler:
    """HTTP handler for /health endpoint."""

    def __init__(self, worker: DistributedWorkerClient):
        self.worker = worker

    def __call__(self, environ: dict[str, Any], start_response) -> list[bytes]:
        """WSGI application for /health endpoint."""
        if environ.get("PATH_INFO") == "/health":
            health_data = self.worker.health()
            body = json.dumps(health_data).encode('utf-8')
            start_response('200 OK', [
                ('Content-Type', 'application/json'),
                ('Content-Length', str(len(body)))
            ])
            return [body]

        start_response('404 Not Found', [('Content-Type', 'text/plain')])
        return [b'not found']


def main():
    parser = argparse.ArgumentParser(description="EvoHime distributed Python worker")
    parser.add_argument("--server", default="http://127.0.0.1:3000")
    parser.add_argument("--workers", type=int, default=1)
    args = parser.parse_args()

    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
    LOGGER.info("distributed python worker v7.54 starting")
    LOGGER.info("server: %s, workers: %d", args.server, args.workers)

    client = DistributedWorkerClient(args.server, args.workers)

    # Start worker threads
    threads = [
        threading.Thread(
            target=client.worker_loop,
            name=f"worker-{i}",
            daemon=False
        )
        for i in range(args.workers)
    ]
    for thread in threads:
        thread.start()

    LOGGER.info("worker threads started; press Ctrl+C to exit")
    try:
        for thread in threads:
            thread.join()
    except KeyboardInterrupt:
        LOGGER.info("shutting down")


if __name__ == "__main__":
    main()
