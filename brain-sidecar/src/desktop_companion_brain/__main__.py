from __future__ import annotations

import logging
import os

from .config import SidecarConfig
from .server import serve


def _configure_logging(log_path: os.PathLike[str]) -> None:
    root = logging.getLogger()
    root.handlers.clear()
    root.setLevel(logging.CRITICAL)

    logger = logging.getLogger("desktop_companion_brain")
    logger.handlers.clear()
    logger.setLevel(logging.INFO)
    logger.propagate = False
    handler = logging.FileHandler(log_path, encoding="utf-8")
    handler.setFormatter(logging.Formatter("%(asctime)s %(levelname)s %(message)s"))
    logger.addHandler(handler)


def main() -> None:
    os.environ["MEM0_TELEMETRY"] = "false"
    os.environ["ANONYMIZED_TELEMETRY"] = "false"
    os.environ["POSTHOG_DISABLED"] = "true"
    for name in (
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "OPENROUTER_API_KEY",
        "OPENROUTER_API_BASE",
        "MEM0_API_KEY",
    ):
        os.environ.pop(name, None)
    os.environ["NO_PROXY"] = "127.0.0.1,localhost,::1"
    os.environ["no_proxy"] = "127.0.0.1,localhost,::1"

    config = SidecarConfig.from_environment()
    log_path = config.data_dir / "brain-sidecar.log"
    _configure_logging(log_path)
    serve(config)


if __name__ == "__main__":
    main()
