"""Request-scoped dependencies."""

from __future__ import annotations

from functools import lru_cache

from fastapi import Depends, HTTPException, status

from oq_adapters import RuntimeAdapter
from oq_adapters.legacy_py import LegacyPyAdapter
from oq_adapters.oq_cli import OqCliAdapter

from ..settings import Settings

_ADAPTERS = {
    "legacy_py": LegacyPyAdapter,
    "oq_cli": OqCliAdapter,
}


@lru_cache(maxsize=1)
def get_settings() -> Settings:
    return Settings.from_env()


def get_adapter(
    settings: Settings = Depends(get_settings),
) -> RuntimeAdapter:
    if settings.runtime_root is None:
        raise HTTPException(
            status_code=status.HTTP_428_PRECONDITION_REQUIRED,
            detail="no runtime configured; complete setup first",
        )
    factory = _ADAPTERS.get(settings.runtime_kind)
    if factory is None:
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"unknown runtime kind {settings.runtime_kind!r}",
        )
    if factory is LegacyPyAdapter:
        return factory(
            settings.runtime_root,
            python_executable=settings.runtime_python or None,
        )
    return factory(settings.runtime_root)


def require_writes(settings: Settings = Depends(get_settings)) -> None:
    """Gate every mutating route behind an explicit opt-in.

    A deck that has not been told it may write is a viewer. That is the
    state a fresh install is in, and the state an operator can return to
    in one setting when they want to look at production without the risk
    of changing it.
    """
    if not settings.allow_writes:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail=(
                "this deck is in read-only mode; enable writes in "
                "Settings before changing anything"
            ),
        )
