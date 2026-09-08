"""Runtime adapters for Quanterdeck.

An adapter teaches the deck how to talk to one kind of OpenQuanter. Two
ship here — `legacy_py` for the 1.x Python runtime and `oq_cli` for the
2.0 Rust tools — and a third party adds their own by publishing a package
that advertises an entry point:

    [project.entry-points."quanterdeck.adapters"]
    my_runtime = "my_pkg.adapter:MyAdapter"

Discovery is by entry point rather than by directory, so an adapter is a
normal installable package and needs no fork of this repository.
"""

from __future__ import annotations

from importlib.metadata import entry_points
from typing import Any

from .models import (
    SAFE_ACTIONS,
    ActionResult,
    BacktestSpec,
    Capabilities,
    ConfigDiff,
    ConfigDoc,
    GateStage,
    JobEvent,
    JobState,
    LiveState,
    Order,
    ParamField,
    Position,
    RuntimeKind,
    Service,
    ServiceAction,
    ServiceStatus,
    StrategyClass,
    StrategyInstance,
)
from .protocol import (
    AdapterError,
    ConflictError,
    NotSupported,
    RefusedError,
    RuntimeAdapter,
)

ENTRY_POINT_GROUP = "quanterdeck.adapters"

__all__ = [
    "ENTRY_POINT_GROUP",
    "SAFE_ACTIONS",
    "ActionResult",
    "AdapterError",
    "BacktestSpec",
    "Capabilities",
    "ConfigDiff",
    "ConfigDoc",
    "ConflictError",
    "GateStage",
    "JobEvent",
    "JobState",
    "LiveState",
    "NotSupported",
    "Order",
    "ParamField",
    "Position",
    "RefusedError",
    "RuntimeAdapter",
    "RuntimeKind",
    "Service",
    "ServiceAction",
    "ServiceStatus",
    "StrategyClass",
    "StrategyInstance",
    "available",
    "load",
]


def available() -> dict[str, Any]:
    """Every adapter class installed in this environment, by name."""
    found: dict[str, Any] = {}
    for ep in entry_points(group=ENTRY_POINT_GROUP):
        found[ep.name] = ep
    return found


def load(name: str) -> type:
    """Resolve one adapter class by entry-point name."""
    eps = available()
    if name not in eps:
        known = ", ".join(sorted(eps)) or "none installed"
        raise AdapterError(f"no adapter named {name!r} (installed: {known})")
    return eps[name].load()
