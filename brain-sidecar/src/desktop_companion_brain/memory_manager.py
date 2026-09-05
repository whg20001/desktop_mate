from __future__ import annotations

import threading
import time
from datetime import UTC, datetime
from typing import Any

from .config import SidecarConfig
from .memory import LongTermMemoryProvider, Mem0Memory, contains_sensitive
from .memory_inference import LocalMemoryInferenceProvider, MemoryInferenceProvider
from .memory_policy import ApprovedMemoryEvent, MemoryPolicy, memory_content_hash
from .memory_store import MemoryEventStore, ProviderDelivery
from .session_store import ConversationSessionStore


class MemoryManager:
    """Owns the canonical SQLite facts and rebuildable provider indexes."""

    def __init__(
        self,
        config: SidecarConfig,
        sessions: ConversationSessionStore | None = None,
        *,
        providers: list[LongTermMemoryProvider] | None = None,
        inference: MemoryInferenceProvider | None = None,
    ) -> None:
        self.config = config
        self.sessions = sessions
        self.store = MemoryEventStore(config.data_dir / "memory-events.sqlite3")
        self.store.purge_tombstones(config.memory_retention_days)
        self.policy = MemoryPolicy(
            minimum_importance=config.memory_minimum_importance,
            require_confirmation=config.memory_approval_required,
        )
        self._inference_error: str | None = None
        try:
            self.inference = inference or LocalMemoryInferenceProvider(
                config.llm_base_url,
                config.llm_model,
                config.request_timeout_seconds,
            )
        except Exception as error:
            self.inference = None
            self._inference_error = (
                f"memory inference unavailable: {type(error).__name__}"
            )
        if providers is None:
            providers = [Mem0Memory(config)]
            if config.graphiti_enabled:
                from .graphiti_provider import GraphitiMemory

                providers.append(GraphitiMemory(config))
        self.providers = {provider.provider_id: provider for provider in providers}
        self._stop = threading.Event()
        self._wake = threading.Event()
        self._worker = threading.Thread(
            target=self._run_worker,
            name="memory-provider-outbox",
            daemon=True,
        )
        self._worker.start()

    def ready(self) -> tuple[bool, str]:
        states = self._provider_states()
        mem0_states = [state for state in states if state["id"] == "mem0"]
        mem0_ready = not mem0_states or any(state["ready"] for state in mem0_states)
        summary = ", ".join(
            f"{state['id']}={'ready' if state['ready'] else 'degraded'}"
            for state in states
        )
        return mem0_ready, "canonical local SQLite is ready; " + summary

    def remember_turn(
        self,
        messages: list[dict[str, str]],
        scope: dict[str, str],
        turn_id: str,
    ) -> None:
        if self.inference is None:
            raise RuntimeError(self._inference_error or "memory inference is unavailable")
        candidates = self.inference.extract(messages, scope, turn_id)
        for candidate in candidates:
            decision = self.policy.review(candidate, scope, (turn_id,))
            if decision.event is not None:
                self.store.add(decision.event, self.providers)
        self._wake.set()

    def search(
        self,
        query: str,
        scope: dict[str, str],
        limit: int,
    ) -> list[dict[str, Any]]:
        requested = max(1, min(limit, 20))
        candidates = self._canonical_search(query, scope, requested * 2)
        for name, provider in self.providers.items():
            try:
                if not provider.ready()[0]:
                    continue
                for record in provider.search(query, scope, requested * 2):
                    candidates.append(_provider_record(record, name))
            except Exception:
                continue
        return _merge_results(candidates, requested)

    def list(self, scope: dict[str, str]) -> list[dict[str, Any]]:
        canonical = [_event_record(event, self._event_providers(event.event_id)) for event in self.store.list(scope)]
        known_provider_ids = {
            provider_id
            for record in canonical
            for provider_id in record["providers"].values()
            if provider_id
        }
        mem0 = self.providers.get("mem0")
        if mem0 is not None:
            try:
                for record in mem0.list(scope):
                    if record["id"] not in known_provider_ids:
                        legacy = _provider_record(record, "mem0")
                        legacy["id"] = "legacy:mem0:" + record["id"]
                        legacy["status"] = "approved"
                        legacy["kind"] = "semantic"
                        legacy["importance"] = 0.5
                        legacy["providers"] = {"mem0": record["id"]}
                        canonical.append(legacy)
            except Exception:
                pass
        return canonical

    def update(self, memory_id: str, content: str, scope: dict[str, str]) -> None:
        normalized, content_hash = self.policy.validate_edit(content)
        legacy = _legacy_id(memory_id)
        if legacy is not None:
            provider, provider_id = legacy
            target = self.providers.get(provider)
            update = getattr(target, "update", None)
            if not callable(update):
                raise ValueError("legacy memory provider is unavailable")
            update(provider_id, normalized, scope)
            return
        self.store.update(memory_id, scope, normalized, content_hash, self.providers)
        self._wake.set()

    def delete(self, memory_id: str, scope: dict[str, str]) -> None:
        legacy = _legacy_id(memory_id)
        if legacy is not None:
            provider, provider_id = legacy
            target = self.providers.get(provider)
            if target is None:
                raise ValueError("legacy memory provider is unavailable")
            target.delete_record(provider_id, scope)
            return
        self.store.delete(memory_id, scope, self.providers)
        self._wake.set()

    def approve(self, memory_id: str, scope: dict[str, str]) -> None:
        self.store.approve(memory_id, scope, self.providers)
        self._wake.set()

    def reject(self, memory_id: str, scope: dict[str, str]) -> None:
        self.store.reject(memory_id, scope)

    def rebuild(self, provider: str, scope: dict[str, str] | None = None) -> int:
        if provider not in self.providers:
            raise ValueError("memory provider is not enabled")
        count = self.store.queue_rebuild(provider, scope)
        self._wake.set()
        return count

    def status(self) -> dict[str, Any]:
        provider_states = self._provider_states()
        mem0_states = [
            state for state in provider_states if state["id"] == "mem0"
        ]
        ready = not mem0_states or any(state["ready"] for state in mem0_states)
        return {
            "enabled": True,
            "ready": ready,
            "approvalRequired": self.config.memory_approval_required,
            "inferenceReady": self.inference is not None,
            "providers": provider_states,
            **self.store.stats(),
        }

    def _provider_states(self) -> list[dict[str, Any]]:
        states: list[dict[str, Any]] = []
        for name, provider in self.providers.items():
            try:
                ready, detail = provider.ready()
            except Exception as error:
                ready = False
                detail = f"provider health failed: {type(error).__name__}"
            states.append({"id": name, "ready": ready, "detail": detail})
        return states

    def close(self) -> None:
        self._stop.set()
        self._wake.set()
        self._worker.join(timeout=5.0)
        if self._worker.is_alive():
            return
        for provider in self.providers.values():
            try:
                provider.close()
            except Exception:
                pass
        self.store.close()

    def _run_worker(self) -> None:
        next_session_recovery = 0.0
        while not self._stop.is_set():
            worked = self._deliver_batch()
            if self.sessions is not None and time.monotonic() >= next_session_recovery:
                self._recover_session_writes()
                next_session_recovery = time.monotonic() + 30.0
            if not worked:
                self._wake.wait(timeout=1.0)
                self._wake.clear()

    def _deliver_batch(self) -> bool:
        deliveries = self.store.pending_deliveries(providers=self.providers)
        processed = False
        for delivery in deliveries:
            if self._stop.is_set():
                break
            provider = self.providers.get(delivery.provider)
            if provider is None:
                continue
            processed = True
            try:
                provider_record_id = self._deliver(provider, delivery)
                self.store.complete_delivery(delivery, provider_record_id)
            except Exception as error:
                self.store.fail_delivery(delivery, error)
        return processed

    def _deliver(
        self,
        provider: LongTermMemoryProvider,
        delivery: ProviderDelivery,
    ) -> str | None:
        if delivery.operation == "delete":
            provider.delete_record(
                delivery.provider_record_id or delivery.event.event_id,
                delivery.event.scope,
            )
            return delivery.provider_record_id
        return provider.upsert(delivery.event, delivery.provider_record_id)

    def _recover_session_writes(self) -> None:
        if self.sessions is None:
            return
        for pending in self.sessions.pending_memory_writes(16):
            try:
                self.remember_turn(
                    pending["messages"],
                    pending["scope"],
                    pending["turnId"],
                )
                self.sessions.complete_memory_write(pending["turnId"])
            except Exception:
                return

    def _canonical_search(
        self,
        query: str,
        scope: dict[str, str],
        limit: int,
    ) -> list[dict[str, Any]]:
        query_terms = set(query.casefold().split())
        scored: list[dict[str, Any]] = []
        for event in self.store.active(scope):
            terms = set(event.content.casefold().split())
            overlap = len(query_terms.intersection(terms))
            score = 0.35 + min(0.35, overlap * 0.08) + event.importance * 0.3
            record = _event_record(event, self._event_providers(event.event_id))
            record["score"] = min(score, 1.0)
            scored.append(record)
        return sorted(scored, key=lambda item: item["score"], reverse=True)[:limit]

    def _event_providers(self, event_id: str) -> dict[str, str | None]:
        return {
            name: self.store.provider_record_id(event_id, name)
            for name in self.providers
        }


