from __future__ import annotations

import asyncio
import os
import socket
import sqlite3
import tempfile
import threading
import time
import unittest
import uuid
from pathlib import Path
from typing import Any
from unittest.mock import patch

from desktop_companion_brain.config import SidecarConfig
from desktop_companion_brain.graphiti_provider import GraphitiMemory, _AsyncRunner
from desktop_companion_brain.memory_manager import MemoryManager
from desktop_companion_brain.memory_policy import (
    ApprovedMemoryEvent,
    MemoryCandidate,
    MemoryPolicy,
    memory_content_hash,
)
from desktop_companion_brain.memory_store import MemoryEventStore
from desktop_companion_brain.security import LocalityError, local_graph_url


def make_config(
    root: Path,
    *,
    graphiti_enabled: bool = False,
    approval_required: bool = False,
) -> SidecarConfig:
    return SidecarConfig(
        host="127.0.0.1",
        port=0,
        token="t" * 32,
        app_data_root=root,
        data_dir=root,
        llm_base_url="http://127.0.0.1:11434/v1",
        llm_model="local-model",
        embedding_base_url="http://127.0.0.1:11434/v1",
        embedding_model="local-embedding",
        memory_enabled=True,
        recall_enabled=True,
        memory_write_enabled=True,
        recall_limit=6,
        request_timeout_seconds=2,
        graphiti_enabled=graphiti_enabled,
        memory_approval_required=approval_required,
    )


def make_scope(
    user: str = "user-a",
    character: str = "character-a",
    session: str = "session-a",
) -> dict[str, str]:
    return {"userId": user, "characterId": character, "sessionId": session}


def make_event(
    content: str,
    scope: dict[str, str] | None = None,
    *,
    kind: str = "semantic",
    status: str = "approved",
    importance: float = 0.8,
) -> ApprovedMemoryEvent:
    now = int(time.time() * 1_000)
    return ApprovedMemoryEvent(
        event_id=str(uuid.uuid4()),
        scope=dict(scope or make_scope()),
        content=content,
        content_hash=memory_content_hash(content),
        kind=kind,
        importance=importance,
        tags=(),
        occurred_at_unix_ms=None,
        created_at_unix_ms=now,
        source_event_ids=("turn-a",),
        metadata={"source": "conversation"},
        status=status,
    )


class FakeInference:
    def __init__(self, candidates: list[MemoryCandidate]) -> None:
        self.candidates = candidates

    def extract(
        self,
        messages: list[dict[str, str]],
        scope: dict[str, str],
        turn_id: str,
    ) -> list[MemoryCandidate]:
        return list(self.candidates)


class FakeProvider:
    def __init__(
        self,
        provider_id: str,
        *,
        ready: bool = True,
        search_records: list[dict[str, Any]] | None = None,
        search_error: Exception | None = None,
        upsert_error: Exception | None = None,
        close_error: Exception | None = None,
    ) -> None:
        self.provider_id = provider_id
        self.is_ready = ready
        self.search_records = search_records or []
        self.search_error = search_error
        self.upsert_error = upsert_error
        self.close_error = close_error
        self.search_calls = 0
        self.upsert_calls: list[tuple[str, int]] = []
        self.delete_calls: list[tuple[str, dict[str, str]]] = []
        self.closed = False

    def ready(self) -> tuple[bool, str]:
        return self.is_ready, "ready" if self.is_ready else "offline"

    def search(
        self,
        query: str,
        scope: dict[str, str],
        limit: int,
    ) -> list[dict[str, Any]]:
        self.search_calls += 1
        if self.search_error is not None:
            raise self.search_error
        return [dict(record) for record in self.search_records[:limit]]

    def list(self, scope: dict[str, str]) -> list[dict[str, Any]]:
        return []

    def upsert(
        self,
        event: ApprovedMemoryEvent,
        provider_record_id: str | None,
    ) -> str:
        self.upsert_calls.append((event.event_id, event.revision))
        if self.upsert_error is not None:
            raise self.upsert_error
        return provider_record_id or f"{self.provider_id}-{event.event_id}"

    def delete_record(
        self,
        provider_record_id: str,
        scope: dict[str, str],
    ) -> None:
        self.delete_calls.append((provider_record_id, dict(scope)))

    def close(self) -> None:
        self.closed = True
        if self.close_error is not None:
            raise self.close_error


