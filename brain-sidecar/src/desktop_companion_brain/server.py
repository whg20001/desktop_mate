from __future__ import annotations

import hmac
import json
import logging
import threading
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any
from urllib.parse import unquote

from . import __version__
from .config import SidecarConfig
from .llm import LocalLlmProvider
from .http_client import LocalHttpClient
from .memory import MemoryPort, create_memory
from .orchestrator import ConversationOrchestrator
from .session_store import ConversationSessionStore, validate_scope


LOGGER = logging.getLogger("desktop_companion_brain")


class BrainApplication:
    def __init__(
        self,
        config: SidecarConfig,
        *,
        llm: LocalLlmProvider | None = None,
        memory: MemoryPort | None = None,
    ) -> None:
        self.config = config
        self.sessions = ConversationSessionStore(config.data_dir / "conversations.sqlite3")
        self.llm = llm or _create_llm(config)
        self.memory = memory or create_memory(config)
        self.orchestrator = (
            ConversationOrchestrator(config, self.sessions, self.llm, self.memory)
            if self.llm is not None
            else None
        )

    def health(self) -> tuple[HTTPStatus, dict[str, Any]]:
        configuration_errors = self.config.readiness_errors()
        llm_ready, llm_detail = (
            self.llm.ready() if self.llm is not None else (False, "local LLM is not configured")
        )
        memory_ready, memory_detail = self.memory.ready()
        embedding_ready, embedding_detail = _embedding_health(self.config)
        ready = (
            not configuration_errors
            and llm_ready
            and embedding_ready
            and memory_ready
        )
        return (
            HTTPStatus.OK if ready else HTTPStatus.SERVICE_UNAVAILABLE,
            {
                "status": "ok" if ready else "degraded",
                "version": __version__,
                "components": {
                    "llm": {"ready": llm_ready, "detail": llm_detail},
                    "memory": {"ready": memory_ready, "detail": memory_detail},
                    "embedding": {
                        "ready": embedding_ready,
                        "detail": embedding_detail,
                    },
                    "session": {"ready": True, "detail": "local SQLite is ready"},
                },
                "configurationErrors": configuration_errors,
                "locality": {
                    "serviceLoopback": True,
                    "llmLocal": bool(self.config.llm_base_url),
                    "embeddingLocal": (
                        bool(self.config.embedding_base_url) or not self.config.memory_enabled
                    ),
                    "vectorStoreLocal": True,
                    "metadataStoreLocal": True,
                },
            },
        )

    def close(self) -> None:
        close_memory = getattr(self.memory, "close", None)
        if callable(close_memory):
            close_memory()
        self.sessions.close()


class BrainHttpServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, application: BrainApplication) -> None:
        self.application = application
        super().__init__((application.config.host, application.config.port), BrainRequestHandler)


