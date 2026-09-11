from __future__ import annotations

import sqlite3
import tempfile
import threading
import time
import unittest
from concurrent.futures import ThreadPoolExecutor
from contextlib import closing
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from desktop_companion_brain.memory_manager import MemoryManager
from desktop_companion_brain.memory_policy import MemoryCandidate
from desktop_companion_brain.orchestrator import ConversationOrchestrator
from desktop_companion_brain.session_store import ConversationSessionStore
from test_memory_phase2 import FakeInference, FakeProvider, make_config, make_scope
from test_orchestrator import FakeLlm


def request(turn_id: str = 'turn-1') -> dict:
    return {
        'turnId': turn_id,
        'scope': make_scope(),
        'userInput': 'I prefer short replies',
        'availableActions': [],
    }


def wait_until(predicate) -> None:
    deadline = time.monotonic() + 3
    while not predicate():
        if time.monotonic() >= deadline:
            raise AssertionError('background memory operation did not complete')
        time.sleep(0.01)


class BlockingInference(FakeInference):
    def __init__(self) -> None:
        super().__init__([MemoryCandidate(content='Prefers short replies', importance=0.9)])
        self.started = threading.Event()
        self.release = threading.Event()
        self.calls = 0

    def extract(self, messages, scope, turn_id, *, existing_memories=None):
        self.calls += 1
        self.started.set()
        if not self.release.wait(timeout=5):
            raise RuntimeError('test inference timed out')
        return super().extract(messages, scope, turn_id)


class BackgroundMemoryTests(unittest.TestCase):
    def test_reply_and_concurrent_retries_do_not_wait_for_memory_extraction(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            sessions = ConversationSessionStore(root / 'sessions.sqlite3')
            inference = BlockingInference()
            manager = MemoryManager(make_config(root), sessions, providers=[], inference=inference)
            llm = FakeLlm()
            orchestrator = ConversationOrchestrator(make_config(root), sessions, llm, manager)
            executor = ThreadPoolExecutor(max_workers=2)
            try:
                replies = [executor.submit(orchestrator.converse, request()) for _ in range(2)]
                first, second = [reply.result(timeout=1) for reply in replies]
                self.assertEqual(first, second)
                self.assertEqual(llm.calls, 1)
                manager._wake.set()
                self.assertTrue(inference.started.wait(timeout=2))
                self.assertTrue(sessions.memory_write_pending('turn-1'))
                cached = executor.submit(orchestrator.converse, request()).result(timeout=1)
                self.assertEqual(cached, first)
                self.assertEqual(inference.calls, 1)
                inference.release.set()
                wait_until(lambda: not sessions.memory_write_pending('turn-1'))
                self.assertEqual(len(manager.list(make_scope())), 1)
                self.assertEqual(inference.calls, 1)
            finally:
                inference.release.set()
                executor.shutdown(wait=True)
                manager.close()
                sessions.close()

    def test_failed_extraction_is_deferred_without_blocking_later_turns_and_recovers(self) -> None:
        class FailingInference(FakeInference):
            def __init__(self):
                super().__init__([])
                self.calls = []

            def extract(self, messages, scope, turn_id, *, existing_memories=None):
                self.calls.append(turn_id)
                if turn_id == 'bad':
                    raise RuntimeError('offline')
                return []

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / 'sessions.sqlite3'
            sessions = ConversationSessionStore(database)
            inference = FailingInference()
            manager = MemoryManager(make_config(root), sessions, providers=[], inference=inference)
            try:
                orchestrator = ConversationOrchestrator(make_config(root), sessions, FakeLlm(), manager)
                orchestrator.converse(request('bad'))
                orchestrator.converse(request('good'))
                manager._wake.set()
                wait_until(lambda: not sessions.memory_write_pending('good'))
                self.assertTrue(sessions.memory_write_pending('bad'))
                self.assertEqual(sessions.pending_memory_writes(), [])
                self.assertEqual(inference.calls.count('bad'), 1)
            finally:
                manager.close()
                sessions.close()

            recovered = ConversationSessionStore(database)
            self.assertTrue(recovered.memory_write_pending('bad'))
            self.assertEqual(recovered.pending_memory_writes(), [])
            # Move beyond the persisted retry delay; no new conversation is needed.
            with patch('desktop_companion_brain.session_store.time.time', return_value=time.time() + 31):
                manager = MemoryManager(
                    make_config(root), recovered, providers=[],
                    inference=FakeInference([MemoryCandidate(content='Recovered fact', importance=0.9)]),
                )
                try:
                    wait_until(lambda: not recovered.memory_write_pending('bad'))
                    self.assertEqual(len(manager.list(make_scope())), 1)
                finally:
                    manager.close()
                    recovered.close()

    def test_disabled_writes_leave_previous_pending_turns_untouched(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            sessions = ConversationSessionStore(root / 'sessions.sqlite3')
            sessions.commit('old', make_scope(), 'hello', 'reply', {'text': 'reply'}, True)
            inference = BlockingInference()
            manager = MemoryManager(
                replace(make_config(root), memory_write_enabled=False), sessions,
                providers=[], inference=inference,
            )
            try:
                self.assertFalse(manager._recover_session_writes())
                self.assertTrue(sessions.memory_write_pending('old'))
                self.assertEqual(inference.calls, 0)
            finally:
                inference.release.set()
                manager.close()
                sessions.close()

    def test_shutdown_during_extraction_leaves_recoverable_work_and_closes_resources(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / 'sessions.sqlite3'
            sessions = ConversationSessionStore(database)
            sessions.commit('pending', make_scope(), 'hello', 'reply', {'text': 'reply'}, True)
            inference = BlockingInference()
            provider = FakeProvider('mem0')
            manager = MemoryManager(make_config(root), sessions, providers=[provider], inference=inference)
            try:
                self.assertTrue(inference.started.wait(timeout=2))
                # Simulate reaching close's join timeout while inference is in flight.
                with patch.object(manager._worker, 'join'):
                    manager.close()
                with patch.object(sessions, 'complete_memory_write', wraps=sessions.complete_memory_write) as complete:
                    sessions.close()
                    inference.release.set()
                    manager._worker.join(timeout=2)
                    self.assertFalse(manager._worker.is_alive())
                    complete.assert_not_called()
                self.assertTrue(provider.closed)
                recovered = ConversationSessionStore(database)
                try:
                    self.assertTrue(recovered.memory_write_pending('pending'))
                finally:
                    recovered.close()
            finally:
                inference.release.set()
                manager.close()

    def test_existing_session_database_migrates_without_losing_pending_turns(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            database = Path(directory) / 'sessions.sqlite3'
            sessions = ConversationSessionStore(database)
            sessions.commit('old', make_scope(), 'hello', 'reply', {'text': 'reply'}, True)
            sessions.close()
            with closing(sqlite3.connect(database)) as connection:
                connection.execute('ALTER TABLE conversation_turns DROP COLUMN memory_retry_at_unix_ms')
            migrated = ConversationSessionStore(database)
            try:
                self.assertEqual(migrated.pending_memory_writes()[0]['turnId'], 'old')
                self.assertEqual(migrated.cached_response('old', make_scope()), {'text': 'reply'})
                migrated.defer_memory_write('old')
                self.assertEqual(migrated.pending_memory_writes(), [])
            finally:
                migrated.close()


if __name__ == '__main__':
    unittest.main()