class MemoryPolicyTests(unittest.TestCase):
    def test_sensitive_content_and_low_importance_are_rejected(self) -> None:
        policy = MemoryPolicy(minimum_importance=0.6, require_confirmation=False)
        for content in (
            "my password is hunter2",
            "API key: local-secret",
            "access token abc123",
        ):
            with self.subTest(content=content):
                decision = policy.review(
                    MemoryCandidate(content=content, importance=0.9),
                    make_scope(),
                    ("turn-a",),
                )
                self.assertEqual(decision.status, "rejected")
                self.assertEqual(decision.reason, "sensitive")
                self.assertIsNone(decision.event)

        decision = policy.review(
            MemoryCandidate(content="Prefers tea", importance=0.59),
            make_scope(),
            ("turn-a",),
        )
        self.assertEqual(decision.reason, "below_importance_threshold")

    def test_sensitive_auxiliary_fields_are_filtered(self) -> None:
        policy = MemoryPolicy(minimum_importance=0.5, require_confirmation=False)
        decision = policy.review(
            MemoryCandidate(
                content="Prefers jasmine tea",
                importance=0.8,
                tags=("preference", "api key secret"),
                metadata={
                    "source": "conversation",
                    "scene": "password shown on screen",
                    "ignored": "must not survive allow-listing",
                },
            ),
            make_scope(),
            ("turn-safe", "refresh token secret"),
        )
        self.assertEqual(decision.status, "approved")
        self.assertIsNotNone(decision.event)
        assert decision.event is not None
        self.assertEqual(decision.event.tags, ("preference",))
        self.assertEqual(decision.event.metadata, {"source": "conversation"})
        self.assertEqual(decision.event.source_event_ids, ("turn-safe",))

    def test_confirmation_and_automatic_approval_have_distinct_statuses(self) -> None:
        candidate = MemoryCandidate(content="Prefers dark mode", importance=0.8)
        pending = MemoryPolicy(
            minimum_importance=0.5,
            require_confirmation=True,
        ).review(candidate, make_scope(), ("turn-a",))
        approved = MemoryPolicy(
            minimum_importance=0.5,
            require_confirmation=False,
        ).review(candidate, make_scope(), ("turn-a",))
        self.assertEqual(pending.status, "pending")
        self.assertEqual(pending.event.status, "pending")
        self.assertEqual(approved.status, "approved")
        self.assertEqual(approved.event.status, "approved")

    def test_approval_queues_provider_delivery_and_kind_is_part_of_deduplication(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            store = MemoryEventStore(Path(directory) / "events.sqlite3")
            try:
                pending = make_event("Likes local-first software", status="pending")
                store.add(pending, ("mem0",))
                self.assertEqual(store.pending_deliveries(), [])
                store.approve(pending.event_id, pending.scope, ("mem0",))
                deliveries = store.pending_deliveries()
                self.assertEqual(len(deliveries), 1)
                self.assertEqual(deliveries[0].event.revision, 2)

                semantic = make_event("Shared memory text", kind="semantic")
                episodic = make_event("Shared memory text", kind="episodic")
                store.add(semantic, ())
                store.add(episodic, ())
                matching = [
                    event
                    for event in store.active(semantic.scope)
                    if event.content == "Shared memory text"
                ]
                self.assertEqual({event.kind for event in matching}, {"semantic", "episodic"})
            finally:
                store.close()


class MemoryOutboxRevisionTests(unittest.TestCase):
    def test_provider_filter_skips_disabled_provider_backlog(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            store = MemoryEventStore(Path(directory) / 'events.sqlite3')
            try:
                for index in range(33):
                    store.add(make_event(f'Graphiti fact {index}'), ('graphiti',))
                mem0_event = make_event('Mem0 fact after Graphiti backlog')
                store.add(mem0_event, ('mem0',))
                deliveries = store.pending_deliveries(limit=32, providers=('mem0',))
                self.assertEqual(len(deliveries), 1)
                self.assertEqual(deliveries[0].provider, 'mem0')
                self.assertEqual(deliveries[0].event.event_id, mem0_event.event_id)
                self.assertEqual(
                    len(store.pending_deliveries(providers=('graphiti',))),
                    32,
                )
            finally:
                store.close()

    def test_update_and_delete_requeue_historical_providers(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            store = MemoryEventStore(Path(directory) / 'events.sqlite3')
            try:
                event = make_event('Fact indexed by Graphiti')
                store.add(event, ('graphiti',))
                graphiti = store.pending_deliveries()[0]
                self.assertTrue(store.complete_delivery(graphiti, 'graph-record'))

                store.update(
                    event.event_id,
                    event.scope,
                    'Updated fact',
                    memory_content_hash('Updated fact'),
                    ('mem0',),
                )
                updated = {
                    delivery.provider: delivery
                    for delivery in store.pending_deliveries()
                }
                self.assertEqual(set(updated), {'graphiti', 'mem0'})
                self.assertEqual(updated['graphiti'].operation, 'upsert')
                self.assertEqual(updated['graphiti'].provider_record_id, 'graph-record')
                self.assertTrue(
                    store.complete_delivery(updated['graphiti'], 'graph-record')
                )
                self.assertTrue(store.complete_delivery(updated['mem0'], 'mem0-record'))

                store.delete(event.event_id, event.scope, ('mem0',))
                deleted = {
                    delivery.provider: delivery
                    for delivery in store.pending_deliveries()
                }
                self.assertEqual(set(deleted), {'graphiti', 'mem0'})
                self.assertEqual(deleted['graphiti'].operation, 'delete')
                self.assertEqual(deleted['graphiti'].provider_record_id, 'graph-record')
                self.assertEqual(deleted['mem0'].provider_record_id, 'mem0-record')
            finally:
                store.close()

    def test_stale_upsert_completion_does_not_complete_new_revision(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            store = MemoryEventStore(Path(directory) / "events.sqlite3")
            try:
                event = make_event("Original fact")
                store.add(event, ("mem0",))
                old_delivery = store.pending_deliveries()[0]
                store.update(
                    event.event_id,
                    event.scope,
                    "Corrected fact",
                    memory_content_hash("Corrected fact"),
                    ("mem0",),
                )
                latest = store.pending_deliveries()[0]
                self.assertEqual(latest.event.revision, 2)
                self.assertFalse(store.complete_delivery(old_delivery, "stale-record"))
                still_pending = store.pending_deliveries()
                self.assertEqual(len(still_pending), 1)
                self.assertEqual(still_pending[0].event.revision, 2)
                self.assertTrue(store.complete_delivery(latest, "current-record"))
                self.assertEqual(store.pending_deliveries(), [])
                self.assertEqual(
                    store.provider_record_id(event.event_id, "mem0"),
                    "current-record",
                )
            finally:
                store.close()

    def test_stale_failure_does_not_delay_new_revision(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            store = MemoryEventStore(Path(directory) / "events.sqlite3")
            try:
                event = make_event("Original fact")
                store.add(event, ("mem0",))
                old_delivery = store.pending_deliveries()[0]
                store.update(
                    event.event_id,
                    event.scope,
                    "New fact",
                    memory_content_hash("New fact"),
                    ("mem0",),
                )
                self.assertFalse(store.fail_delivery(old_delivery, RuntimeError("old failure")))
                current = store.pending_deliveries()
                self.assertEqual(len(current), 1)
                self.assertEqual(current[0].attempts, 0)
                self.assertEqual(current[0].event.revision, 2)
            finally:
                store.close()

    def test_stale_upsert_cannot_swallow_delete_delivery(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            store = MemoryEventStore(Path(directory) / "events.sqlite3")
            try:
                event = make_event("Fact to remove")
                store.add(event, ("mem0",))
                old_upsert = store.pending_deliveries()[0]
                store.delete(event.event_id, event.scope, ("mem0",))
                latest_delete = store.pending_deliveries()[0]
                self.assertEqual(latest_delete.operation, "delete")
                self.assertEqual(latest_delete.event.revision, 2)
                self.assertFalse(store.complete_delivery(old_upsert, "late-record"))
                pending = store.pending_deliveries()
                self.assertEqual(len(pending), 1)
                self.assertEqual(pending[0].operation, "delete")
                self.assertTrue(store.complete_delivery(latest_delete, None))
                self.assertEqual(store.pending_deliveries(), [])
                self.assertIsNone(store.provider_record_id(event.event_id, "mem0"))
            finally:
                store.close()


class MemoryManagerTests(unittest.TestCase):
    def test_close_isolates_provider_errors_and_closes_store(self) -> None:
        failing = FakeProvider('mem0', close_error=RuntimeError('close failed'))
        healthy = FakeProvider('graphiti')
        with tempfile.TemporaryDirectory() as directory:
            manager = MemoryManager(
                make_config(Path(directory)),
                providers=[failing, healthy],
                inference=FakeInference([]),
            )
            manager.close()
            self.assertTrue(failing.closed)
            self.assertTrue(healthy.closed)
            with self.assertRaises(sqlite3.ProgrammingError):
                manager.store.stats()

    def test_canonical_results_survive_provider_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            provider = FakeProvider("mem0", search_error=RuntimeError("offline"))
            manager = MemoryManager(
                make_config(Path(directory)),
                providers=[provider],
                inference=FakeInference([]),
            )
            try:
                event = make_event("Prefers jasmine tea")
                manager.store.add(event, ())
                results = manager.search("jasmine tea", event.scope, 5)
                self.assertEqual([record["content"] for record in results], [event.content])
                self.assertEqual(results[0]["sources"], ["sqlite"])
                self.assertEqual(provider.search_calls, 1)
            finally:
                manager.close()

    def test_provider_results_are_merged_by_content_hash(self) -> None:
        content = "Prefers local-first software"
        mem0 = FakeProvider(
            "mem0",
            search_records=[{"id": "m1", "content": content, "score": 0.92}],
        )
        graphiti = FakeProvider(
            "graphiti",
            search_records=[{"id": "g1", "content": content.upper(), "score": 0.81}],
        )
        with tempfile.TemporaryDirectory() as directory:
            manager = MemoryManager(
                make_config(Path(directory), graphiti_enabled=True),
                providers=[mem0, graphiti],
                inference=FakeInference([]),
            )
            try:
                event = make_event(content, importance=0.7)
                manager.store.add(event, ())
                results = manager.search("local-first", event.scope, 5)
                self.assertEqual(len(results), 1)
                self.assertEqual(results[0]["score"], 0.92)
                self.assertEqual(
                    results[0]["sources"],
                    ["sqlite", "mem0", "graphiti"],
                )
            finally:
                manager.close()

    def test_user_and_character_scopes_isolate_all_mutations_and_reads(self) -> None:
        owner = make_scope("owner", "companion-a")
        other_user = make_scope("other", "companion-a")
        other_character = make_scope("owner", "companion-b")
        with tempfile.TemporaryDirectory() as directory:
            manager = MemoryManager(
                make_config(Path(directory)),
                providers=[],
                inference=FakeInference([]),
            )
            try:
                event = make_event("Owner likes oolong", owner)
                manager.store.add(event, ())
                self.assertEqual(len(manager.list(owner)), 1)
                for foreign_scope in (other_user, other_character):
                    with self.subTest(scope=foreign_scope):
                        self.assertEqual(manager.list(foreign_scope), [])
                        self.assertEqual(manager.search("oolong", foreign_scope, 5), [])
                        with self.assertRaises(ValueError):
                            manager.update(event.event_id, "Tampered", foreign_scope)
                        with self.assertRaises(ValueError):
                            manager.delete(event.event_id, foreign_scope)
                manager.update(event.event_id, "Owner likes green tea", owner)
                self.assertEqual(manager.store.get(event.event_id, owner).revision, 2)
                manager.delete(event.event_id, owner)
                self.assertEqual(manager.list(owner), [])
                self.assertEqual(manager.search("green tea", owner, 5), [])
            finally:
                manager.close()

    def test_pending_memory_requires_approval_and_rejected_memory_is_erased(self) -> None:
        candidate = MemoryCandidate(content="Prefers reduced motion", importance=0.9)
        with tempfile.TemporaryDirectory() as directory:
            manager = MemoryManager(
                make_config(Path(directory), approval_required=True),
                providers=[],
                inference=FakeInference([candidate]),
            )
            try:
                scope = make_scope()
                manager.remember_turn([{"role": "user", "content": "remember it"}], scope, "t1")
                pending = manager.list(scope)
                self.assertEqual(len(pending), 1)
                self.assertEqual(pending[0]["status"], "pending")
                self.assertEqual(manager.search("reduced motion", scope, 5), [])
                manager.approve(pending[0]["id"], scope)
                self.assertEqual(len(manager.search("reduced motion", scope, 5)), 1)

                manager.inference = FakeInference(
                    [MemoryCandidate(content="Prefers compact replies", importance=0.9)]
                )
                manager.remember_turn(
                    [{"role": "user", "content": "another preference"}],
                    scope,
                    "t2",
                )
                second = next(
                    record for record in manager.list(scope) if record["status"] == "pending"
                )
                manager.reject(second["id"], scope)
                self.assertNotIn(second["id"], {record["id"] for record in manager.list(scope)})
                rejected = manager.store.get(second["id"], scope)
                self.assertEqual(rejected.status, "rejected")
                self.assertEqual(rejected.content, "")
            finally:
                manager.close()

    def test_unavailable_graphiti_degrades_without_being_called(self) -> None:
        graphiti = FakeProvider("graphiti", ready=False)
        with tempfile.TemporaryDirectory() as directory:
            manager = MemoryManager(
                make_config(Path(directory), graphiti_enabled=True),
                providers=[graphiti],
                inference=FakeInference([]),
            )
            try:
                event = make_event("Canonical data remains available")
                manager.store.add(event, ())
                ready, detail = manager.ready()
                status = manager.status()
                results = manager.search("canonical", event.scope, 5)
                self.assertTrue(ready)
                self.assertIn("graphiti=degraded", detail)
                self.assertTrue(status["ready"])
                self.assertFalse(status["providers"][0]["ready"])
                self.assertEqual(len(results), 1)
                self.assertEqual(graphiti.search_calls, 0)
            finally:
                manager.close()

    def test_graphiti_search_failure_does_not_hide_mem0_or_canonical_results(self) -> None:
        mem0 = FakeProvider(
            "mem0",
            search_records=[{"id": "m1", "content": "Provider-only fact", "score": 0.8}],
        )
        graphiti = FakeProvider(
            "graphiti",
            search_error=RuntimeError("graph unavailable"),
        )
        with tempfile.TemporaryDirectory() as directory:
            manager = MemoryManager(
                make_config(Path(directory), graphiti_enabled=True),
                providers=[mem0, graphiti],
                inference=FakeInference([]),
            )
            try:
                event = make_event("Canonical fact")
                manager.store.add(event, ())
                contents = {
                    record["content"]
                    for record in manager.search("fact", event.scope, 5)
                }
                self.assertEqual(contents, {"Canonical fact", "Provider-only fact"})
            finally:
                manager.close()

    def test_graphiti_is_not_created_when_disabled(self) -> None:
        mem0 = FakeProvider("mem0")
        with tempfile.TemporaryDirectory() as directory:
            with patch(
                "desktop_companion_brain.memory_manager.Mem0Memory",
                return_value=mem0,
            ):
                manager = MemoryManager(
                    make_config(Path(directory), graphiti_enabled=False),
                    inference=FakeInference([]),
                )
            try:
                self.assertEqual(set(manager.providers), {"mem0"})
            finally:
                manager.close()
        self.assertTrue(mem0.closed)


class GraphLocalityTests(unittest.TestCase):
    @staticmethod
    def _loopback_answers(host: str, port: int, **_: Any) -> list[tuple[Any, ...]]:
        if host == "::1":
            return [(socket.AF_INET6, socket.SOCK_STREAM, 6, "", ("::1", port, 0, 0))]
        return [(socket.AF_INET, socket.SOCK_STREAM, 6, "", ("127.0.0.1", port))]

    def test_only_loopback_graph_endpoints_are_accepted(self) -> None:
        accepted = (
            "bolt://127.0.0.1:7687",
            "bolt://[::1]:7687",
        )
        with patch(
            "desktop_companion_brain.security.socket.getaddrinfo",
            side_effect=self._loopback_answers,
        ):
            for value in accepted:
                with self.subTest(value=value):
                    self.assertEqual(local_graph_url(value), value)

    def test_remote_or_credentialed_graph_endpoints_are_rejected(self) -> None:
        rejected = (
            "bolt://192.168.1.10:7687",
            "neo4j://example.com:7687",
            "neo4j://localhost:7687",
            "https://127.0.0.1:7687",
            "bolt://user:pass@127.0.0.1:7687",
            "bolt://127.0.0.1:7687?x=1",
            "bolt://127.0.0.1:7687#fragment",
        )
        for value in rejected:
            with self.subTest(value=value), self.assertRaises(LocalityError):
                local_graph_url(value)

    def test_mixed_graph_dns_answers_are_rejected(self) -> None:
        answers = [
            (socket.AF_INET, socket.SOCK_STREAM, 6, "", ("127.0.0.1", 7687)),
            (socket.AF_INET, socket.SOCK_STREAM, 6, "", ("203.0.113.7", 7687)),
        ]
        with patch(
            "desktop_companion_brain.security.socket.getaddrinfo",
            return_value=answers,
        ):
            with self.assertRaises(LocalityError):
                local_graph_url("bolt://localhost:7687")

    def test_graph_endpoint_paths_are_rejected(self) -> None:
        with self.assertRaises(LocalityError):
            local_graph_url('bolt://127.0.0.1:7687/other')

    def test_normal_graph_identifiers_load_from_local_settings(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            data = root / "brain"
            data.mkdir()
            (data / "settings.json").write_text(
                '{"graphitiDatabase":"neo4j","graphitiUser":"desktop-user"}',
                encoding="utf-8",
            )
            environment = {
                "DESKTOP_COMPANION_BRAIN_HOST": "127.0.0.1",
                "DESKTOP_COMPANION_BRAIN_PORT": "8765",
                "DESKTOP_COMPANION_BRAIN_TOKEN": "t" * 32,
                "DESKTOP_COMPANION_APP_DATA_ROOT": str(root),
                "DESKTOP_COMPANION_BRAIN_DATA_DIR": str(data),
            }
            with patch.dict(os.environ, environment, clear=True):
                config = SidecarConfig.from_environment()
            self.assertEqual(config.graphiti_database, "neo4j")
            self.assertEqual(config.graphiti_user, "desktop-user")


class GraphitiLifecycleTests(unittest.TestCase):
    def test_runner_close_cancels_pending_tasks_before_stopping_thread(self) -> None:
        started = threading.Event()
        finalized = threading.Event()

        async def pending() -> None:
            started.set()
            try:
                await asyncio.Future()
            finally:
                finalized.set()

        runner = _AsyncRunner()
        thread = runner._thread
        future = runner.submit(pending())
        self.assertTrue(started.wait(timeout=1))
        runner.close()
        self.assertTrue(finalized.wait(timeout=1))
        self.assertTrue(future.done())
        self.assertFalse(thread.is_alive())

    def test_graphiti_close_cancels_initialization_and_is_idempotent(self) -> None:
        started = threading.Event()
        finalized = threading.Event()
        graph_closed = threading.Event()

        class InitializingGraph:
            async def close(self) -> None:
                graph_closed.set()

        async def initialize(memory: GraphitiMemory) -> None:
            memory._initializing_graph = InitializingGraph()
            started.set()
            try:
                await asyncio.Future()
            finally:
                finalized.set()

        with tempfile.TemporaryDirectory() as directory:
            with patch.object(GraphitiMemory, "_initialize", initialize):
                memory = GraphitiMemory(make_config(Path(directory)))
            thread = memory._runner._thread
            self.assertTrue(started.wait(timeout=1))
            memory.close()
            memory.close()
            self.assertTrue(finalized.wait(timeout=1))
            self.assertTrue(graph_closed.wait(timeout=1))
            self.assertFalse(thread.is_alive())


if __name__ == "__main__":
    unittest.main()
