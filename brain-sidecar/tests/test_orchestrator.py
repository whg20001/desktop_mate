from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from typing import Any

from desktop_companion_brain.config import SidecarConfig
from desktop_companion_brain.orchestrator import ConversationOrchestrator
from desktop_companion_brain.session_store import ConversationSessionStore


class FakeLlm:
    def __init__(self) -> None:
        self.calls = 0

    def complete(self, **_: Any) -> dict[str, Any]:
        self.calls += 1
        return {
            "text": "你好。",
            "emotion": {"type": "happy", "intensity": 4},
            "actionIntent": {"id": "greeting", "intensity": 0.7},
        }


class FakeMemory:
    def __init__(self) -> None:
        self.operations: list[str] = []

    def ready(self):
        return True, "ready"

    def search(self, query, scope, limit):
        self.operations.append("recall")
        return [{"id": "m1", "content": "喜欢蓝色", "score": 1.0}]

    def remember_turn(self, messages, scope, turn_id):
        self.operations.append("write")


class OrchestratorTests(unittest.TestCase):
    def test_turn_is_structured_and_idempotent(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = SidecarConfig(
                host="127.0.0.1",
                port=8765,
                token="a" * 32,
                app_data_root=root,
                data_dir=root,
                llm_base_url="http://127.0.0.1:11434/v1",
                llm_model="local",
                embedding_base_url="http://127.0.0.1:11434/v1",
                embedding_model="embed",
                memory_enabled=True,
                recall_enabled=True,
                memory_write_enabled=True,
                recall_limit=6,
                request_timeout_seconds=5,
            )
            llm = FakeLlm()
            memory = FakeMemory()
            sessions = ConversationSessionStore(root / "session.sqlite3")
            orchestrator = ConversationOrchestrator(
                config,
                sessions,
                llm,
                memory,
            )
            request = {
                "turnId": "turn-1",
                "scope": {"userId": "u", "characterId": "c", "sessionId": "s"},
                "userInput": "你好",
                "availableActions": [{"id": "greeting", "description": "wave"}],
            }
            first = orchestrator.converse(request)
            second = orchestrator.converse(request)
            self.assertEqual(first, second)
            self.assertEqual(llm.calls, 1)
            self.assertEqual(memory.operations, ["recall"])
            self.assertEqual(first["emotion"]["intensity"], 1.0)
            self.assertEqual(first["actionIntent"]["id"], "greeting")
            self.assertTrue(sessions.memory_write_pending("turn-1"))
            self.assertEqual(len(sessions.pending_memory_writes()), 1)
            sessions.close()

    def test_unknown_action_is_not_forwarded(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = SidecarConfig(
                host="127.0.0.1",
                port=8765,
                token="a" * 32,
                app_data_root=root,
                data_dir=root,
                llm_base_url="http://127.0.0.1:11434/v1",
                llm_model="local",
                embedding_base_url="http://127.0.0.1:11434/v1",
                embedding_model="embed",
                memory_enabled=True,
                recall_enabled=False,
                memory_write_enabled=False,
                recall_limit=6,
                request_timeout_seconds=5,
            )
            llm = FakeLlm()
            memory = FakeMemory()
            sessions = ConversationSessionStore(root / "session.sqlite3")
            response = ConversationOrchestrator(
                config,
                sessions,
                llm,
                memory,
            ).converse(
                {
                    "turnId": "turn-2",
                    "scope": {"userId": "u", "characterId": "c", "sessionId": "s"},
                    "userInput": "你好",
                    "availableActions": [{"id": "idle", "description": "rest"}],
                }
            )
            self.assertNotIn("actionIntent", response)
            self.assertEqual(memory.operations, [])
            self.assertFalse(sessions.memory_write_pending("turn-2"))
            sessions.close()


if __name__ == "__main__":
    unittest.main()
