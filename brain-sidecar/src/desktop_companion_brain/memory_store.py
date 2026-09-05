from __future__ import annotations

import json
import sqlite3
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

from .memory_policy import ApprovedMemoryEvent


@dataclass(frozen=True)
class ProviderDelivery:
    event: ApprovedMemoryEvent
    provider: str
    operation: str
    provider_record_id: str | None
    attempts: int


class MemoryEventStore:
    """Canonical local facts plus an idempotent outbox for derived indexes."""

    def __init__(self, path: Path) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        self._lock = threading.RLock()
        self._connection = sqlite3.connect(path, check_same_thread=False)
        self._connection.row_factory = sqlite3.Row
        self._connection.execute("PRAGMA journal_mode = WAL")
        self._connection.execute("PRAGMA foreign_keys = ON")
        self._connection.executescript(
            """
            CREATE TABLE IF NOT EXISTS memory_events (
              event_id TEXT PRIMARY KEY,
              user_id TEXT NOT NULL,
              character_id TEXT NOT NULL,
              origin_session_id TEXT NOT NULL,
              content TEXT NOT NULL,
              content_hash TEXT NOT NULL,
              kind TEXT NOT NULL CHECK(kind IN ('semantic', 'episodic')),
              importance REAL NOT NULL,
              tags_json TEXT NOT NULL,
              occurred_at_unix_ms INTEGER,
              created_at_unix_ms INTEGER NOT NULL,
              updated_at_unix_ms INTEGER NOT NULL,
              source_event_ids_json TEXT NOT NULL,
              metadata_json TEXT NOT NULL,
              status TEXT NOT NULL CHECK(status IN ('pending', 'approved', 'rejected', 'deleted')),
              revision INTEGER NOT NULL DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_memory_events_scope_status
              ON memory_events(user_id, character_id, status, updated_at_unix_ms DESC);
            CREATE INDEX IF NOT EXISTS idx_memory_events_source
              ON memory_events(user_id, character_id, content_hash);

            CREATE TABLE IF NOT EXISTS memory_deliveries (
              event_id TEXT NOT NULL REFERENCES memory_events(event_id) ON DELETE CASCADE,
              provider TEXT NOT NULL,
              operation TEXT NOT NULL CHECK(operation IN ('upsert', 'delete')),
              status TEXT NOT NULL CHECK(status IN ('pending', 'complete')),
              attempts INTEGER NOT NULL DEFAULT 0,
              next_attempt_unix_ms INTEGER NOT NULL DEFAULT 0,
              provider_record_id TEXT,
              last_error_type TEXT,
              updated_at_unix_ms INTEGER NOT NULL,
              PRIMARY KEY(event_id, provider)
            );
            CREATE INDEX IF NOT EXISTS idx_memory_delivery_outbox
              ON memory_deliveries(status, next_attempt_unix_ms, updated_at_unix_ms);

            """
        )
        self._connection.commit()

    def close(self) -> None:
        with self._lock:
            self._connection.close()

    def add(self, event: ApprovedMemoryEvent, providers: Iterable[str]) -> ApprovedMemoryEvent:
        with self._lock:
            existing = self._connection.execute(
                """SELECT * FROM memory_events
                   WHERE user_id = ?1 AND character_id = ?2 AND content_hash = ?3
                     AND kind = ?4
                     AND status IN ('pending', 'approved')
                   ORDER BY created_at_unix_ms DESC LIMIT 1""",
                (
                    event.scope["userId"],
                    event.scope["characterId"],
                    event.content_hash,
                    event.kind,
                ),
            ).fetchone()
            if existing is not None:
                if existing["status"] == "approved":
                    now = _now()
                    for provider in providers:
                        self._connection.execute(
                            """INSERT OR IGNORE INTO memory_deliveries(
                                 event_id, provider, operation, status, attempts,
                                 next_attempt_unix_ms, updated_at_unix_ms
                               ) VALUES (?1, ?2, 'upsert', 'pending', 0, 0, ?3)""",
                            (existing["event_id"], provider, now),
                        )
                    self._connection.commit()
                return _event_from_row(existing)
            try:
                self._connection.execute(
                    """INSERT INTO memory_events(
                         event_id, user_id, character_id, origin_session_id, content,
                         content_hash, kind, importance, tags_json, occurred_at_unix_ms,
                         created_at_unix_ms, updated_at_unix_ms, source_event_ids_json,
                         metadata_json, status, revision
                       ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                                 ?11, ?12, ?13, ?14, ?15, ?16)""",
                    _event_values(event),
                )
                if event.status == "approved":
                    self._queue(
                        event.event_id,
                        providers,
                        "upsert",
                        event.created_at_unix_ms,
                    )
                self._connection.commit()
            except Exception:
                self._connection.rollback()
                raise
        return event

    def list(self, scope: dict[str, str]) -> list[ApprovedMemoryEvent]:
        with self._lock:
            rows = self._connection.execute(
                """SELECT * FROM memory_events
                   WHERE user_id = ?1 AND character_id = ?2
                     AND status IN ('pending', 'approved')
                   ORDER BY updated_at_unix_ms DESC""",
                _scope(scope),
            ).fetchall()
        return [_event_from_row(row) for row in rows]

    def active(self, scope: dict[str, str] | None = None) -> list[ApprovedMemoryEvent]:
        with self._lock:
            if scope is None:
                rows = self._connection.execute(
                    "SELECT * FROM memory_events WHERE status = 'approved' ORDER BY created_at_unix_ms"
                ).fetchall()
            else:
                rows = self._connection.execute(
                    """SELECT * FROM memory_events
                       WHERE user_id = ?1 AND character_id = ?2 AND status = 'approved'
                       ORDER BY created_at_unix_ms""",
                    _scope(scope),
                ).fetchall()
        return [_event_from_row(row) for row in rows]

    def get(self, event_id: str, scope: dict[str, str]) -> ApprovedMemoryEvent:
        with self._lock:
            row = self._connection.execute(
                """SELECT * FROM memory_events
                   WHERE event_id = ?1 AND user_id = ?2 AND character_id = ?3""",
                (event_id, *_scope(scope)),
            ).fetchone()
        if row is None:
            raise ValueError("memory does not belong to the requested scope")
        return _event_from_row(row)

    def approve(self, event_id: str, scope: dict[str, str], providers: Iterable[str]) -> None:
        event = self.get(event_id, scope)
        if event.status != "pending":
            raise ValueError("only pending memories can be approved")
        now = _now()
        with self._lock:
            self._connection.execute(
                """UPDATE memory_events SET status = 'approved', revision = revision + 1,
                   updated_at_unix_ms = ?1 WHERE event_id = ?2""",
                (now, event_id),
            )
            self._queue(event_id, providers, "upsert", now)
            self._connection.commit()

    def reject(self, event_id: str, scope: dict[str, str]) -> None:
        event = self.get(event_id, scope)
        if event.status != "pending":
            raise ValueError("only pending memories can be rejected")
        with self._lock:
            self._connection.execute(
                """UPDATE memory_events SET status = 'rejected', content = '',
                   tags_json = '[]', source_event_ids_json = '[]', metadata_json = '{}',
                   revision = revision + 1,
                   updated_at_unix_ms = ?1 WHERE event_id = ?2""",
                (_now(), event_id),
            )
            self._connection.commit()

    def update(
        self,
        event_id: str,
        scope: dict[str, str],
        content: str,
        content_hash: str,
        providers: Iterable[str],
    ) -> None:
        event = self.get(event_id, scope)
        if event.status not in {"pending", "approved"}:
            raise ValueError("memory cannot be updated")
        now = _now()
        with self._lock:
            targets = self._delivery_providers(event_id, providers)
            self._connection.execute(
                """UPDATE memory_events
                   SET content = ?1, content_hash = ?2, revision = revision + 1,
                       updated_at_unix_ms = ?3
                   WHERE event_id = ?4""",
                (content, content_hash, now, event_id),
            )
            if event.status == "approved":
                self._queue(event_id, targets, "upsert", now)
            self._connection.commit()

    def delete(
        self,
        event_id: str,
        scope: dict[str, str],
        providers: Iterable[str],
    ) -> None:
        event = self.get(event_id, scope)
        if event.status == "deleted":
            return
        now = _now()
        with self._lock:
            targets = self._delivery_providers(event_id, providers)
            self._connection.execute(
                """UPDATE memory_events
                   SET status = 'deleted', content = '', tags_json = '[]',
                       source_event_ids_json = '[]', metadata_json = '{}',
                       revision = revision + 1,
                       updated_at_unix_ms = ?1 WHERE event_id = ?2""",
                (now, event_id),
            )
            self._queue(event_id, targets, "delete", now)
            self._connection.commit()

    def pending_deliveries(
        self,
        limit: int = 32,
        providers: Iterable[str] | None = None,
    ) -> list[ProviderDelivery]:
        provider_values = (
            tuple(dict.fromkeys(providers))
            if providers is not None
            else None
        )
        if provider_values == ():
            return []
        provider_clause = ""
        parameters: list[Any] = [_now()]
        if provider_values is not None:
            placeholders = ", ".join("?" for _ in provider_values)
            provider_clause = f" AND d.provider IN ({placeholders})"
            parameters.extend(provider_values)
        parameters.append(max(1, min(limit, 100)))
        with self._lock:
            rows = self._connection.execute(
                """SELECT e.*, d.provider, d.operation, d.provider_record_id, d.attempts
                   FROM memory_deliveries d
                   JOIN memory_events e ON e.event_id = d.event_id
                   WHERE d.status = 'pending' AND d.next_attempt_unix_ms <= ?"""
                + provider_clause
                + " ORDER BY d.updated_at_unix_ms LIMIT ?",
                parameters,
            ).fetchall()
        return [
            ProviderDelivery(
                event=_event_from_row(row),
                provider=row["provider"],
                operation=row["operation"],
                provider_record_id=row["provider_record_id"],
                attempts=int(row["attempts"]),
            )
            for row in rows
        ]

    def complete_delivery(
        self,
        delivery: ProviderDelivery,
        provider_record_id: str | None,
    ) -> bool:
        with self._lock:
            if delivery.operation == "delete":
                cursor = self._connection.execute(
                    """DELETE FROM memory_deliveries
                       WHERE event_id = ?1 AND provider = ?2 AND operation = 'delete'
                         AND EXISTS (
                           SELECT 1 FROM memory_events e WHERE e.event_id = ?1
                             AND e.revision = ?3 AND e.status = 'deleted'
                         )""",
                    (delivery.event.event_id, delivery.provider, delivery.event.revision),
                )
            else:
                cursor = self._connection.execute(
                    """UPDATE memory_deliveries SET status = 'complete', attempts = 0,
                       provider_record_id = ?1, last_error_type = NULL,
                       updated_at_unix_ms = ?2
                       WHERE event_id = ?3 AND provider = ?4 AND operation = 'upsert'
                         AND EXISTS (
                           SELECT 1 FROM memory_events e WHERE e.event_id = ?3
                             AND e.revision = ?5 AND e.status = 'approved'
                         )""",
                    (
                        provider_record_id,
                        _now(),
                        delivery.event.event_id,
                        delivery.provider,
                        delivery.event.revision,
                    ),
                )
            self._connection.commit()
            return cursor.rowcount == 1

    def fail_delivery(self, delivery: ProviderDelivery, error: Exception) -> bool:
        attempts = delivery.attempts + 1
        delay_ms = min(300_000, 1_000 * (2 ** min(attempts, 8)))
        with self._lock:
            cursor = self._connection.execute(
                """UPDATE memory_deliveries
                   SET attempts = ?1, next_attempt_unix_ms = ?2,
                       last_error_type = ?3, updated_at_unix_ms = ?4
                   WHERE event_id = ?5 AND provider = ?6 AND operation = ?7
                     AND EXISTS (
                       SELECT 1 FROM memory_events e WHERE e.event_id = ?5
                         AND e.revision = ?8
                     )""",
                (
                    attempts,
                    _now() + delay_ms,
                    type(error).__name__,
                    _now(),
                    delivery.event.event_id,
                    delivery.provider,
                    delivery.operation,
                    delivery.event.revision,
                ),
            )
            self._connection.commit()
            return cursor.rowcount == 1

    def queue_rebuild(self, provider: str, scope: dict[str, str] | None = None) -> int:
        events = self.active(scope)
        now = _now()
        with self._lock:
            for event in events:
                self._queue(event.event_id, (provider,), "upsert", now)
            self._connection.commit()
        return len(events)

    def provider_record_id(self, event_id: str, provider: str) -> str | None:
        with self._lock:
            row = self._connection.execute(
                """SELECT provider_record_id FROM memory_deliveries
                   WHERE event_id = ?1 AND provider = ?2""",
                (event_id, provider),
            ).fetchone()
        return None if row is None else row[0]

    def stats(self) -> dict[str, Any]:
        with self._lock:
            event_rows = self._connection.execute(
                "SELECT status, COUNT(*) FROM memory_events GROUP BY status"
            ).fetchall()
            delivery_rows = self._connection.execute(
                """SELECT provider, status, COUNT(*) FROM memory_deliveries
                   GROUP BY provider, status"""
            ).fetchall()
        return {
            "events": {row[0]: row[1] for row in event_rows},
            "deliveries": {
                f"{row[0]}:{row[1]}": row[2] for row in delivery_rows
            },
        }

    def purge_tombstones(self, retention_days: int) -> int:
        cutoff = _now() - max(1, retention_days) * 86_400_000
        with self._lock:
            cursor = self._connection.execute(
                """DELETE FROM memory_events
                   WHERE status IN ('rejected', 'deleted')
                     AND updated_at_unix_ms < ?1
                     AND NOT EXISTS (
                       SELECT 1 FROM memory_deliveries d
                       WHERE d.event_id = memory_events.event_id
                     )""",
                (cutoff,),
            )
            self._connection.commit()
            return cursor.rowcount

    def _queue(
        self,
        event_id: str,
        providers: Iterable[str],
        operation: str,
        now: int,
    ) -> None:
        for provider in providers:
            self._connection.execute(
                """INSERT INTO memory_deliveries(
                     event_id, provider, operation, status, attempts,
                     next_attempt_unix_ms, updated_at_unix_ms
                   ) VALUES (?1, ?2, ?3, 'pending', 0, 0, ?4)
                   ON CONFLICT(event_id, provider) DO UPDATE SET
                     operation = excluded.operation,
                     status = 'pending', attempts = 0, next_attempt_unix_ms = 0,
                     last_error_type = NULL, updated_at_unix_ms = excluded.updated_at_unix_ms""",
                (event_id, provider, operation, now),
            )

    def _delivery_providers(
        self,
        event_id: str,
        providers: Iterable[str],
    ) -> tuple[str, ...]:
        historical = self._connection.execute(
            "SELECT provider FROM memory_deliveries WHERE event_id = ?",
            (event_id,),
        ).fetchall()
        return tuple(
            dict.fromkeys((*providers, *(str(row[0]) for row in historical)))
        )


