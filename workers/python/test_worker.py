import json
import threading
import unittest
from http.client import HTTPConnection

from worker import EMBEDDING_DIMENSION, JobService, create_server, health


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

    def test_unknown_task_is_reported_as_failed(self):
        service = JobService()
        try:
            job = service.submit("missing", {})
            for _ in range(100):
                if (current := service.get(job.id)).status == "failed":
                    break
                threading.Event().wait(0.01)
            self.assertEqual(current.status, "failed")
            self.assertIn("unsupported task", current.error)
        finally:
            service.close()

    def test_text_embed_is_deterministic_and_normalized(self):
        service = JobService()
        try:
            job = service.submit("text.embed", {"text": "hello world"})
            for _ in range(100):
                if (current := service.get(job.id)).status in {"completed", "failed"}:
                    break
                threading.Event().wait(0.01)
            self.assertEqual(current.status, "completed")
            self.assertEqual(current.result["dimension"], EMBEDDING_DIMENSION)
            self.assertAlmostEqual(current.result["norm"], 1.0, places=6)

            repeat = service.submit("text.embed", {"text": "hello world"})
            for _ in range(100):
                if (repeat_current := service.get(repeat.id)).status in {"completed", "failed"}:
                    break
                threading.Event().wait(0.01)
            self.assertEqual(repeat_current.result, current.result)
        finally:
            service.close()

    def test_text_embed_rejects_empty_input(self):
        service = JobService()
        try:
            job = service.submit("text.embed", {"text": "  "})
            for _ in range(100):
                if (current := service.get(job.id)).status == "failed":
                    break
                threading.Event().wait(0.01)
            self.assertEqual(current.status, "failed")
            self.assertIn("non-empty", current.error)
        finally:
            service.close()


class HttpWorkerTests(unittest.TestCase):
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
        finally:
            connection.close()
            server.shutdown()
            server.service.close()
            server.server_close()
            thread.join(timeout=2)


if __name__ == "__main__":
    unittest.main()
