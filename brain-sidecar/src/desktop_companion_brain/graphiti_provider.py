from __future__ import annotations

import asyncio
import hashlib
import os
import threading
from concurrent.futures import Future
from datetime import UTC, datetime
from typing import Any, Coroutine

from .config import SidecarConfig
from .memory_policy import ApprovedMemoryEvent
from .openai_client import create_local_async_openai_client
from .retrieval import lexical_score


class GraphitiMemory:
    """Optional temporal graph index backed by a loopback-only Neo4j service."""

    provider_id = "graphiti"

    def __init__(self, config: SidecarConfig) -> None:
        self._config = config
        self._runner = _AsyncRunner()
        self._graph: Any | None = None
        self._initializing_graph: Any | None = None
        self._clients: list[Any] = []
        self._startup_error: str | None = None
        self._closed = False
        self._initializing = self._runner.submit(self._initialize())

    def ready(self) -> tuple[bool, str]:
        self._finish_initialization()
        if self._initializing is not None:
            return False, "Graphiti is initializing"
        if self._graph is None:
            return False, self._startup_error or "Graphiti is unavailable"
        return True, "local Graphiti and Neo4j are ready"

    def search(
        self,
        query: str,
        scope: dict[str, str],
        limit: int,
    ) -> list[dict[str, Any]]:
        graph = self._require()
        edges = self._runner.run(
            graph.search(
                query,
                group_ids=[_group_id(scope)],
                num_results=max(1, min(limit, 20)),
            ),
            self._config.request_timeout_seconds,
        )
        results: list[dict[str, Any]] = []
        for index, edge in enumerate(edges):
            fact = getattr(edge, "fact", None)
            identifier = getattr(edge, "uuid", None)
            if isinstance(fact, str) and fact.strip() and isinstance(identifier, str):
                results.append(
                    {
                        "id": identifier,
                        "content": fact.strip(),
                        "score": max(0.0, 1.0 - index * 0.05),
                        "canonicalEventIds": list(getattr(edge, 'episodes', [])),
                        "createdAt": None,
                        "updatedAt": None,
                    }
                )
        return results

    def list(self, scope: dict[str, str]) -> list[dict[str, Any]]:
        return []

    def upsert(
        self,
        event: ApprovedMemoryEvent,
        provider_record_id: str | None,
    ) -> str:
        graph = self._require()
        record_id = provider_record_id or event.event_id
        try:
            self._runner.run(
                graph.remove_episode(record_id),
                self._config.request_timeout_seconds,
            )
        except Exception:
            pass
        reference_time = datetime.fromtimestamp(
            (event.occurred_at_unix_ms or event.created_at_unix_ms) / 1000,
            tz=UTC,
        )
        self._runner.run(
            self._add_episode(graph, event, record_id, reference_time),
            self._config.request_timeout_seconds,
        )
        return record_id

    async def _add_episode(
        self,
        graph: Any,
        event: ApprovedMemoryEvent,
        record_id: str,
        reference_time: datetime,
    ) -> None:
        from graphiti_core.nodes import EpisodeType

        await graph.add_episode(
            name=f"desktop-companion-memory-{event.event_id}",
            episode_body=event.content,
            source_description=f"local {event.kind} memory",
            reference_time=reference_time,
            source=EpisodeType.text,
            group_id=_group_id(event.scope),
            uuid=record_id,
            update_communities=False,
        )

    def delete_record(self, provider_record_id: str, scope: dict[str, str]) -> None:
        self._runner.run(
            self._require().remove_episode(provider_record_id),
            self._config.request_timeout_seconds,
        )

    def close(self) -> None:
        if self._closed:
            return
        self._closed = True
        if self._initializing is not None:
            self._initializing.cancel()
            self._initializing = None
        try:
            self._runner.run(self._close_async(), 5.0)
        except Exception:
            pass
        finally:
            self._graph = None
            self._runner.close()

    async def _initialize(self) -> None:
        os.environ["GRAPHITI_TELEMETRY_ENABLED"] = "false"
        from graphiti_core import Graphiti
        from graphiti_core.cross_encoder.client import CrossEncoderClient
        from graphiti_core.driver.neo4j_driver import Neo4jDriver
        from graphiti_core.embedder.openai import OpenAIEmbedder, OpenAIEmbedderConfig
        from graphiti_core.llm_client.config import LLMConfig
        from graphiti_core.llm_client.openai_generic_client import OpenAIGenericClient

        class LocalLexicalReranker(CrossEncoderClient):
            async def rank(
                self,
                query: str,
                passages: list[str],
            ) -> list[tuple[str, float]]:
                scored = [
                    (passage, lexical_score(query, passage))
                    for passage in passages
                ]
                return sorted(scored, key=lambda value: value[1], reverse=True)

        llm_http = create_local_async_openai_client(
            self._config.llm_base_url,
            self._config.request_timeout_seconds,
        )
        embedding_http = create_local_async_openai_client(
            self._config.embedding_base_url,
            self._config.request_timeout_seconds,
        )
        self._clients = [llm_http, embedding_http]
        llm = OpenAIGenericClient(
            config=LLMConfig(
                api_key="local-only",
                model=self._config.llm_model,
                small_model=self._config.llm_model,
                base_url=self._config.llm_base_url,
                temperature=0.0,
            ),
            client=llm_http,
            structured_output_mode="json_object",
        )
        embedder = OpenAIEmbedder(
            config=OpenAIEmbedderConfig(
                embedding_model=self._config.embedding_model,
                embedding_dim=self._config.embedding_dimensions,
                api_key="local-only",
                base_url=self._config.embedding_base_url,
            ),
            client=embedding_http,
        )
        driver = Neo4jDriver(
            self._config.graphiti_uri,
            self._config.graphiti_user,
            os.environ["DESKTOP_COMPANION_GRAPHITI_PASSWORD"],
            database=self._config.graphiti_database,
        )
        graph = Graphiti(
            graph_driver=driver,
            llm_client=llm,
            embedder=embedder,
            cross_encoder=LocalLexicalReranker(),
            store_raw_episode_content=False,
            max_coroutines=2,
        )
        self._initializing_graph = graph
        try:
            await graph.build_indices_and_constraints()
            await driver.health_check()
        except BaseException:
            if self._initializing_graph is graph:
                self._initializing_graph = None
                await graph.close()
            await self._close_clients()
            raise
        else:
            self._initializing_graph = None
            self._graph = graph

    async def _close_async(self) -> None:
        graph = self._graph or self._initializing_graph
        self._graph = None
        self._initializing_graph = None
        if graph is not None:
            await graph.close()
        await self._close_clients()

    async def _close_clients(self) -> None:
        clients, self._clients = self._clients, []
        for client in clients:
            await client.close()

    def _require(self) -> Any:
        self._finish_initialization()
        if self._graph is None:
            raise RuntimeError(self._startup_error or "Graphiti is unavailable")
        return self._graph

    def _finish_initialization(self) -> None:
        if self._initializing is None or not self._initializing.done():
            return
        try:
            self._initializing.result()
        except Exception as error:
            self._startup_error = (
                f"Graphiti initialization failed: {type(error).__name__}"
            )
        finally:
            self._initializing = None


