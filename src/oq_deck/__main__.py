"""`python -m oq_deck` / `oq-deck up`."""

from __future__ import annotations

import sys

from .settings import ConfigurationError, Settings


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    if argv and argv[0] not in {"up", "serve"}:
        print(f"oq-deck: unknown command {argv[0]!r}", file=sys.stderr)
        print("usage: oq-deck up", file=sys.stderr)
        return 2

    try:
        settings = Settings.from_env()
    except ConfigurationError as exc:
        # Refusing to start is the feature. Say what to do about it.
        print(f"oq-deck: {exc}", file=sys.stderr)
        return 1

    import uvicorn

    uvicorn.run(
        "oq_deck.app:app",
        host=settings.host,
        port=settings.port,
        reload=settings.dev_mode,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