class BrainRequestHandler(BaseHTTPRequestHandler):
    server: BrainHttpServer

    def log_message(self, format: str, *args: object) -> None:
        LOGGER.info("request method=%s path=%s", self.command, self.path.split("?", 1)[0])

    def do_GET(self) -> None:
        if not self._authenticated():
            return
        if self.path == "/live":
            self._json(HTTPStatus.OK, {"status": "alive", "version": __version__})
            return
        if self.path in {"/ready", "/health"}:
            status, body = self.server.application.health()
            self._json(status, body)
            return
        self._json(HTTPStatus.NOT_FOUND, {"error": "route not found"})

    def do_POST(self) -> None:
        if not self._authenticated():
            return
        try:
            body = self._body()
            if self.path == "/shutdown":
                self._json(HTTPStatus.OK, {"status": "stopping"})
                threading.Thread(target=self.server.shutdown, daemon=True).start()
            elif self.path == "/v1/conversation":
                orchestrator = self.server.application.orchestrator
                if orchestrator is None:
                    raise RuntimeError("local LLM is not configured")
                self._json(HTTPStatus.OK, orchestrator.converse(body))
            elif self.path == "/v1/memories":
                entry = body.get("entry")
                if not isinstance(entry, dict):
                    raise ValueError("entry is required")
                self._json(HTTPStatus.OK, {"id": self.server.application.memory.add_entry(entry)})
            elif self.path == "/v1/memories/list":
                scope = validate_scope(body.get("scope"))
                self._json(
                    HTTPStatus.OK,
                    {"records": self.server.application.memory.list(scope)},
                )
            elif self.path == "/v1/memories/search":
                scope = validate_scope(_sidecar_scope(body.get("scope")))
                records = self.server.application.memory.search(
                    str(body.get("text", "")),
                    scope,
                    max(1, min(int(body.get("topK", 6)), 100)),
                )
                self._json(
                    HTTPStatus.OK,
                    {"records": [_provider_record(item, body["scope"]) for item in records]},
                )
            elif self.path == "/v1/memories/clear":
                self.server.application.memory.clear(validate_scope(_sidecar_scope(body)))
                self._json(HTTPStatus.OK, {"status": "cleared"})
            else:
                self._json(HTTPStatus.NOT_FOUND, {"error": "route not found"})
        except (ValueError, RuntimeError) as error:
            self._failure(error)

    def do_PATCH(self) -> None:
        if not self._authenticated():
            return
        if not self.path.startswith("/v1/memories/"):
            self._json(HTTPStatus.NOT_FOUND, {"error": "route not found"})
            return
        try:
            memory_id = unquote(self.path.removeprefix("/v1/memories/"))
            body = self._body()
            content = body.get("content")
            if not isinstance(content, str):
                raise ValueError("content is required")
            scope = validate_scope(body.get("scope"))
            self.server.application.memory.update(memory_id, content, scope)
            self._json(HTTPStatus.OK, {"status": "updated"})
        except (ValueError, RuntimeError) as error:
            self._failure(error)

    def do_DELETE(self) -> None:
        if not self._authenticated():
            return
        if not self.path.startswith("/v1/memories/"):
            self._json(HTTPStatus.NOT_FOUND, {"error": "route not found"})
            return
        try:
            memory_id = unquote(self.path.removeprefix("/v1/memories/"))
            scope = validate_scope(self._body().get("scope"))
            self.server.application.memory.delete(memory_id, scope)
            self._json(HTTPStatus.OK, {"status": "deleted"})
        except (ValueError, RuntimeError) as error:
            self._failure(error)

    def _authenticated(self) -> bool:
        supplied = self.headers.get("x-desktop-companion-token", "")
        if not hmac.compare_digest(supplied, self.server.application.config.token):
            self._json(HTTPStatus.UNAUTHORIZED, {"error": "unauthorized"})
            return False
        return True

    def _body(self) -> dict[str, Any]:
        try:
            length = int(self.headers.get("content-length", "0"))
        except ValueError as error:
            raise ValueError("invalid content length") from error
        if length <= 0 or length > 1_000_000:
            raise ValueError("request body size is invalid")
        try:
            body = json.loads(self.rfile.read(length))
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise ValueError("request body is invalid JSON") from error
        if not isinstance(body, dict):
            raise ValueError("request body must be an object")
        return body

    def _failure(self, error: Exception) -> None:
        status = HTTPStatus.BAD_REQUEST if isinstance(error, ValueError) else HTTPStatus.SERVICE_UNAVAILABLE
        self._json(status, {"error": f"request failed ({type(error).__name__})"})

    def _json(self, status: HTTPStatus, body: dict[str, Any]) -> None:
        encoded = json.dumps(body, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        self.send_response(status.value)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(encoded)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(encoded)


def serve(config: SidecarConfig) -> None:
    application = BrainApplication(config)
    server = BrainHttpServer(application)
    LOGGER.info("sidecar started host=127.0.0.1 port=%d", config.port)
    try:
        server.serve_forever(poll_interval=0.2)
    finally:
        server.server_close()
        application.close()
        LOGGER.info("sidecar stopped")


def _create_llm(config: SidecarConfig) -> LocalLlmProvider | None:
    if not config.llm_base_url or not config.llm_model:
        return None
    return LocalLlmProvider(config.llm_base_url, config.llm_model, config.request_timeout_seconds)


def _embedding_health(config: SidecarConfig) -> tuple[bool, str]:
    if not config.memory_enabled:
        return True, "embedding is not required while memory is disabled"
    if not config.embedding_base_url or not config.embedding_model:
        return False, "local embedding is not configured"
    try:
        payload = LocalHttpClient(
            config.embedding_base_url,
            config.request_timeout_seconds,
        ).json("GET", "models")
        models = payload.get("data", []) if isinstance(payload, dict) else []
        identifiers = {
            item.get("id") for item in models if isinstance(item, dict)
        }
        if identifiers and config.embedding_model not in identifiers:
            return False, "configured local embedding model is not listed"
        return True, "local embedding provider is reachable"
    except RuntimeError as error:
        return False, str(error)


def _sidecar_scope(scope: Any) -> dict[str, Any]:
    if not isinstance(scope, dict):
        raise ValueError("scope is required")
    copy = dict(scope)
    copy["sessionId"] = copy.get("sessionId") or "global"
    return copy


def _provider_record(record: dict[str, Any], scope: dict[str, Any]) -> dict[str, Any]:
    metadata = record.get("metadata") if isinstance(record.get("metadata"), dict) else {}
    stored_scope = metadata.get("scope") if isinstance(metadata.get("scope"), dict) else scope
    return {
        "entry": {
            "scope": stored_scope,
            "kind": metadata.get("kind", "semantic"),
            "content": record["content"],
            "importance": metadata.get("importance", record.get("score", 0.5)),
            "tags": metadata.get("tags", []),
            "occurredAtUnixMs": metadata.get("occurredAtUnixMs"),
            "createdAtUnixMs": metadata.get("createdAtUnixMs", 0),
            "sourceEventIds": metadata.get("sourceEventIds", []),
            "metadata": metadata,
        },
        "score": record.get("score", 1.0),
    }