class _AsyncRunner:
    def __init__(self) -> None:
        self._loop = asyncio.new_event_loop()
        self._started = threading.Event()
        self._thread = threading.Thread(
            target=self._run,
            name="graphiti-async-loop",
            daemon=True,
        )
        self._thread.start()
        self._started.wait(timeout=2.0)

    def run(self, coroutine: Coroutine[Any, Any, Any], timeout: float) -> Any:
        future = self.submit(coroutine)
        return future.result(timeout=timeout)

    def submit(self, coroutine: Coroutine[Any, Any, Any]) -> Future[Any]:
        return asyncio.run_coroutine_threadsafe(coroutine, self._loop)

    def close(self) -> None:
        if self._loop.is_closed():
            return
        if self._loop.is_running():
            try:
                self.run(self._cancel_pending(), 2.0)
            except Exception:
                pass
            self._loop.call_soon_threadsafe(self._loop.stop)
        self._thread.join(timeout=2.0)
        if not self._thread.is_alive() and not self._loop.is_closed():
            self._loop.close()

    def _run(self) -> None:
        asyncio.set_event_loop(self._loop)
        self._started.set()
        self._loop.run_forever()

    async def _cancel_pending(self) -> None:
        current = asyncio.current_task()
        tasks = [
            task
            for task in asyncio.all_tasks()
            if task is not current and not task.done()
        ]
        for task in tasks:
            task.cancel()
        if tasks:
            await asyncio.gather(*tasks, return_exceptions=True)
        await self._loop.shutdown_asyncgens()


def _group_id(scope: dict[str, str]) -> str:
    identity = f"{scope['userId']}\0{scope['characterId']}".encode("utf-8")
    return "desktop-companion-" + hashlib.sha256(identity).hexdigest()[:24]
