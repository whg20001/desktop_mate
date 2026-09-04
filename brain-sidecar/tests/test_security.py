from __future__ import annotations

import os
import socket
import sys
import tempfile
import types
import unittest
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from desktop_companion_brain.config import SidecarConfig
from desktop_companion_brain.http_client import LocalHttpClient
from desktop_companion_brain.memory import Mem0Memory
from desktop_companion_brain.security import LocalityError, local_data_directory, local_http_url


class LocalityTests(unittest.TestCase):
    def test_only_loopback_http_endpoints_are_accepted(self) -> None:
        self.assertEqual(
            local_http_url("http://127.0.0.1:11434/v1/"),
            "http://127.0.0.1:11434/v1",
        )
        self.assertEqual(
            local_http_url("http://localhost:11434/v1"),
            "http://localhost:11434/v1",
        )
        for value in (
            "https://127.0.0.1:11434/v1",
            "http://192.168.1.2:11434/v1",
            "http://0.0.0.0:11434/v1",
            "http://token@127.0.0.1:11434/v1",
            "http://example.com:11434/v1",
        ):
            with self.subTest(value=value), self.assertRaises(LocalityError):
                local_http_url(value)

    def test_data_directory_cannot_escape_app_data(self) -> None:
        with tempfile.TemporaryDirectory() as root, tempfile.TemporaryDirectory() as outside:
            child = local_data_directory(str(Path(root) / "brain"), root)
            self.assertEqual(child, (Path(root) / "brain").resolve())
            with self.assertRaises(LocalityError):
                local_data_directory(outside, root)

    def test_mixed_dns_answers_are_rejected(self) -> None:
        answers = [
            (socket.AF_INET, socket.SOCK_STREAM, 6, "", ("127.0.0.1", 11434)),
            (socket.AF_INET, socket.SOCK_STREAM, 6, "", ("203.0.113.7", 11434)),
        ]
        with patch("desktop_companion_brain.security.socket.getaddrinfo", return_value=answers):
            with self.assertRaises(LocalityError):
                local_http_url("http://localhost:11434/v1")

    def test_arbitrary_hostnames_are_rejected_even_if_dns_temporarily_returns_loopback(self) -> None:
        answers = [
            (socket.AF_INET, socket.SOCK_STREAM, 6, "", ("127.0.0.1", 11434)),
        ]
        with patch("desktop_companion_brain.security.socket.getaddrinfo", return_value=answers):
            with self.assertRaises(LocalityError):
                local_http_url("http://attacker-controlled.test:11434/v1")

    def test_http_client_ignores_environment_proxies_and_disables_redirects(self) -> None:
        with patch.dict(
            os.environ,
            {
                "HTTP_PROXY": "http://203.0.113.8:3128",
                "HTTPS_PROXY": "http://203.0.113.8:3128",
            },
        ):
            client = LocalHttpClient("http://127.0.0.1:11434/v1", 2)
        self.assertFalse(
            any(
                handler.__class__.__name__ == "ProxyHandler"
                for handler in client._opener.handlers
            )
        )
        redirects = [
            handler
            for handler in client._opener.handlers
            if handler.__class__.__name__ == "_NoRedirect"
        ]
        self.assertEqual(len(redirects), 1)
        self.assertIsNone(
            redirects[0].redirect_request(None, None, 302, "redirect", {}, "http://example.com")
        )

    def test_mem0_configuration_has_no_cloud_or_external_storage_defaults(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            config = SidecarConfig(
                host="127.0.0.1",
                port=8765,
                token="t" * 32,
                app_data_root=root,
                data_dir=root / "brain",
                llm_base_url="http://127.0.0.1:11434/v1",
                llm_model="local-llm",
                embedding_base_url="http://127.0.0.1:11434/v1",
                embedding_model="local-embedding",
                memory_enabled=True,
                recall_enabled=True,
                memory_write_enabled=True,
                recall_limit=6,
                request_timeout_seconds=5,
            )
            mem0 = config.mem0_config()
            self.assertEqual(mem0["llm"]["provider"], "openai")
            self.assertEqual(
                mem0["llm"]["config"]["openai_base_url"],
                "http://127.0.0.1:11434/v1",
            )
            self.assertEqual(
                mem0["embedder"]["config"]["openai_base_url"],
                "http://127.0.0.1:11434/v1",
            )
            self.assertEqual(mem0["embedder"]["config"]["embedding_dims"], 768)
            self.assertEqual(mem0["vector_store"]["provider"], "qdrant")
            self.assertEqual(
                mem0["vector_store"]["config"]["embedding_model_dims"],
                768,
            )
            self.assertTrue(
                mem0["vector_store"]["config"]["collection_name"].startswith(
                    "desktop_companion_memories_768_"
                )
            )
            Path(mem0["vector_store"]["config"]["path"]).resolve().relative_to(root)
            Path(mem0["history_db_path"]).resolve().relative_to(root)

    def test_embedding_model_and_dimensions_have_stable_isolated_collections(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            base = SidecarConfig(
                host="127.0.0.1",
                port=8765,
                token="t" * 32,
                app_data_root=root,
                data_dir=root / "brain",
                llm_base_url="http://127.0.0.1:11434/v1",
                llm_model="local-llm",
                embedding_base_url="http://127.0.0.1:11434/v1",
                embedding_model="embedding-a",
                memory_enabled=True,
                recall_enabled=True,
                memory_write_enabled=True,
                recall_limit=6,
                request_timeout_seconds=5,
                embedding_dimensions=768,
            )

            def collection(config: SidecarConfig) -> str:
                return config.mem0_config()["vector_store"]["config"]["collection_name"]

            base_collection = collection(base)
            self.assertEqual(collection(replace(base)), base_collection)
            self.assertNotEqual(
                collection(replace(base, embedding_dimensions=1024)),
                base_collection,
            )
            self.assertNotEqual(
                collection(replace(base, embedding_model="embedding-b")),
                base_collection,
            )
            self.assertIn("_768_", base_collection)
            self.assertNotIn("embedding-a", base_collection)

    def test_mem0_telemetry_is_disabled_before_initialization(self) -> None:
        captured: dict[str, object] = {}

        class Closeable:
            def close(self):
                captured["closed"] = int(captured.get("closed", 0)) + 1

        class FakeMem0Instance:
            def __init__(self):
                self.llm = types.SimpleNamespace(client=Closeable())
                self.embedding_model = types.SimpleNamespace(client=Closeable())

        class FakeMemoryFactory:
            @classmethod
            def from_config(cls, config):
                captured["config"] = config
                captured["environment"] = {
                    name: os.environ.get(name)
                    for name in (
                        "MEM0_TELEMETRY",
                        "ANONYMIZED_TELEMETRY",
                        "POSTHOG_DISABLED",
                    )
                }
                return FakeMem0Instance()

        fake_mem0 = types.ModuleType("mem0")
        fake_mem0.Memory = FakeMemoryFactory
        fake_memory_package = types.ModuleType("mem0.memory")
        fake_main = types.ModuleType("mem0.memory.main")
        fake_main.MEM0_TELEMETRY = True
        fake_telemetry = types.ModuleType("mem0.memory.telemetry")
        fake_telemetry.MEM0_TELEMETRY = True
        fake_telemetry.client_telemetry = Closeable()
        fake_memory_package.main = fake_main
        fake_memory_package.telemetry = fake_telemetry
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            config = SidecarConfig(
                host="127.0.0.1",
                port=8765,
                token="t" * 32,
                app_data_root=root,
                data_dir=root / "brain",
                llm_base_url="http://127.0.0.1:11434/v1",
                llm_model="local-llm",
                embedding_base_url="http://127.0.0.1:11434/v1",
                embedding_model="local-embedding",
                memory_enabled=True,
                recall_enabled=True,
                memory_write_enabled=True,
                recall_limit=6,
                request_timeout_seconds=5,
            )
            with (
                patch.dict(
                    sys.modules,
                    {
                        "mem0": fake_mem0,
                        "mem0.memory": fake_memory_package,
                        "mem0.memory.main": fake_main,
                        "mem0.memory.telemetry": fake_telemetry,
                    },
                ),
                patch(
                    "desktop_companion_brain.memory.create_local_openai_client",
                    side_effect=lambda *_: Closeable(),
                ),
            ):
                memory = Mem0Memory(config)
            self.assertTrue(memory.ready()[0])
        self.assertEqual(
            captured["environment"],
            {
                "MEM0_TELEMETRY": "false",
                "ANONYMIZED_TELEMETRY": "false",
                "POSTHOG_DISABLED": "true",
            },
        )
        self.assertFalse(fake_main.MEM0_TELEMETRY)
        self.assertFalse(fake_telemetry.MEM0_TELEMETRY)
        self.assertGreaterEqual(int(captured["closed"]), 3)


if __name__ == "__main__":
    unittest.main()
