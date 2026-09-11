from __future__ import annotations

import threading
from typing import Any

from .config import SidecarConfig
from .llm import LocalLlmProvider
from .memory import MemoryPort
from .session_store import ConversationSessionStore, validate_scope


EMOTIONS = {"neutral", "happy", "concerned", "curious", "surprised", "sad"}


class ConversationOrchestrator:
    def __init__(
        self,
        config: SidecarConfig,
        sessions: ConversationSessionStore,
        llm: LocalLlmProvider,
        memory: MemoryPort,
    ) -> None:
        self.config = config
        self.sessions = sessions
        self.llm = llm
        self.memory = memory
        self._turn_locks = [threading.Lock() for _ in range(64)]

    def converse(self, request: dict[str, Any]) -> dict[str, Any]:
        turn_id = _bounded_string(request.get("turnId"), "turnId", 128)
        user_input = _bounded_string(request.get("userInput"), "userInput", 16_000)
        scope = validate_scope(request.get("scope"))
        available_actions = _available_actions(request.get("availableActions", []))
        desktop_context = request.get("desktopContext")
        if desktop_context is not None and not isinstance(desktop_context, dict):
            raise ValueError("desktopContext must be an object")

        with self._turn_lock(turn_id):
            cached = self.sessions.cached_response(turn_id, scope, user_input)
            if cached is not None:
                return cached

            recent = self.sessions.recent(scope, 12)
            degraded: list[str] = []
            memories: list[dict[str, Any]] = []
            if self.config.memory_enabled and self.config.recall_enabled:
                try:
                    memories = self.memory.search(user_input, scope, self.config.recall_limit)
                except Exception as error:
                    degraded.append(_safe_failure("memory recall", error))

            raw = self.llm.complete(
                user_input=user_input,
                recent_messages=recent,
                memories=memories,
                available_actions=available_actions,
                desktop_context=desktop_context,
            )
            response = validate_character_response(raw, turn_id, available_actions)
            if degraded:
                response["degradedReasons"] = degraded
            response = self.sessions.commit(
                turn_id,
                scope,
                user_input,
                response["text"],
                response,
                self.config.memory_enabled and self.config.memory_write_enabled,
            )
            # The reply and pending memory write are committed together. Only the
            # MemoryManager worker extracts memories, including after a restart.
            return response

    def _turn_lock(self, turn_id: str) -> threading.Lock:
        return self._turn_locks[hash(turn_id) % len(self._turn_locks)]


def validate_character_response(
    value: dict[str, Any],
    turn_id: str,
    available_actions: list[dict[str, Any]],
) -> dict[str, Any]:
    text = _bounded_string(value.get("text"), "response text", 8_000)
    response: dict[str, Any] = {"turnId": turn_id, "text": text}

    emotion = value.get("emotion")
    if isinstance(emotion, dict):
        kind = emotion.get("type")
        if kind not in EMOTIONS:
            kind = "neutral"
        response["emotion"] = {
            "type": kind,
            "intensity": _clamp(emotion.get("intensity"), 0.5),
            "durationMs": int(_clamp(emotion.get("durationMs"), 2200, 250, 10_000)),
        }

    intent = value.get("actionIntent")
    allowed_ids = {action["id"] for action in available_actions}
    if isinstance(intent, dict) and intent.get("id") in allowed_ids:
        response["actionIntent"] = {
            "id": intent["id"],
            "intensity": _clamp(intent.get("intensity"), 0.5),
        }

    speech = value.get("speech")
    speech_text = speech.get("text") if isinstance(speech, dict) else text
    if isinstance(speech_text, str) and speech_text.strip():
        response["speech"] = {"text": speech_text.strip()[:8_000]}
    return response


def _available_actions(value: Any) -> list[dict[str, Any]]:
    if not isinstance(value, list) or len(value) > 32:
        raise ValueError("availableActions must be an array with at most 32 items")
    actions: list[dict[str, Any]] = []
    seen: set[str] = set()
    for item in value:
        if not isinstance(item, dict):
            raise ValueError("availableActions contains an invalid item")
        identifier = _bounded_string(item.get("id"), "action id", 128)
        if identifier in seen:
            continue
        seen.add(identifier)
        scenes = item.get("scenes", [])
        actions.append(
            {
                "id": identifier,
                "description": str(item.get("description", ""))[:500],
                "scenes": [str(scene)[:128] for scene in scenes[:16]]
                if isinstance(scenes, list)
                else [],
            }
        )
    return actions


def _bounded_string(value: Any, name: str, maximum: int) -> str:
    if not isinstance(value, str) or not value.strip() or len(value) > maximum:
        raise ValueError(f"{name} must contain 1 to {maximum} characters")
    return value.strip()


def _clamp(value: Any, fallback: float, minimum: float = 0.0, maximum: float = 1.0) -> float:
    try:
        number = float(value)
    except (TypeError, ValueError):
        number = fallback
    if number != number or number in (float("inf"), float("-inf")):
        number = fallback
    return max(minimum, min(number, maximum))


def _safe_failure(operation: str, error: Exception) -> str:
    return f"{operation} unavailable ({type(error).__name__})"
