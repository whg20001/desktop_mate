from __future__ import annotations

import json
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.request import HTTPRedirectHandler, ProxyHandler, Request, build_opener

from .security import local_http_url


class _NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, request, file_pointer, code, message, headers, new_url):
        return None


class LocalHttpClient:
    def __init__(self, base_url: str, timeout_seconds: float) -> None:
        self.base_url = local_http_url(base_url)
        self.timeout_seconds = timeout_seconds
        self._opener = build_opener(ProxyHandler({}), _NoRedirect())

    def json(self, method: str, path: str, body: Any | None = None) -> Any:
        url = f"{self.base_url}/{path.lstrip('/')}"
        local_http_url(url)
        data = None if body is None else json.dumps(body).encode("utf-8")
        request = Request(
            url,
            data=data,
            method=method,
            headers={"Content-Type": "application/json", "Accept": "application/json"},
        )
        try:
            with self._opener.open(request, timeout=self.timeout_seconds) as response:
                local_http_url(response.geturl())
                payload = response.read(2_000_001)
        except HTTPError as error:
            raise RuntimeError(f"local provider returned HTTP {error.code}") from error
        except URLError as error:
            raise RuntimeError("local provider is unreachable") from error
        if len(payload) > 2_000_000:
            raise RuntimeError("local provider response exceeded the size limit")
        try:
            return json.loads(payload)
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise RuntimeError("local provider returned invalid JSON") from error
