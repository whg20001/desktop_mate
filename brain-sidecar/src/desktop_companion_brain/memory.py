from __future__ import annotations

import os
import threading
from typing import TYPE_CHECKING, Any, Protocol

from .config import SidecarConfig
from .openai_client import create_local_openai_client

if TYPE_CHECKING:
    from .memory_policy import ApprovedMemoryEvent
    from .session_store import ConversationSessionStore


SENSITIVE_MARKERS = (
    "password",
    "passphrase",
    "api key",
    "api_key",
    "access token",
    "refresh token",
    "private key",
    "credit card",
    "cvv",
    "密码",
    "口令",
    "验证码",
    "访问令牌",
    "私钥",
    "信用卡",
    "银行卡号",
    "支付密码",
)


class MemoryPort(Protocol):
    def ready(self) -> tuple[bool, str]: ...
    def close(self) -> None: ...
    def search(self, query: str, scope: dict[str, str], limit: int) -> list[dict[str, Any]]: ...
    def remember_turn(self, messages: list[dict[str, str]], scope: dict[str, str], turn_id: str) -> None: ...
    def list(self, scope: dict[str, str]) -> list[dict[str, Any]]: ...
    def update(self, memory_id: str, content: str, scope: dict[str, str]) -> None: ...
    def delete(self, memory_id: str, scope: dict[str, str]) -> None: ...
    def approve(self, memory_id: str, scope: dict[str, str]) -> None: ...
    def reject(self, memory_id: str, scope: dict[str, str]) -> None: ...
    def rebuild(self, provider: str, scope: dict[str, str] | None = None) -> int: ...
    def status(self) -> dict[str, Any]: ...


class LongTermMemoryProvider(Protocol):
    provider_id: str

    def ready(self) -> tuple[bool, str]: ...
    def close(self) -> None: ...
    def search(self, query: str, scope: dict[str, str], limit: int) -> list[dict[str, Any]]: ...
    def list(self, scope: dict[str, str]) -> list[dict[str, Any]]: ...
    def upsert(self, event: ApprovedMemoryEvent, provider_record_id: str | None) -> str: ...
    def delete_record(self, provider_record_id: str, scope: dict[str, str]) -> None: ...


class DisabledMemory:
    def ready(self) -> tuple[bool, str]:
        return True, "long-term memory is disabled"

    def search(self, query: str, scope: dict[str, str], limit: int) -> list[dict[str, Any]]:
        return []

    def remember_turn(self, messages: list[dict[str, str]], scope: dict[str, str], turn_id: str) -> None:
        return None

    def list(self, scope: dict[str, str]) -> list[dict[str, Any]]:
        return []

    def update(self, memory_id: str, content: str, scope: dict[str, str]) -> None:
        raise RuntimeError("long-term memory is disabled")

    def delete(self, memory_id: str, scope: dict[str, str]) -> None:
        raise RuntimeError("long-term memory is disabled")

    def approve(self, memory_id: str, scope: dict[str, str]) -> None:
        raise RuntimeError("long-term memory is disabled")

    def reject(self, memory_id: str, scope: dict[str, str]) -> None:
        raise RuntimeError("long-term memory is disabled")

    def rebuild(self, provider: str, scope: dict[str, str] | None = None) -> int:
        raise RuntimeError("long-term memory is disabled")

    def close(self) -> None:
        return None

    def status(self) -> dict[str, Any]:
        return {"enabled": False, "ready": True, "providers": []}