def _scope(scope: dict[str, str]) -> tuple[str, str]:
    return scope["userId"], scope["characterId"]


def _now() -> int:
    return int(time.time() * 1_000)


def _event_values(event: ApprovedMemoryEvent) -> tuple[Any, ...]:
    return (
        event.event_id,
        event.scope["userId"],
        event.scope["characterId"],
        event.scope["sessionId"],
        event.content,
        event.content_hash,
        event.kind,
        event.importance,
        json.dumps(event.tags, ensure_ascii=False, separators=(",", ":")),
        event.occurred_at_unix_ms,
        event.created_at_unix_ms,
        event.created_at_unix_ms,
        json.dumps(event.source_event_ids, ensure_ascii=False, separators=(",", ":")),
        json.dumps(event.metadata, ensure_ascii=False, separators=(",", ":")),
        event.status,
        event.revision,
    )


def _event_from_row(row: sqlite3.Row) -> ApprovedMemoryEvent:
    return ApprovedMemoryEvent(
        event_id=row["event_id"],
        scope={
            "userId": row["user_id"],
            "characterId": row["character_id"],
            "sessionId": row["origin_session_id"],
        },
        content=row["content"],
        content_hash=row["content_hash"],
        kind=row["kind"],
        importance=float(row["importance"]),
        tags=tuple(json.loads(row["tags_json"])),
        occurred_at_unix_ms=row["occurred_at_unix_ms"],
        created_at_unix_ms=int(row["created_at_unix_ms"]),
        source_event_ids=tuple(json.loads(row["source_event_ids_json"])),
        metadata=json.loads(row["metadata_json"]),
        status=row["status"],
        revision=int(row["revision"]),
    )
