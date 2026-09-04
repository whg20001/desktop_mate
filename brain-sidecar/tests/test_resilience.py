from __future__ import annotations

import tempfile
import unittest
from http import HTTPStatus
from pathlib import Path
from typing import Any
from unittest.mock import patch

from desktop_companion_brain.config import SidecarConfig
from desktop_companion_brain.orchestrator import ConversationOrchestrator
from desktop_companion_brain.server import BrainApplication
from desktop_companion_brain.session_store import ConversationSessionStore


def config(root: Path, *, memory_enabled: bool = True) -> SidecarConfig:
    return SidecarConfig(
        host="127.0.0.1",
        port=0,
        token="t" * 32,
        app_data_root=root,
        data_dir=root,
        llm_base_url="http://127.0.0.1:11434/v1",
        llm_model="local-model",
        embedding_base_url=(
            "http://127.0.0.1:11434/v1" if memory_enabled else ""
        ),
        embedding_model="local-embedding" if memory_enabled else "",
        memory_enabled=memory_enabled,
        recall_enabled=memory_enabled,
        memory_write_enabled=memory_enabled,
        recall_limit=6,
        request_timeout_seconds=5,
    )


class ReadyLlm:
    def __init__(self) -> None:
        self.calls = 0

    def ready(self) -> tuple[bool, str]:
        return True, "ready"

    def complete(self, **_: Any) -> dict[str, Any]:
        self.calls += 1
        return {"text": "local response"}


class FailingLlm(ReadyLlm):
    def ready(self) -> tuple[bool, str]:
        return False, "local LLM is offline"

    def complete(self, **_: Any) -> dict[str, Any]:
        self.calls += 1
        raise RuntimeError("local LLM is offline")


class ReadyMemory:
    def ready(self) -> tuple[bool, str]:
        return True, "ready"

    def search(
        self, query: str, scope: dict[str, str], limit: int
    ) -> list[dict[str, Any]]:
        return []

    def remember_turn(
        self,
        messages: list[dict[str, str]],
        scope: dict[str, str],
        turn_id: str,
    ) -> None:
        return None


class UnavailableMemory(ReadyMemory):
    def ready(self) -> tuple[bool, str]:
        return False, "Mem0 is offline"


class FlakyMemory(ReadyMemory):
    def __init__(self, failures: int) -> None:
        self.failures = failures
        self.write_attempts: list[tuple[str, list[dict[str, str]]]] = []

    def remember_turn(
        self,
        messages: list[dict[str, str]],
        scope: dict[str, str],
        turn_id: str,
    ) -> None:
        self.write_attempts.append((turn_id, messages))
        if self.failures:
            self.failures -= 1
            raise RuntimeError("Mem0 temporarily unavailable")


