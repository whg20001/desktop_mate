from __future__ import annotations

import http.client
import io
import json
import logging
import tempfile
import threading
import unittest
from pathlib import Path

from desktop_companion_brain.config import SidecarConfig
from desktop_companion_brain.server import BrainApplication, BrainHttpServer


class FakeLlm:
    def ready(self):
        return True, "ready"

    def complete(self, **kwargs):
        return {"text": "本地回复", "emotion": {"type": "happy", "intensity": 0.5}}


class FakeMemory:
    def ready(self):
        return True, "disabled for test"

    def search(self, query, scope, limit):
        return []

    def remember_turn(self, messages, scope, turn_id):
        raise AssertionError("disabled memory must not write")

    def list(self, scope):
        return []


class FailingConversationLlm(FakeLlm):
    def complete(self, **kwargs):
        raise RuntimeError("private provider failure details")


class ServerTests(unittest.TestCase):
    def test_health_requires_token_and_conversation_is_available(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = SidecarConfig(
                host="127.0.0.1",
                port=0,
                token="s" * 32,
                app_data_root=root,
                data_dir=root,
                llm_base_url="http://127.0.0.1:11434/v1",
                llm_model="local",
                embedding_base_url="",
                embedding_model="",
                memory_enabled=False,
                recall_enabled=False,
                memory_write_enabled=False,
                recall_limit=6,
                request_timeout_seconds=5,
            )
            application = BrainApplication(config, llm=FakeLlm(), memory=FakeMemory())
            server = BrainHttpServer(application)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            port = server.server_address[1]
            try:
                connection = http.client.HTTPConnection("127.0.0.1", port, timeout=2)
                connection.request("GET", "/ready")
                self.assertEqual(connection.getresponse().status, 401)
                connection.close()

                connection = http.client.HTTPConnection("127.0.0.1", port, timeout=2)
                connection.request(
                    "GET",
                    "/ready",
                    headers={"x-desktop-companion-token": config.token},
                )
                self.assertEqual(connection.getresponse().status, 200)
                connection.close()

                payload = json.dumps(
                    {
                        "turnId": "turn-http",
                        "scope": {
                            "userId": "u",
                            "characterId": "c",
                            "sessionId": "s",
                        },
                        "userInput": "你好",
                        "availableActions": [],
                    }
                )
                connection = http.client.HTTPConnection("127.0.0.1", port, timeout=2)
                connection.request(
                    "POST",
                    "/v1/conversation",
                    body=payload,
                    headers={
                        "content-type": "application/json",
                        "x-desktop-companion-token": config.token,
                    },
                )
                response = connection.getresponse()
                self.assertEqual(response.status, 200)
                self.assertEqual(json.loads(response.read())["text"], "本地回复")
                connection.close()
            finally:
                server.shutdown()
                server.server_close()
                application.sessions.close()
                thread.join(timeout=2)

    def test_all_routes_reject_missing_or_incorrect_tokens(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = SidecarConfig(
                host="127.0.0.1",
                port=0,
                token="s" * 32,
                app_data_root=root,
                data_dir=root,
                llm_base_url="http://127.0.0.1:11434/v1",
                llm_model="local",
                embedding_base_url="",
                embedding_model="",
                memory_enabled=False,
                recall_enabled=False,
                memory_write_enabled=False,
                recall_limit=6,
                request_timeout_seconds=5,
            )
            application = BrainApplication(config, llm=FakeLlm(), memory=FakeMemory())
            server = BrainHttpServer(application)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            port = server.server_address[1]
            try:
                for method, path, body in (
                    ("GET", "/live", None),
                    ("GET", "/ready", None),
                    ("POST", "/v1/conversation", b"{}"),
                    ("POST", "/shutdown", b"{}"),
                    ("PATCH", "/v1/memories/id", b"{}"),
                    ("DELETE", "/v1/memories/id", b"{}"),
                ):
                    for token in (None, "wrong-token"):
                        with self.subTest(method=method, path=path, token=token):
                            headers = {"content-type": "application/json"}
                            if token is not None:
                                headers["x-desktop-companion-token"] = token
                            connection = http.client.HTTPConnection(
                                "127.0.0.1", port, timeout=2
                            )
                            connection.request(method, path, body=body, headers=headers)
                            response = connection.getresponse()
                            self.assertEqual(response.status, 401)
                            self.assertEqual(
                                json.loads(response.read()), {"error": "unauthorized"}
                            )
                            connection.close()
            finally:
                server.shutdown()
                server.server_close()
                application.sessions.close()
                thread.join(timeout=2)

    def test_authenticated_shutdown_stops_the_server_without_a_residual_thread(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = SidecarConfig(
                host="127.0.0.1",
                port=0,
                token="s" * 32,
                app_data_root=root,
                data_dir=root,
                llm_base_url="http://127.0.0.1:11434/v1",
                llm_model="local",
                embedding_base_url="",
                embedding_model="",
                memory_enabled=False,
                recall_enabled=False,
                memory_write_enabled=False,
                recall_limit=6,
                request_timeout_seconds=5,
            )
            application = BrainApplication(config, llm=FakeLlm(), memory=FakeMemory())
            server = BrainHttpServer(application)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            connection = http.client.HTTPConnection(
                "127.0.0.1", server.server_address[1], timeout=2
            )
            connection.request(
                "POST",
                "/shutdown",
                body="{}",
                headers={
                    "content-type": "application/json",
                    "x-desktop-companion-token": config.token,
                },
            )
            response = connection.getresponse()
            self.assertEqual(response.status, 200)
            response.read()
            connection.close()
            thread.join(timeout=2)
            try:
                self.assertFalse(thread.is_alive())
            finally:
                server.server_close()
                application.sessions.close()

    def test_request_logs_do_not_contain_token_prompt_or_memory_content(self) -> None:
        secret_token = "token-that-must-not-be-logged-123"
        private_prompt = "private prompt must not be logged"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = SidecarConfig(
                host="127.0.0.1",
                port=0,
                token=secret_token,
                app_data_root=root,
                data_dir=root,
                llm_base_url="http://127.0.0.1:11434/v1",
                llm_model="local",
                embedding_base_url="",
                embedding_model="",
                memory_enabled=False,
                recall_enabled=False,
                memory_write_enabled=False,
                recall_limit=6,
                request_timeout_seconds=5,
            )
            application = BrainApplication(config, llm=FakeLlm(), memory=FakeMemory())
            server = BrainHttpServer(application)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            stream = io.StringIO()
            handler = logging.StreamHandler(stream)
            logger = logging.getLogger("desktop_companion_brain")
            logger.addHandler(handler)
            logger.setLevel(logging.INFO)
            thread.start()
            try:
                body = json.dumps(
                    {
                        "turnId": "turn-log",
                        "scope": {
                            "userId": "u",
                            "characterId": "c",
                            "sessionId": "s",
                        },
                        "userInput": private_prompt,
                        "availableActions": [],
                    }
                )
                connection = http.client.HTTPConnection(
                    "127.0.0.1", server.server_address[1], timeout=2
                )
                connection.request(
                    "POST",
                    "/v1/conversation?opaque=should-not-appear",
                    body=body,
                    headers={
                        "content-type": "application/json",
                        "x-desktop-companion-token": secret_token,
                    },
                )
                connection.getresponse().read()
                connection.close()
                logs = stream.getvalue()
                self.assertNotIn(secret_token, logs)
                self.assertNotIn(private_prompt, logs)
                self.assertNotIn("opaque=should-not-appear", logs)
            finally:
                logger.removeHandler(handler)
                server.shutdown()
                server.server_close()
                application.sessions.close()
                thread.join(timeout=2)

    def test_llm_failure_returns_degraded_error_and_server_remains_live(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = SidecarConfig(
                host="127.0.0.1",
                port=0,
                token="s" * 32,
                app_data_root=root,
                data_dir=root,
                llm_base_url="http://127.0.0.1:11434/v1",
                llm_model="local",
                embedding_base_url="",
                embedding_model="",
                memory_enabled=False,
                recall_enabled=False,
                memory_write_enabled=False,
                recall_limit=6,
                request_timeout_seconds=5,
            )
            application = BrainApplication(
                config, llm=FailingConversationLlm(), memory=FakeMemory()
            )
            server = BrainHttpServer(application)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                headers = {
                    "content-type": "application/json",
                    "x-desktop-companion-token": config.token,
                }
                connection = http.client.HTTPConnection(
                    "127.0.0.1", server.server_address[1], timeout=2
                )
                connection.request(
                    "POST",
                    "/v1/conversation",
                    body=json.dumps(
                        {
                            "turnId": "turn-provider-failed",
                            "scope": {
                                "userId": "u",
                                "characterId": "c",
                                "sessionId": "s",
                            },
                            "userInput": "hello",
                            "availableActions": [],
                        }
                    ),
                    headers=headers,
                )
                response = connection.getresponse()
                self.assertEqual(response.status, 503)
                error = json.loads(response.read())["error"]
                self.assertNotIn("private provider failure details", error)
                connection.close()

                connection = http.client.HTTPConnection(
                    "127.0.0.1", server.server_address[1], timeout=2
                )
                connection.request(
                    "GET",
                    "/live",
                    headers={"x-desktop-companion-token": config.token},
                )
                response = connection.getresponse()
                self.assertEqual(response.status, 200)
                response.read()
                connection.close()
            finally:
                server.shutdown()
                server.server_close()
                application.sessions.close()
                thread.join(timeout=2)


if __name__ == "__main__":
    unittest.main()