class Mem0Memory:
    provider_id = "mem0"

    def __init__(self, config: SidecarConfig) -> None:
        self._startup_error: str | None = None
        self._memory: Any | None = None
        self._lock = threading.RLock()
        try:
            for name in (
                "OPENROUTER_API_KEY",
                "OPENROUTER_API_BASE",
                "MEM0_API_KEY",
            ):
                os.environ.pop(name, None)
            os.environ["MEM0_TELEMETRY"] = "false"
            os.environ["ANONYMIZED_TELEMETRY"] = "false"
            os.environ["POSTHOG_DISABLED"] = "true"
            os.environ["OPENAI_API_KEY"] = "local-only"
            os.environ["OPENAI_BASE_URL"] = config.llm_base_url
            from mem0 import Memory
            from mem0.memory import main as mem0_main
            from mem0.memory import telemetry as mem0_telemetry

            mem0_main.MEM0_TELEMETRY = False
            mem0_telemetry.MEM0_TELEMETRY = False
            telemetry_client = getattr(mem0_telemetry, "client_telemetry", None)
            telemetry_close = getattr(telemetry_client, "close", None)
            if callable(telemetry_close):
                telemetry_close()

            self._memory = Memory.from_config(config.mem0_config())
            _replace_openai_client(
                self._memory.llm,
                create_local_openai_client(
                    config.llm_base_url,
                    config.request_timeout_seconds,
                ),
            )
            _replace_openai_client(
                self._memory.embedding_model,
                create_local_openai_client(
                    config.embedding_base_url,
                    config.request_timeout_seconds,
                ),
            )
        except Exception as error:
            self._startup_error = f"Mem0 initialization failed: {type(error).__name__}"
            if self._memory is not None:
                self.close()

    def ready(self) -> tuple[bool, str]:
        if self._memory is None:
            return False, self._startup_error or "Mem0 is unavailable"
        return True, "Mem0 uses local LLM, embedding, SQLite history and embedded Qdrant"

    def search(self, query: str, scope: dict[str, str], limit: int) -> list[dict[str, Any]]:
        with self._lock:
            result = self._require().search(query=query, limit=limit, **_filters(scope))
        values = result.get("results", result) if isinstance(result, dict) else result
        if not isinstance(values, list):
            raise RuntimeError("Mem0 search response is invalid")
        return [_normalize_record(value) for value in values if isinstance(value, dict)][:limit]

    def list(self, scope: dict[str, str]) -> list[dict[str, Any]]:
        with self._lock:
            result = self._require().get_all(**_filters(scope))
        values = result.get("results", result) if isinstance(result, dict) else result
        if not isinstance(values, list):
            raise RuntimeError("Mem0 list response is invalid")
        return [_normalize_record(value) for value in values if isinstance(value, dict)]

    def update(self, memory_id: str, content: str, scope: dict[str, str]) -> None:
        if not content.strip() or contains_sensitive(content):
            raise ValueError("memory content is empty or sensitive")
        with self._lock:
            self._require_scope(memory_id, scope)
            self._require().update(memory_id=memory_id, data=content.strip())

    def delete(self, memory_id: str, scope: dict[str, str]) -> None:
        with self._lock:
            self._require_scope(memory_id, scope)
            self._require().delete(memory_id=memory_id)

    def upsert(
        self,
        event: ApprovedMemoryEvent,
        provider_record_id: str | None,
    ) -> str:
        with self._lock:
            record_id = provider_record_id or self._record_for_event(event)
            if record_id:
                self._require_scope(record_id, event.scope)
                self._require().update(memory_id=record_id, data=event.content)
                return record_id
            result = self._require().add(
                event.content,
                infer=False,
                metadata={
                    "canonicalEventId": event.event_id,
                    "kind": event.kind,
                    "importance": event.importance,
                    "revision": event.revision,
                    "originSessionId": event.scope["sessionId"],
                    "localOnly": True,
                },
                **_filters(event.scope),
            )
        values = result.get("results", result) if isinstance(result, dict) else result
        if isinstance(values, list):
            for value in values:
                if isinstance(value, dict):
                    try:
                        return _normalize_record(value)["id"]
                    except RuntimeError:
                        continue
        record_id = self._record_for_event(event)
        if record_id:
            return record_id
        raise RuntimeError("Mem0 did not return the created memory id")

    def delete_record(self, provider_record_id: str, scope: dict[str, str]) -> None:
        record_id = provider_record_id
        records = self.list(scope)
        if not any(record["id"] == record_id for record in records):
            record_id = next(
                (
                    record["id"]
                    for record in records
                    if record.get("metadata", {}).get("canonicalEventId")
                    == provider_record_id
                ),
                "",
            )
        if record_id:
            self.delete(record_id, scope)

    def close(self) -> None:
        if self._memory is None:
            return
        with self._lock:
            for component_name in ("llm", "embedding_model", "vector_store"):
                component = getattr(self._memory, component_name, None)
                client = getattr(component, "client", None)
                close = getattr(client, "close", None)
                if callable(close):
                    close()
            close_memory = getattr(self._memory, "close", None)
            if callable(close_memory):
                close_memory()
            self._memory = None

    def _require(self):
        if self._memory is None:
            raise RuntimeError(self._startup_error or "Mem0 is unavailable")
        return self._memory

    def _require_scope(self, memory_id: str, scope: dict[str, str]) -> None:
        if not any(record["id"] == memory_id for record in self.list(scope)):
            raise ValueError("memory does not belong to the requested scope")

    def _record_for_event(self, event: ApprovedMemoryEvent) -> str | None:
        for record in self.list(event.scope):
            metadata = record.get("metadata", {})
            if metadata.get("canonicalEventId") == event.event_id:
                return record["id"]
        return None


def create_memory(
    config: SidecarConfig,
    sessions: ConversationSessionStore | None = None,
) -> MemoryPort:
    if not config.memory_enabled:
        return DisabledMemory()
    from .memory_manager import MemoryManager

    return MemoryManager(config, sessions)


def contains_sensitive(content: str) -> bool:
    lowered = content.casefold()
    return any(marker in lowered for marker in SENSITIVE_MARKERS)


def _replace_openai_client(component: Any, client: Any) -> None:
    previous = getattr(component, "client", None)
    if previous is None:
        client.close()
        raise RuntimeError("Mem0 local OpenAI-compatible provider is unavailable")
    component.client = client
    close = getattr(previous, "close", None)
    if callable(close):
        close()


def _filters(scope: dict[str, str]) -> dict[str, str]:
    return {
        "user_id": scope["userId"],
        "agent_id": scope["characterId"],
    }


def _normalize_record(value: dict[str, Any]) -> dict[str, Any]:
    memory_id = value.get("id") or value.get("memory_id")
    content = value.get("memory") or value.get("text") or value.get("content")
    if not isinstance(memory_id, str) or not isinstance(content, str):
        raise RuntimeError("Mem0 record is missing its id or content")
    metadata = value.get("metadata") if isinstance(value.get("metadata"), dict) else {}
    score = value.get("score", 1.0)
    try:
        score = float(score)
    except (TypeError, ValueError):
        score = 0.0
    if not 0.0 <= score <= 1.0:
        score = 0.0
    return {
        "id": memory_id,
        "content": content,
        "score": score,
        "metadata": metadata,
        "createdAt": value.get("created_at") or value.get("createdAt"),
        "updatedAt": value.get("updated_at") or value.get("updatedAt"),
    }
