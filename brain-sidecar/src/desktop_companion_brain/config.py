from __future__ import annotations

import hashlib
import json
import os
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .security import LocalityError, local_data_directory, local_http_url


def _bool(value: Any, default: bool) -> bool:
    if value is None:
        return default
    if isinstance(value, bool):
        return value
    return str(value).strip().lower() in {"1", "true", "yes", "on"}


@dataclass(frozen=True)
class SidecarConfig:
    host: str
    port: int
    token: str
    app_data_root: Path
    data_dir: Path
    llm_base_url: str
    llm_model: str
    embedding_base_url: str
    embedding_model: str
    memory_enabled: bool
    recall_enabled: bool
    memory_write_enabled: bool
    recall_limit: int
    request_timeout_seconds: float
    embedding_dimensions: int = 768

    @classmethod
    def from_environment(cls) -> "SidecarConfig":
        host = os.environ.get("DESKTOP_COMPANION_BRAIN_HOST", "127.0.0.1")
        if host != "127.0.0.1":
            raise LocalityError("the brain sidecar may only bind to 127.0.0.1")
        port = int(os.environ["DESKTOP_COMPANION_BRAIN_PORT"])
        if not 1 <= port <= 65535:
            raise ValueError("sidecar port is outside the valid range")
        token = os.environ["DESKTOP_COMPANION_BRAIN_TOKEN"]
        if not 16 <= len(token) <= 256:
            raise ValueError("sidecar token must contain 16 to 256 characters")

        root_value = os.environ["DESKTOP_COMPANION_APP_DATA_ROOT"]
        data_value = os.environ["DESKTOP_COMPANION_BRAIN_DATA_DIR"]
        root = Path(root_value).expanduser().resolve()
        data_dir = local_data_directory(data_value, root_value)
        persisted = _load_json(data_dir / "settings.json")

        llm_url = str(persisted.get("llmBaseUrl", "")).strip()
        embedding_url = str(persisted.get("embeddingBaseUrl", "")).strip()
        if llm_url:
            llm_url = local_http_url(llm_url)
        if embedding_url:
            embedding_url = local_http_url(embedding_url)

        return cls(
            host=host,
            port=port,
            token=token,
            app_data_root=root,
            data_dir=data_dir,
            llm_base_url=llm_url,
            llm_model=str(persisted.get("llmModel", "")).strip(),
            embedding_base_url=embedding_url,
            embedding_model=str(persisted.get("embeddingModel", "")).strip(),
            memory_enabled=_bool(persisted.get("memoryEnabled"), True),
            recall_enabled=_bool(persisted.get("recallEnabled"), True),
            memory_write_enabled=_bool(persisted.get("memoryWriteEnabled"), True),
            recall_limit=max(1, min(int(persisted.get("recallLimit", 6)), 20)),
            request_timeout_seconds=max(
                2.0,
                min(float(persisted.get("requestTimeoutSeconds", 30)), 120.0),
            ),
            embedding_dimensions=max(
                64,
                min(int(persisted.get("embeddingDimensions", 768)), 8192),
            ),
        )

    def readiness_errors(self) -> list[str]:
        errors: list[str] = []
        if not self.llm_base_url or not self.llm_model:
            errors.append("local LLM endpoint and model are not configured")
        if self.memory_enabled and (
            not self.embedding_base_url or not self.embedding_model
        ):
            errors.append("local embedding endpoint and model are not configured")
        return errors

    def mem0_config(self) -> dict[str, Any]:
        if self.readiness_errors():
            raise ValueError("local model configuration is incomplete")
        embedding_identity = hashlib.sha256(
            self.embedding_model.encode("utf-8")
        ).hexdigest()[:12]
        return {
            "version": "v1.1",
            "llm": {
                "provider": "openai",
                "config": {
                    "model": self.llm_model,
                    "openai_base_url": self.llm_base_url,
                    "api_key": "local-only",
                    "temperature": 0.0,
                },
            },
            "embedder": {
                "provider": "openai",
                "config": {
                    "model": self.embedding_model,
                    "openai_base_url": self.embedding_base_url,
                    "api_key": "local-only",
                    "embedding_dims": self.embedding_dimensions,
                },
            },
            "vector_store": {
                "provider": "qdrant",
                "config": {
                    "collection_name": (
                        "desktop_companion_memories_"
                        f"{self.embedding_dimensions}_{embedding_identity}"
                    ),
                    "path": str(self.data_dir / "vectors"),
                    "on_disk": True,
                    "embedding_model_dims": self.embedding_dimensions,
                },
            },
            "history_db_path": str(self.data_dir / "mem0-history.sqlite3"),
        }


def _load_json(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError("brain settings could not be read") from error
    if not isinstance(value, dict):
        raise ValueError("brain settings must be a JSON object")
    return value
