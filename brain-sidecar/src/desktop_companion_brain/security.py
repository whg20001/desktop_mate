from __future__ import annotations

import ipaddress
import socket
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit


class LocalityError(ValueError):
    """Raised when a network or storage target escapes the local boundary."""


def local_http_url(value: str) -> str:
    parsed = urlsplit(value.strip())
    if parsed.scheme != "http":
        raise LocalityError("local endpoints must use HTTP over loopback")
    if parsed.username or parsed.password:
        raise LocalityError("credentials are not allowed in endpoint URLs")
    if parsed.query or parsed.fragment:
        raise LocalityError("endpoint query strings and fragments are not allowed")
    if not parsed.hostname or not parsed.port:
        raise LocalityError("endpoint must include a host and port")

    try:
        host_address = ipaddress.ip_address(parsed.hostname)
    except ValueError:
        if parsed.hostname.lower() != "localhost":
            raise LocalityError("endpoint host must be localhost or a loopback IP address")
    else:
        if not host_address.is_loopback:
            raise LocalityError("endpoint must use a loopback IP address")

    try:
        addresses = {
            item[4][0]
            for item in socket.getaddrinfo(
                parsed.hostname,
                parsed.port,
                type=socket.SOCK_STREAM,
            )
        }
    except OSError as error:
        raise LocalityError("endpoint host could not be resolved") from error
    if not addresses or any(not ipaddress.ip_address(address).is_loopback for address in addresses):
        raise LocalityError("endpoint must resolve exclusively to loopback addresses")

    path = parsed.path.rstrip("/")
    return urlunsplit((parsed.scheme, parsed.netloc, path, "", ""))


def local_data_directory(value: str, app_data_root: str) -> Path:
    root = Path(app_data_root).expanduser().resolve()
    candidate = Path(value).expanduser().resolve()
    if str(candidate).startswith("\\") or str(root).startswith("\\"):
        raise LocalityError("UNC and network share paths are not allowed")
    try:
        candidate.relative_to(root)
    except ValueError as error:
        raise LocalityError("sidecar data must remain below the application data directory") from error
    candidate.mkdir(parents=True, exist_ok=True)
    return candidate
