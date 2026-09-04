from __future__ import annotations

import json
import sqlite3
import threading
import time
from pathlib import Path
from typing import Any


class ConversationSessionStore:
    def __init__(self, path: Path) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        self._lock = threading.RLock()
        self._connection = sqlite3.connect(path, check_same_thread=False)
        self._connection.execute("PRAGMA journal_mode = WAL")
        self._connection.execute("PRAGMA foreign_keys = ON")
        self._connection.executescript(
            """
            CREATE TABLE IF NOT EXISTS conversation_turns (
              turn_id TEXT PRIMARY KEY,
              user_id TEXT NOT NULL,
              character_id TEXT NOT NULL,
              session_id TEXT NOT NULL,
              user_content TEXT NOT NULL,
              assistant_content TEXT NOT NULL,
              response_json TEXT NOT NULL,
              memory_write_completed INTEGER NOT NULL DEFAULT 0,
              created_at_unix_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_conversation_scope_time
              ON conversation_turns(user_id, character_id, session_id, created_at_unix_ms);
            """
        )
        self._connection.commit()
        self._ensure_column(
            "conversation_turns",
            "memory_write_completed",
            "INTEGER NOT NULL DEFAULT 0",
        )

    def close(self) -> None:
        with self._lock:
            self._connection.close()

    def cached_response(
        self,
        turn_id: str,
        scope: dict[str, str],
        user_content: str | None = None,
    ) -> dict[str, Any] | None:
        with self._lock:
            row = self._connection.execute(
                """SELECT user_id, character_id, session_id, user_content, response_json
                   FROM conversation_turns WHERE turn_id = ?1""",
                (turn_id,),
            ).fetchone()
        if row is None:
            return None
        if tuple(row[:3]) != _scope_tuple(scope):
            raise ValueError("turnId already belongs to another conversation scope")
        if user_content is not None and row[3] != user_content:
            raise ValueError("turnId already belongs to different user input")
        value = json.loads(row[4])
        if not isinstance(value, dict):
            raise ValueError("cached response is invalid")
        return value

    def recent(self, scope: dict[str, str], limit: int) -> list[dict[str, str]]:
        with self._lock:
            rows = self._connection.execute(
                """SELECT user_content, assistant_content
                   FROM conversation_turns
                   WHERE user_id = ?1 AND character_id = ?2 AND session_id = ?3
                   ORDER BY created_at_unix_ms DESC LIMIT ?4""",
                (*_scope_tuple(scope), max(0, min(limit, 100))),
            ).fetchall()
        messages: list[dict[str, str]] = []
        for user_content, assistant_content in reversed(rows):
            messages.extend(
                [
                    {"role": "user", "content": user_content},
                    {"role": "assistant", "content": assistant_content},
                ]
            )
        return messages

    def commit(
        self,
        turn_id: str,
        scope: dict[str, str],
        user_content: str,
        assistant_content: str,
        response: dict[str, Any],
        memory_write_required: bool,
    ) -> dict[str, Any]:
        encoded = json.dumps(response, ensure_ascii=False, separators=(",", ":"))
        with self._lock:
            try:
                self._connection.execute(
                    """INSERT INTO conversation_turns(
                         turn_id, user_id, character_id, session_id, user_content,
                         assistant_content, response_json, memory_write_completed,
                         created_at_unix_ms
                       ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)""",
                    (
                        turn_id,
                        *_scope_tuple(scope),
                        user_content,
                        assistant_content,
                        encoded,
                        0 if memory_write_required else 1,
                        int(time.time() * 1000),
                    ),
                )
                self._connection.commit()
            except sqlite3.IntegrityError:
                cached = self.cached_response(turn_id, scope, user_content)
                if cached is None:
                    raise
                return cached
        return response

    def memory_write_pending(self, turn_id: str) -> bool:
        with self._lock:
            row = self._connection.execute(
                "SELECT memory_write_completed FROM conversation_turns WHERE turn_id = ?1",
                (turn_id,),
            ).fetchone()
        return row is not None and row[0] == 0

    def complete_memory_write(self, turn_id: str) -> None:
        with self._lock:
            self._connection.execute(
                """UPDATE conversation_turns SET memory_write_completed = 1
                   WHERE turn_id = ?1""",
                (turn_id,),
            )
            self._connection.commit()

    def _ensure_column(self, table: str, column: str, definition: str) -> None:
        columns = {
            row[1]
            for row in self._connection.execute(f"PRAGMA table_info({table})").fetchall()
        }
        if column not in columns:
            self._connection.execute(
                f"ALTER TABLE {table} ADD COLUMN {column} {definition}"
            )
            self._connection.commit()

def validate_scope(scope: Any) -> dict[str, str]:
    if not isinstance(scope, dict):
        raise ValueError("scope must be an object")
    result: dict[str, str] = {}
    for key in ("userId", "characterId", "sessionId"):
        value = scope.get(key)
        if not isinstance(value, str) or not value.strip() or len(value) > 128:
            raise ValueError(f"{key} must contain 1 to 128 characters")
        if any(ord(character) < 32 or ord(character) == 127 for character in value):
            raise ValueError(f"{key} cannot contain control characters")
        result[key] = value.strip()
    return result


def _scope_tuple(scope: dict[str, str]) -> tuple[str, str, str]:
    return scope["userId"], scope["characterId"], scope["sessionId"]
