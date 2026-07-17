import json
import threading
import unittest
from http.client import HTTPConnection

from worker import (
    JobService,
    chunk_text,
    create_server,
    diff_text,
    extract_entities,
    similarity_text,
    summarize_text,
    validate_task_payload,
)


class JobServiceTests(unittest.TestCase):
    def test_text_stats_completes(self):
        service = JobService()
        try:
            job = service.submit("text.stats", {"text": "one two\nthree"})
            for _ in range(100):
                if (current := service.get(job.id)).status in {"completed", "failed"}:
                    break
                threading.Event().wait(0.01)
            self.assertEqual(current.status, "completed")
            self.assertEqual(current.result, {"characters": 13, "words": 3, "lines": 2})
        finally:
            service.close()

    def test_unknown_task_is_rejected_on_submit(self):
        service = JobService()
        try:
            with self.assertRaisesRegex(ValueError, "unsupported task"):
                service.submit("missing", {})
        finally:
            service.close()

    def test_invalid_text_stats_payload_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "payload.text"):
            validate_task_payload("text.stats", {"text": 123})

    def test_summarize_keeps_top_sentences_in_source_order(self):
        text = (
            "Cats sleep often. "
            "Dogs chase cats and cats run. "
            "Birds fly south. "
            "Cats chase mice while cats nap."
        )
        result = summarize_text(text, max_sentences=2)
        self.assertEqual(result["sentences_used"], 2)
        self.assertEqual(
            result["source_sentences"],
            [
                "Dogs chase cats and cats run.",
                "Cats chase mice while cats nap.",
            ],
        )
        self.assertEqual(
            result["summary"],
            "Dogs chase cats and cats run. Cats chase mice while cats nap.",
        )

    def test_summarize_empty_text(self):
        self.assertEqual(
            summarize_text("", 3),
            {"summary": "", "sentences_used": 0, "source_sentences": []},
        )

    def test_chunk_respects_size_and_overlap(self):
        result = chunk_text("abcdefghij", chunk_size=4, overlap=1)
        self.assertEqual(result["count"], 3)
        self.assertEqual(
            result["chunks"],
            [
                {"index": 0, "text": "abcd", "start": 0, "end": 4},
                {"index": 1, "text": "defg", "start": 3, "end": 7},
                {"index": 2, "text": "ghij", "start": 6, "end": 10},
            ],
        )

    def test_chunk_rejects_overlap_gte_size(self):
        with self.assertRaisesRegex(ValueError, "overlap"):
            validate_task_payload("text.chunk", {"text": "hi", "chunk_size": 64, "overlap": 64})

    def test_summarize_rejects_bad_max_sentences(self):
        with self.assertRaisesRegex(ValueError, "max_sentences"):
            validate_task_payload("text.summarize", {"text": "hi", "max_sentences": 0})

    def test_similarity_scores_related_texts_higher(self):
        close = similarity_text(
            "prefer git worktrees for parallel agents",
            "use worktrees when launching parallel agents",
        )
        far = similarity_text(
            "prefer git worktrees for parallel agents",
            "postgres connection pool size sixteen",
        )
        self.assertGreater(close["score"], far["score"])
        self.assertGreater(close["shared_tokens"], 0)
        self.assertEqual(
            similarity_text("", "anything")["score"],
            0.0,
        )

    def test_similarity_rejects_missing_fields(self):
        with self.assertRaisesRegex(ValueError, "text_a"):
            validate_task_payload("text.similarity", {"text_b": "only b"})

    def test_entities_extracts_urls_emails_paths_tickets(self):
        result = extract_entities(
            "Ping roman@example.com about EVOHIME-42 at https://example.com/docs "
            "and check ./crates/server/src/main.rs again https://example.com/docs"
        )
        self.assertEqual(result["emails"], ["roman@example.com"])
        self.assertEqual(result["tickets"], ["EVOHIME-42"])
        self.assertEqual(result["urls"], ["https://example.com/docs"])
        self.assertTrue(any(path.endswith("main.rs") for path in result["paths"]))
        self.assertEqual(result["counts"]["urls"], 1)

    def test_entities_job_completes(self):
        service = JobService()
        try:
            job = service.submit(
                "text.entities",
                {"text": "see https://evohime.dev and ticket ABC-9"},
            )
            for _ in range(100):
                if (current := service.get(job.id)).status in {"completed", "failed"}:
                    break
                threading.Event().wait(0.01)
            self.assertEqual(current.status, "completed")
            self.assertEqual(current.result["urls"], ["https://evohime.dev"])
            self.assertEqual(current.result["tickets"], ["ABC-9"])
        finally:
            service.close()

    def test_diff_reports_line_changes(self):
        result = diff_text("alpha\nbeta\ngamma\n", "alpha\nbeta2\ngamma\ndelta\n")
        self.assertEqual(result["lines_a"], 3)
        self.assertEqual(result["lines_b"], 4)
        self.assertEqual(result["lines_removed"], 1)
        self.assertEqual(result["lines_added"], 2)
        self.assertLess(result["ratio"], 1.0)
        self.assertTrue(any(line.startswith("-beta") for line in result["unified_diff"]))
        self.assertTrue(any(line.startswith("+delta") for line in result["unified_diff"]))
        self.assertFalse(result["diff_truncated"])

    def test_diff_truncates_unified_output(self):
        left = "\n".join(f"line-{i}" for i in range(40))
        right = "\n".join(f"line-{i}-x" for i in range(40))
        result = diff_text(left, right, context=0, max_diff_lines=5)
        self.assertTrue(result["diff_truncated"])
        self.assertEqual(len(result["unified_diff"]), 5)

    def test_diff_rejects_bad_context(self):
        with self.assertRaisesRegex(ValueError, "context"):
            validate_task_payload(
                "text.diff",
                {"text_a": "a", "text_b": "b", "context": 99},
            )

    def test_diff_job_completes(self):
        service = JobService()
        try:
            job = service.submit(
                "text.diff",
                {"text_a": "one\ntwo\n", "text_b": "one\nthree\n"},
            )
            for _ in range(100):
                if (current := service.get(job.id)).status in {"completed", "failed"}:
                    break
                threading.Event().wait(0.01)
            self.assertEqual(current.status, "completed")
            self.assertEqual(current.result["lines_removed"], 1)
            self.assertEqual(current.result["lines_added"], 1)
        finally:
            service.close()

    def test_text_summarize_job_completes(self):
        service = JobService()
        try:
            job = service.submit(
                "text.summarize",
                {"text": "Alpha beta. Beta gamma beta. Tiny.", "max_sentences": 1},
            )
            for _ in range(100):
                if (current := service.get(job.id)).status in {"completed", "failed"}:
                    break
                threading.Event().wait(0.01)
            self.assertEqual(current.status, "completed")
            self.assertEqual(current.result["sentences_used"], 1)
            self.assertEqual(current.result["source_sentences"], ["Beta gamma beta."])
        finally:
            service.close()

    def test_running_job_exposes_heartbeat(self):
        service = JobService()
        try:
            # Block the single worker with a long-running-ish echo after we inject delay
            # via a custom task path: submit then poll while status is running.
            # Use text.stats on a large-enough string; heartbeat is set when status=running.
            job = service.submit("text.stats", {"text": "heartbeat"})
            saw_heartbeat = False
            for _ in range(100):
                current = service.get(job.id)
                if current.status == "running" and current.heartbeat_at:
                    saw_heartbeat = True
                    break
                if current.status in {"completed", "failed"}:
                    # Job may finish before we observe running; completed still keeps heartbeat.
                    self.assertIsNotNone(current.heartbeat_at)
                    saw_heartbeat = True
                    break
                threading.Event().wait(0.01)
            self.assertTrue(saw_heartbeat)
            self.assertIsInstance(service.get(job.id).heartbeat_at, str)
        finally:
            service.close()

    def test_text_keywords_returns_deterministic_frequencies(self):
        service = JobService()
        try:
            job = service.submit("text.keywords", {"text": "Rust rust worker, worker task"})
            for _ in range(100):
                if (current := service.get(job.id)).status in {"completed", "failed"}:
                    break
                threading.Event().wait(0.01)
            self.assertEqual(current.status, "completed")
            self.assertEqual(current.result["keywords"], [
                {"word": "rust", "count": 2},
                {"word": "worker", "count": 2},
                {"word": "task", "count": 1},
            ])
        finally:
            service.close()


