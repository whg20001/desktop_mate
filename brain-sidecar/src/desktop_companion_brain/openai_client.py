from __future__ import annotations

import httpx
from openai import AsyncOpenAI, OpenAI

from .security import local_http_url


def create_local_openai_client(base_url: str, timeout_seconds: float) -> OpenAI:
    transport = httpx.Client(
        trust_env=False,
        follow_redirects=False,
        timeout=timeout_seconds,
    )
    return OpenAI(
        api_key="local-only",
        base_url=local_http_url(base_url),
        http_client=transport,
        max_retries=0,
    )


def create_local_async_openai_client(
    base_url: str,
    timeout_seconds: float,
) -> AsyncOpenAI:
    transport = httpx.AsyncClient(
        trust_env=False,
        follow_redirects=False,
        timeout=timeout_seconds,
    )
    return AsyncOpenAI(
        api_key="local-only",
        base_url=local_http_url(base_url),
        http_client=transport,
        max_retries=0,
    )
