from __future__ import annotations

import os
import threading
from typing import Any, Protocol

from .config import SidecarConfig
from .openai_client import create_local_openai_client


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
    def add_entry(self, entry: dict[str, Any]) -> str: ...
    def list(self, scope: dict[str, str]) -> list[dict[str, Any]]: ...
    def update(self, memory_id: str, content: str, scope: dict[str, str]) -> None: ...
    def delete(self, memory_id: str, scope: dict[str, str]) -> None: ...
    def clear(self, scope: dict[str, str]) -> None: ...


class DisabledMemory:
    def ready(self) -> tuple[bool, str]:
        return True, "long-term memory is disabled"

    def search(self, query: str, scope: dict[str, str], limit: int) -> list[dict[str, Any]]:
        return []

    def remember_turn(self, messages: list[dict[str, str]], scope: dict[str, str], turn_id: str) -> None:
        return None

    def add_entry(self, entry: dict[str, Any]) -> str:
        raise RuntimeError("long-term memory is disabled")

    def list(self, scope: dict[str, str]) -> list[dict[str, Any]]:
        return []

    def update(self, memory_id: str, content: str, scope: dict[str, str]) -> None:
        raise RuntimeError("long-term memory is disabled")

    def delete(self, memory_id: str, scope: dict[str, str]) -> None:
        raise RuntimeError("long-term memory is disabled")

    def clear(self, scope: dict[str, str]) -> None:
        return None

    def close(self) -> None:
        return None


class Mem0Memory:
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
            mem0_telemetry.client_telemetry.close()

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

    def remember_turn(self, messages: list[dict[str, str]], scope: dict[str, str], turn_id: str) -> None:
        if any(contains_sensitive(message.get("content", "")) for message in messages):
            return
        with self._lock:
            existing = self._require().get_all(**_filters(scope))
            values = existing.get("results", existing) if isinstance(existing, dict) else existing
            if isinstance(values, list) and any(
                isinstance(value, dict)
                and isinstance(value.get("metadata"), dict)
                and value["metadata"].get("turnId") == turn_id
                for value in values
            ):
                return
            self._require().add(
                messages,
                infer=True,
                metadata={
                    "turnId": turn_id,
                    "originSessionId": scope["sessionId"],
                    "scope": {
                        "userId": scope["userId"],
                        "characterId": scope["characterId"],
                        "sessionId": None,
                    },
                    "localOnly": True,
                },
                **_filters(scope),
            )

    def add_entry(self, entry: dict[str, Any]) -> str:
        scope = entry["scope"]
        content = str(entry["content"]).strip()
        if not content or contains_sensitive(content):
            raise ValueError("memory content is empty or sensitive")
        metadata = dict(entry.get("metadata") or {})
        metadata.update(
            {
                "kind": entry.get("kind", "semantic"),
                "importance": entry.get("importance", 0.5),
                "tags": entry.get("tags", []),
                "occurredAtUnixMs": entry.get("occurredAtUnixMs"),
                "createdAtUnixMs": entry.get("createdAtUnixMs"),
                "sourceEventIds": entry.get("sourceEventIds", []),
                "scope": {
                    "userId": scope["userId"],
                    "characterId": scope["characterId"],
                    "sessionId": None,
                },
                "originSessionId": scope.get("sessionId"),
                "localOnly": True,
            }
        )
        with self._lock:
            result = self._require().add(
                content, infer=False, metadata=metadata, **_filters(scope)
            )
        return _result_id(result)

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

    def clear(self, scope: dict[str, str]) -> None:
        with self._lock:
            self._require().delete_all(**_filters(scope))

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


def create_memory(config: SidecarConfig) -> MemoryPort:
    return Mem0Memory(config) if config.memory_enabled else DisabledMemory()


def contains_sensitive(content: str) -> bool:
    lowered = content.lower()
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


def _result_id(result: Any) -> str:
    values = result.get("results", result) if isinstance(result, dict) else result
    if isinstance(values, list) and values:
        value = values[0]
        if isinstance(value, dict) and isinstance(value.get("id"), str):
            return value["id"]
    if isinstance(result, dict) and isinstance(result.get("id"), str):
        return result["id"]
    raise RuntimeError("Mem0 did not return a memory id")


def _normalize_record(value: dict[str, Any]) -> dict[str, Any]:
    memory_id = value.get("id") or value.get("memory_id")
    content = value.get("memory") or value.get("text") or value.get("content")
    if not isinstance(memory_id, str) or not isinstance(content, str):
        raise RuntimeError("Mem0 record is missing its id or content")
    metadata = value.get("metadata") if isinstance(value.get("metadata"), dict) else {}
    score = value.get("score", 1.0)
    return {
        "id": memory_id,
        "content": content,
        "score": max(0.0, min(float(score), 1.0)),
        "metadata": metadata,
        "createdAt": value.get("created_at") or value.get("createdAt"),
        "updatedAt": value.get("updated_at") or value.get("updatedAt"),
    }
