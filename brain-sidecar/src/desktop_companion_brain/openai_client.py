from __future__ import annotations

import httpx
from openai import OpenAI

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
