from __future__ import annotations

import hashlib
import math
import re
import time
import uuid
from dataclasses import dataclass, field
from typing import Any, Literal

from .memory import contains_sensitive


MemoryKind = Literal["semantic", "episodic"]
MemoryStatus = Literal["pending", "approved", "rejected", "deleted"]


@dataclass(frozen=True)
class MemoryCandidate:
    content: str
    kind: MemoryKind = "semantic"
    importance: float = 0.5
    tags: tuple[str, ...] = ()
    occurred_at_unix_ms: int | None = None
    metadata: dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class ApprovedMemoryEvent:
    event_id: str
    scope: dict[str, str]
    content: str
    content_hash: str
    kind: MemoryKind
    importance: float
    tags: tuple[str, ...]
    occurred_at_unix_ms: int | None
    created_at_unix_ms: int
    source_event_ids: tuple[str, ...]
    metadata: dict[str, Any]
    status: MemoryStatus = "approved"
    revision: int = 1
    updated_at_unix_ms: int | None = None


@dataclass(frozen=True)
class PolicyDecision:
    status: Literal["approved", "pending", "rejected"]
    reason: str
    event: ApprovedMemoryEvent | None


class MemoryPolicy:
    def __init__(self, *, minimum_importance: float, require_confirmation: bool) -> None:
        self.minimum_importance = max(0.0, min(float(minimum_importance), 1.0))
        self.require_confirmation = require_confirmation

    def review(
        self,
        candidate: MemoryCandidate,
        scope: dict[str, str],
        source_event_ids: tuple[str, ...],
        *, messages: list[dict[str, str]] | None = None,
    ) -> PolicyDecision:
        if messages is not None and not valid_user_evidence(candidate.metadata, messages):
            return PolicyDecision('rejected', 'unsupported_user_fact', None)
        content = _normalize_content(candidate.content)
        if not content:
            return PolicyDecision("rejected", "empty", None)
        if len(content) > 2_000:
            return PolicyDecision("rejected", "too_long", None)
        if contains_sensitive(content):
            return PolicyDecision("rejected", "sensitive", None)

        importance = float(candidate.importance)
        if not math.isfinite(importance):
            return PolicyDecision("rejected", "invalid_importance", None)
        importance = max(0.0, min(importance, 1.0))
        if importance < self.minimum_importance:
            return PolicyDecision("rejected", "below_importance_threshold", None)

        now = int(time.time() * 1_000)
        occurred_at = candidate.occurred_at_unix_ms
        if occurred_at is not None and (occurred_at < 0 or occurred_at > now + 86_400_000):
            occurred_at = None
        status: MemoryStatus = "pending" if self.require_confirmation else "approved"
        event = ApprovedMemoryEvent(
            event_id=str(uuid.uuid4()),
            scope=dict(scope),
            content=content,
            content_hash=memory_content_hash(content),
            kind=candidate.kind if candidate.kind in {"semantic", "episodic"} else "semantic",
            importance=importance,
            tags=_normalize_tags(candidate.tags),
            occurred_at_unix_ms=occurred_at,
            created_at_unix_ms=now,
            source_event_ids=_normalize_source_ids(source_event_ids),
            metadata=_safe_metadata(candidate.metadata),
            status=status,
        )
        reason = "awaiting_confirmation" if status == "pending" else "approved"
        return PolicyDecision(status, reason, event)

    def validate_edit(self, content: str) -> tuple[str, str]:
        normalized = _normalize_content(content)
        if not normalized or len(normalized) > 2_000:
            raise ValueError("memory content must contain 1 to 2000 characters")
        if contains_sensitive(normalized):
            raise ValueError("memory content is sensitive")
        return normalized, memory_content_hash(normalized)


def memory_content_hash(content: str) -> str:
    normalized = _normalize_content(content).casefold().encode("utf-8")
    return hashlib.sha256(normalized).hexdigest()


def valid_user_evidence(metadata: dict[str, Any], messages: list[dict[str, str]]) -> bool:
    evidence, confidence = metadata.get('evidence'), metadata.get('confidence')
    return bool(
        metadata.get('sourceRole') == 'user'
        and isinstance(evidence, str) and evidence.strip() and len(evidence) <= 1000
        and any(evidence in message.get('content', '') for message in messages if message.get('role') == 'user')
        and not isinstance(confidence, bool) and isinstance(confidence, (int, float))
        and math.isfinite(confidence) and 0.7 <= confidence <= 1.0
    )


def _normalize_content(content: str) -> str:
    return re.sub(r"\s+", " ", str(content)).strip()


def _normalize_tags(tags: tuple[str, ...]) -> tuple[str, ...]:
    result: list[str] = []
    for value in tags:
        tag = _normalize_content(value)[:64]
        if tag and not contains_sensitive(tag) and tag not in result:
            result.append(tag)
        if len(result) == 12:
            break
    return tuple(result)


def _safe_metadata(metadata: dict[str, Any]) -> dict[str, Any]:
    allowed: dict[str, Any] = {}
    for key in ("source", "scene", "subject", "evidence", "sourceRole"):
        value = metadata.get(key)
        if isinstance(value, str) and value.strip():
            cleaned = _normalize_content(value)[:1000 if key == "evidence" else 128]
            if not contains_sensitive(cleaned):
                allowed[key] = cleaned
    confidence = metadata.get("confidence")
    if isinstance(confidence, (int, float)) and math.isfinite(confidence):
        allowed["confidence"] = max(0.0, min(1.0, confidence))
    supersedes = metadata.get("supersedes")
    if isinstance(supersedes, list):
        allowed["supersedes"] = [value for value in supersedes[:8]
                                 if isinstance(value, str) and len(value) <= 128]
    return allowed


def _normalize_source_ids(values: tuple[str, ...]) -> tuple[str, ...]:
    result: list[str] = []
    for value in values[:16]:
        cleaned = _normalize_content(value)[:128]
        if cleaned and not contains_sensitive(cleaned):
            result.append(cleaned)
    return tuple(result)
