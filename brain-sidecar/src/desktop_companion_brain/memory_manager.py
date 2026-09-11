from __future__ import annotations

import logging
import threading
import uuid
from dataclasses import replace
from datetime import UTC, datetime
from typing import Any

from .config import SidecarConfig
from .memory import LongTermMemoryProvider, create_providers, contains_sensitive
from .memory_inference import LocalMemoryInferenceProvider, MemoryInferenceProvider
from .memory_policy import ApprovedMemoryEvent, MemoryCandidate, MemoryPolicy
from .retrieval import lexical_score
from .memory_store import MemoryEventStore, ProviderDelivery
from .session_store import ConversationSessionStore

LOGGER = logging.getLogger("desktop_companion_brain")


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
            providers = create_providers(config)
        self.providers = {provider.provider_id: provider for provider in providers}
        self._stop = threading.Event()
        self._wake = threading.Event()
        self._session_access = threading.Lock()
        self._extraction_lock = threading.Lock()
        self._worker_error: str | None = None
        self._resources_closed = False
        self._worker = threading.Thread(
            target=self._run_worker,
            name="memory-provider-outbox",
            daemon=True,
        )
        self._worker.start()

    def ready(self) -> tuple[bool, str]:
        states = self._provider_states()
        summary = ", ".join(
            f"{state['id']}={'ready' if state['ready'] else 'degraded'}"
            for state in states
        )
        return self._worker.is_alive() and self._worker_error is None, "canonical local SQLite is ready; " + summary

    def remember_turn(
        self,
        messages: list[dict[str, str]],
        scope: dict[str, str],
        turn_id: str,
    ) -> None:
        with self._extraction_lock:
            if self.store.turn_processed(scope, turn_id):
                return
            if self.inference is None:
                raise RuntimeError(self._inference_error or "memory inference is unavailable")
            query = ' '.join(message['content'] for message in messages if message.get('role') == 'user')
            existing = self._canonical_search(query, scope, 16)
            candidates = self.inference.extract(
                messages, scope, turn_id,
                existing_memories=[{'id': item['id'], 'content': item['content'], 'kind': item['kind']}
                                   for item in existing],
            )
            events = []
            allowed_ids = {item['id'] for item in existing if item['kind'] == 'semantic'}
            for candidate in candidates:
                decision = self.policy.review(candidate, scope, (turn_id,), messages=messages)
                if decision.event is not None:
                    event = decision.event
                    event.metadata['supersedes'] = [identifier for identifier in event.metadata.get('supersedes', [])
                                                    if event.kind == 'semantic' and identifier in allowed_ids]
                    event.metadata['supersedesVersions'] = {
                        item['id']: item['revision'] for item in existing
                        if item['id'] in event.metadata['supersedes']
                    }
                    events.append(event)
            self.store.apply_turn(scope, turn_id, events, self.providers)
        self._wake.set()

    def search(
        self,
        query: str,
        scope: dict[str, str],
        limit: int,
    ) -> list[dict[str, Any]]:
        requested = max(1, min(limit, 20))
        rankings = [self._canonical_search(query, scope, requested * 2)]
        for name, provider in self.providers.items():
            try:
                if not provider.ready()[0]:
                    continue
                ranking = []
                for record in provider.search(query, scope, requested * 2):
                    ranking.extend(self._validated_hits(record, name, scope))
                rankings.append(ranking)
            except Exception:
                continue
        results = _merge_rankings(rankings, requested)
        # A provider request can be slow: revalidate after all requests complete,
        # so a delete or edit during retrieval cannot publish stale content.
        final = []
        for record in results:
            event = self.store.get(record['id'], scope)
            if event.status == 'approved' and event.revision == record['revision']:
                latest = _event_record(event, self._event_providers(event.event_id))
                latest['score'], latest['sources'] = record['score'], record['sources']
                record = latest
                final.append(record)
        return final

    def list(self, scope: dict[str, str]) -> list[dict[str, Any]]:
        for name, provider in self.providers.items():
            try:
                for record in provider.list(scope):
                    self._import_legacy(record, name, scope)
            except Exception:
                pass
        return [_event_record(event, self._event_providers(event.event_id)) for event in self.store.list(scope)]

    def update(self, memory_id: str, content: str, scope: dict[str, str]) -> None:
        normalized, content_hash = self.policy.validate_edit(content)
        legacy = _legacy_id(memory_id)
        if legacy is not None:
            memory_id = self._resolve_legacy_id(legacy, scope)
        self.store.update(memory_id, scope, normalized, content_hash, self.providers)
        self._wake.set()

    def delete(self, memory_id: str, scope: dict[str, str]) -> None:
        legacy = _legacy_id(memory_id)
        if legacy is not None:
            memory_id = self._resolve_legacy_id(legacy, scope)
        self.store.delete(memory_id, scope, self.providers)
        self._wake.set()

    def approve(self, memory_id: str, scope: dict[str, str]) -> None:
        self.store.approve(memory_id, scope, self.providers)
        self._wake.set()

    def reject(self, memory_id: str, scope: dict[str, str]) -> None:
        self.store.reject(memory_id, scope)
        self._wake.set()

    def rebuild(self, provider: str, scope: dict[str, str] | None = None) -> int:
        if provider not in self.providers:
            raise ValueError("memory provider is not enabled")
        count = self.store.queue_rebuild(provider, scope)
        self._wake.set()
        return count

    def status(self) -> dict[str, Any]:
        provider_states = self._provider_states()
        queue = self.sessions.memory_queue_status() if self.sessions else {}
        ready = self._worker.is_alive() and self._worker_error is None
        return {
            "enabled": True,
            "ready": ready,
            "approvalRequired": self.config.memory_approval_required,
            "inferenceReady": self.inference is not None and not queue.get('lastError'),
            "workerAlive": self._worker.is_alive(),
            "workerError": self._worker_error,
            "extractionQueue": queue,
            "providers": provider_states,
            **self.store.stats(),
        }

    def retry(self, scope: dict[str, str]) -> None:
        with self._session_access:
            if self._stop.is_set():
                raise RuntimeError('memory manager is stopping')
            if self.sessions:
                self.sessions.retry_memory_writes(scope)
            self.store.retry_deliveries(scope)
            if not self._worker.is_alive():
                self._worker_error = None
                self._worker = threading.Thread(target=self._run_worker, name='memory-provider-outbox', daemon=True)
                self._worker.start()
            self._wake.set()

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
        # After this lock is released, the worker cannot access the session
        # connection again, so BrainApplication may safely close it.
        with self._session_access:
            self._stop.set()
        self._wake.set()
        self._worker.join(timeout=5.0)
        if not self._worker.is_alive() and not self._resources_closed:
            self._close_resources()

    def _close_resources(self) -> None:
        for provider in self.providers.values():
            try:
                provider.close()
            except Exception:
                pass
        self.store.close()
        self._resources_closed = True

    def _run_worker(self) -> None:
        try:
            while not self._stop.is_set():
                self._wake.clear()
                worked = self._deliver_batch()
                worked = self._recover_session_writes() or worked
                if not worked:
                    self._wake.wait(timeout=1.0)
        except Exception as error:
            self._worker_error = type(error).__name__
            LOGGER.error('memory worker stopped (%s)', self._worker_error)
        finally:
            # The worker owns its resources even when close() times out while
            # inference is in flight. Do not close them from another thread.
            if self._stop.is_set():
                self._close_resources()

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

    def _recover_session_writes(self) -> bool:
        if self.sessions is None or not self.config.memory_write_enabled:
            return False
        with self._session_access:
            if self._stop.is_set():
                return False
            pending_writes = self.sessions.pending_memory_writes(1)
        worked = False
        for pending in pending_writes:
            if self._stop.is_set():
                break
            worked = True
            try:
                self.remember_turn(
                    pending["messages"],
                    pending["scope"],
                    pending["turnId"],
                )
                with self._session_access:
                    if self._stop.is_set():
                        break
                    self.sessions.complete_memory_write(pending["turnId"])
            except Exception as error:
                with self._session_access:
                    if self._stop.is_set():
                        break
                    self.sessions.defer_memory_write(pending["turnId"], type(error).__name__)
                LOGGER.warning("memory extraction deferred (%s)", type(error).__name__)
        return worked

    def _canonical_search(
        self,
        query: str,
        scope: dict[str, str],
        limit: int,
    ) -> list[dict[str, Any]]:
        scored: list[dict[str, Any]] = []
        for event in self.store.active(scope):
            score = lexical_score(query, event.content)
            if score <= 0:
                continue
            record = _event_record(event, self._event_providers(event.event_id))
            record["score"] = min(score, 1.0)
            scored.append(record)
        return sorted(scored, key=lambda item: item["score"], reverse=True)[:limit]

    def _validated_hits(self, record: dict[str, Any], provider: str,
                        scope: dict[str, str]) -> list[dict[str, Any]]:
        identifiers = record.get('canonicalEventIds', [])
        if not identifiers:
            identifier = record.get('metadata', {}).get('canonicalEventId')
            identifiers = [identifier] if identifier else []
        if not identifiers:
            event = self.store.resolve_provider_record(provider, record['id'], scope)
            identifiers = [event.event_id] if event else []
        hits = []
        for identifier in identifiers:
            try:
                event = self.store.get(identifier, scope)
            except ValueError:
                continue
            if event.status != 'approved':
                continue
            revision = record.get('metadata', {}).get('revision')
            if revision is not None and revision != event.revision:
                continue
            if record.get('canonicalEventIds'):
                if not self.store.provider_index_current(identifier, provider):
                    continue
            elif record.get('content') != event.content:
                continue
            hit = _event_record(event, self._event_providers(identifier))
            hit['sources'] = [provider]
            hits.append(hit)
        return hits

    def _import_legacy(self, record: dict[str, Any], provider: str, scope: dict[str, str]) -> None:
        # Never resurrect a deleted canonical record as an untracked legacy hit.
        if record.get('canonicalEventIds') or record.get('metadata', {}).get('canonicalEventId'):
            return
        if self.store.resolve_provider_record(provider, record['id'], scope):
            return
        receipt = 'legacy:' + provider + ':' + record['id']
        decision = MemoryPolicy(minimum_importance=0, require_confirmation=True).review(
            MemoryCandidate(content=record['content'], metadata={'source': provider}), scope, (),
        )
        if decision.event is None:
            return
        identifier = str(uuid.uuid5(uuid.NAMESPACE_URL, '\x1f'.join([scope['userId'], scope['characterId'], receipt])))
        if self.store.turn_processed(scope, receipt):
            try:
                previous = self.store.get(identifier, scope)
            except ValueError:
                return
            if previous.status not in {'pending', 'approved'}:
                return
        event = replace(decision.event, event_id=identifier)
        self.store.apply_turn(scope, receipt, [event], (), deduplicate=False)
        self.store.link_provider_record(identifier, provider, record['id'])

    def _resolve_legacy_id(self, legacy: tuple[str, str], scope: dict[str, str]) -> str:
        name, identifier = legacy
        event = self.store.resolve_provider_record(name, identifier, scope)
        if event:
            return event.event_id
        provider = self.providers.get(name)
        if provider:
            for record in provider.list(scope):
                if record['id'] == identifier:
                    self._import_legacy(record, name, scope)
                    event = self.store.resolve_provider_record(name, identifier, scope)
                    if event:
                        return event.event_id
        raise ValueError('legacy memory is unavailable in this scope')

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
    updated = datetime.fromtimestamp((event.updated_at_unix_ms or event.created_at_unix_ms) / 1000, tz=UTC).isoformat()
    return {
        "id": event.event_id,
        "revision": event.revision,
        "content": event.content,
        "score": event.importance,
        "status": event.status,
        "kind": event.kind,
        "importance": event.importance,
        "sources": ["sqlite"],
        "providers": providers,
        "metadata": event.metadata,
        "createdAt": timestamp,
        "updatedAt": updated,
    }


def _merge_rankings(rankings: list[list[dict[str, Any]]], limit: int) -> list[dict[str, Any]]:
    merged: dict[str, dict[str, Any]] = {}
    for ranking in rankings:
        seen = set()
        for rank, record in enumerate(ranking, 1):
            key, content = record['id'], record['content']
            if key in seen or not content or len(content) > 2000 or contains_sensitive(content):
                continue
            seen.add(key)
            current = merged.setdefault(key, {**record, 'score': 0.0, 'sources': []})
            current['score'] += 1.0 / (60 + rank)
            current['sources'] = list(dict.fromkeys([*current['sources'], *record['sources']]))
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
    parts = memory_id.split(':', 2)
    if len(parts) == 3 and parts[0] == 'legacy' and parts[1] and parts[2]:
        return parts[1], parts[2]
    return None