class OrchestratorRecoveryTests(unittest.TestCase):
    def test_failed_memory_write_is_retried_from_sqlite_after_restart(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / "sessions.sqlite3"
            request = {
                "turnId": "turn-outbox",
                "scope": {"userId": "u", "characterId": "c", "sessionId": "s"},
                "userInput": "remember this preference",
                "availableActions": [],
            }

            first_memory = FlakyMemory(failures=1)
            first_llm = ReadyLlm()
            first_store = ConversationSessionStore(database)
            first = ConversationOrchestrator(
                config(root), first_store, first_llm, first_memory
            ).converse(request)
            self.assertEqual(first_llm.calls, 1)
            self.assertTrue(first_store.memory_write_pending("turn-outbox"))
            self.assertIn("memory write unavailable", first["degradedReasons"][0])
            first_store.close()

            recovered_memory = FlakyMemory(failures=0)
            recovered_llm = ReadyLlm()
            recovered_store = ConversationSessionStore(database)
            recovered = ConversationOrchestrator(
                config(root), recovered_store, recovered_llm, recovered_memory
            ).converse(request)

            self.assertEqual(recovered["turnId"], "turn-outbox")
            self.assertEqual(recovered_llm.calls, 0, "a cached turn must not call the LLM again")
            self.assertEqual(len(recovered_memory.write_attempts), 1)
            self.assertFalse(recovered_store.memory_write_pending("turn-outbox"))
            recovered_store.close()

    def test_same_turn_id_with_different_input_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            store = ConversationSessionStore(root / "sessions.sqlite3")
            orchestrator = ConversationOrchestrator(
                config(root, memory_enabled=False), store, ReadyLlm(), ReadyMemory()
            )
            try:
                request = {
                    "turnId": "turn-conflict",
                    "scope": {"userId": "u", "characterId": "c", "sessionId": "s"},
                    "userInput": "first payload",
                    "availableActions": [],
                }
                orchestrator.converse(request)
                conflicting = dict(request, userInput="different payload")
                with self.assertRaises(ValueError):
                    orchestrator.converse(conflicting)
            finally:
                store.close()

    def test_recall_and_write_failures_degrade_without_losing_the_reply(self) -> None:
        class FullyUnavailableMemory(FlakyMemory):
            def search(self, query, scope, limit):
                raise RuntimeError("Mem0 search unavailable")

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            store = ConversationSessionStore(root / "sessions.sqlite3")
            memory = FullyUnavailableMemory(failures=1)
            response = ConversationOrchestrator(
                config(root), store, ReadyLlm(), memory
            ).converse(
                {
                    "turnId": "turn-degraded",
                    "scope": {"userId": "u", "characterId": "c", "sessionId": "s"},
                    "userInput": "hello",
                    "availableActions": [],
                }
            )
            self.assertEqual(response["text"], "local response")
            self.assertEqual(len(response["degradedReasons"]), 2)
            self.assertTrue(store.memory_write_pending("turn-degraded"))
            store.close()

    def test_llm_failure_does_not_commit_a_partial_turn(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            store = ConversationSessionStore(root / "sessions.sqlite3")
            request = {
                "turnId": "turn-llm-failed",
                "scope": {"userId": "u", "characterId": "c", "sessionId": "s"},
                "userInput": "hello",
                "availableActions": [],
            }
            orchestrator = ConversationOrchestrator(
                config(root, memory_enabled=False), store, FailingLlm(), ReadyMemory()
            )
            with self.assertRaises(RuntimeError):
                orchestrator.converse(request)
            self.assertIsNone(store.cached_response("turn-llm-failed", request["scope"]))
            store.close()


class ReadinessTests(unittest.TestCase):
    def test_each_required_local_dependency_can_make_readiness_degraded(self) -> None:
        cases = (
            (FailingLlm(), ReadyMemory(), (True, "ready"), "llm"),
            (ReadyLlm(), UnavailableMemory(), (True, "ready"), "memory"),
            (ReadyLlm(), ReadyMemory(), (False, "embedding offline"), "embedding"),
        )
        for llm, memory, embedding, component in cases:
            with self.subTest(component=component), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                with patch("desktop_companion_brain.server._embedding_health", return_value=embedding):
                    application = BrainApplication(config(root), llm=llm, memory=memory)
                    status, body = application.health()
                self.assertEqual(status, HTTPStatus.SERVICE_UNAVAILABLE)
                self.assertEqual(body["status"], "degraded")
                self.assertFalse(body["components"][component]["ready"])
                application.sessions.close()

    def test_missing_configuration_is_degraded_instead_of_crashing_startup(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            missing = config(root, memory_enabled=False)
            missing = SidecarConfig(**{**missing.__dict__, "llm_base_url": "", "llm_model": ""})
            application = BrainApplication(missing, memory=ReadyMemory())
            status, body = application.health()
            self.assertEqual(status, HTTPStatus.SERVICE_UNAVAILABLE)
            self.assertIn("local LLM endpoint and model are not configured", body["configurationErrors"])
            application.sessions.close()


if __name__ == "__main__":
    unittest.main()