class HttpWorkerTests(unittest.TestCase):
    def test_health_includes_process_liveness_fields(self):
        server = create_server(port=0)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        connection = HTTPConnection(*server.server_address)
        try:
            connection.request("GET", "/health")
            response = connection.getresponse()
            health_payload = json.loads(response.read())
            self.assertEqual(response.status, 200)
            self.assertEqual(health_payload["status"], "ok")
            self.assertIsInstance(health_payload["started_at"], str)
            self.assertTrue(health_payload["started_at"])
            self.assertIsInstance(health_payload["pid"], int)
            self.assertGreater(health_payload["pid"], 0)
        finally:
            connection.close()
            server.shutdown()
            server.service.close()
            server.server_close()
            thread.join(timeout=2)

    def test_health_and_job_lifecycle(self):
        server = create_server(port=0)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        connection = HTTPConnection(*server.server_address)
        try:
            connection.request("GET", "/health")
            response = connection.getresponse()
            health_payload = json.loads(response.read())
            self.assertEqual(response.status, 200)
            self.assertIn("text.stats", health_payload["supported_tasks"])

            body = json.dumps({"task": "text.stats", "payload": {"text": "hello world"}})
            connection.request("POST", "/v1/jobs", body, {"Content-Type": "application/json"})
            response = connection.getresponse()
            job = json.loads(response.read())
            self.assertEqual(response.status, 202)

            for _ in range(100):
                connection.request("GET", f"/v1/jobs/{job['id']}")
                response = connection.getresponse()
                current = json.loads(response.read())
                if current["status"] == "completed":
                    break
                threading.Event().wait(0.01)
            self.assertEqual(current["result"]["words"], 2)
            self.assertIn("heartbeat_at", current)
        finally:
            connection.close()
            server.shutdown()
            server.service.close()
            server.server_close()
            thread.join(timeout=2)

    def test_invalid_payload_is_rejected_over_http(self):
        server = create_server(port=0)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        connection = HTTPConnection(*server.server_address)
        try:
            body = json.dumps({"task": "text.stats", "payload": {"text": 1}})
            connection.request("POST", "/v1/jobs", body, {"Content-Type": "application/json"})
            response = connection.getresponse()
            payload = json.loads(response.read())
            self.assertEqual(response.status, 400)
            self.assertIn("payload.text", payload["error"])
        finally:
            connection.close()
            server.shutdown()
            server.service.close()
            server.server_close()
            thread.join(timeout=2)


if __name__ == "__main__":
    unittest.main()