def _event_record(
    event: ApprovedMemoryEvent,
    providers: dict[str, str | None],
) -> dict[str, Any]:
    timestamp = datetime.fromtimestamp(event.created_at_unix_ms / 1000, tz=UTC).isoformat()
    return {
        "id": event.event_id,
        "content": event.content,
        "score": event.importance,
        "status": event.status,
        "kind": event.kind,
        "importance": event.importance,
        "sources": ["sqlite"],
        "providers": providers,
        "metadata": event.metadata,
        "createdAt": timestamp,
        "updatedAt": timestamp,
    }


def _provider_record(record: dict[str, Any], provider: str) -> dict[str, Any]:
    result = dict(record)
    result["sources"] = [provider]
    result.setdefault("status", "approved")
    result.setdefault("kind", "semantic")
    result.setdefault("importance", result.get("score", 0.5))
    result.setdefault("providers", {provider: record.get("id")})
    return result


def _merge_results(records: list[dict[str, Any]], limit: int) -> list[dict[str, Any]]:
    merged: dict[str, dict[str, Any]] = {}
    for record in records:
        content = str(record.get("content", "")).strip()
        if not content or len(content) > 2_000 or contains_sensitive(content):
            continue
        key = memory_content_hash(content)
        current = merged.get(key)
        if current is None:
            current = dict(record)
            current["content"] = content
            current["sources"] = list(record.get("sources", []))
            merged[key] = current
        else:
            current["score"] = max(float(current.get("score", 0)), float(record.get("score", 0)))
            current["sources"] = list(
                dict.fromkeys([*current.get("sources", []), *record.get("sources", [])])
            )
    ordered = sorted(merged.values(), key=lambda item: float(item.get("score", 0)), reverse=True)
    result: list[dict[str, Any]] = []
    characters = 0
    for record in ordered:
        if characters + len(record["content"]) > 6_000:
            continue
        result.append(record)
        characters += len(record["content"])
        if len(result) == limit:
            break
    return result


def _legacy_id(memory_id: str) -> tuple[str, str] | None:
    prefix = "legacy:mem0:"
    if memory_id.startswith(prefix) and len(memory_id) > len(prefix):
        return "mem0", memory_id[len(prefix) :]
    return None
