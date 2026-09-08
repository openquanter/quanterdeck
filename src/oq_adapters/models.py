"""Data model shared by every runtime adapter.

Standard library only, on purpose. A third party writing an adapter for
their own runtime should not inherit a dependency tree from this package;
`scripts/check-adapter-deps.sh` fails the build if one appears.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from enum import StrEnum
from typing import Any


class RuntimeKind(StrEnum):
    """Which OpenQuanter a deck is pointed at."""

    LEGACY_PY = "legacy_py"  # 1.x: JSON config + manager.sh + daemon.py
    OQ_CLI = "oq_cli"  # 2.0: the `oq` tools + journal


class ServiceStatus(StrEnum):
    RUNNING = "running"
    STOPPED = "stopped"
    UNKNOWN = "unknown"


class ServiceAction(StrEnum):
    """The only actions an adapter will ever perform on a service.

    `RESTART` is separate from stop+start and is not a convenience alias
    for it: on the 1.x daemon a restart is SIGUSR1, which hands the open
    orders over. A stop followed by a start cancels every resting order
    and loses order ownership. The two are different operations with
    different consequences, so they are different enum members and the
    UI only offers `RESTART`.
    """

    STATUS = "status"
    RESTART = "restart"
    START = "start"  # danger zone
    STOP = "stop"  # danger zone


#: Actions the UI may offer without a typed confirmation.
SAFE_ACTIONS = frozenset({ServiceAction.STATUS, ServiceAction.RESTART})


class JobState(StrEnum):
    QUEUED = "queued"
    RUNNING = "running"
    SUCCEEDED = "succeeded"
    FAILED = "failed"
    CANCELLED = "cancelled"


class GateStage(StrEnum):
    """Where a strategy instance sits on the road to live trading.

    The order is the pipeline; an instance may only advance one step at a
    time and never skips. See `oq_deck.domain.gate`.
    """

    DRAFT = "draft"
    BACKTESTED = "backtested"
    OBSERVING = "observing"  # paper / testnet
    CONFIRMED = "confirmed"  # a human signed off
    LIVE = "live"


@dataclass(frozen=True)
class Capabilities:
    """What this adapter can actually do.

    The UI renders from this rather than from a hardcoded feature list, so
    a runtime that has not grown a feature yet shows nothing instead of a
    button that errors.
    """

    kind: RuntimeKind
    version: str
    services: bool = False
    config_read: bool = False
    config_write: bool = False
    strategy_schema: bool = False
    strategy_edit: bool = False
    backtest: bool = False
    sweep: bool = False
    live_state: bool = False
    log_stream: bool = False
    #: Free-form notes the UI shows when a capability is off, e.g. why.
    notes: dict[str, str] = field(default_factory=dict)


@dataclass(frozen=True)
class Service:
    name: str  # daemon, monitor, mail, ticker, sync, ...
    status: ServiceStatus
    pid: int | None = None
    started_at: datetime | None = None
    #: Actions this service accepts. `daemon` deliberately omits START/STOP
    #: from the safe set; see `ServiceAction`.
    actions: tuple[ServiceAction, ...] = (ServiceAction.STATUS,)
    detail: str = ""


@dataclass(frozen=True)
class ActionResult:
    ok: bool
    stdout: str = ""
    stderr: str = ""
    exit_code: int | None = None


@dataclass(frozen=True)
class ParamField:
    """One strategy parameter, derived from the runtime, not hand-written."""

    name: str
    type: str  # int | float | bool | str | list
    default: Any
    #: Present when the runtime can tell us; the UI shows it in novice mode.
    description: str = ""


@dataclass(frozen=True)
class StrategyClass:
    class_name: str
    module: str
    author: str = ""
    parameters: tuple[ParamField, ...] = ()
    variables: tuple[str, ...] = ()


@dataclass(frozen=True)
class StrategyInstance:
    name: str
    class_name: str
    vt_symbol: str
    setting: dict[str, Any]
    stage: GateStage = GateStage.DRAFT
    trading: bool = False


@dataclass(frozen=True)
class ConfigDoc:
    """A configuration file as the deck sees it."""

    key: str  # stable id, e.g. "cta_strategy_setting"
    path: str  # absolute path on the runtime host
    content: dict[str, Any]
    #: sha256 of the bytes read. A write must present the etag it read, so
    #: a change made outside the deck is never silently overwritten.
    etag: str = ""


@dataclass(frozen=True)
class ConfigDiff:
    key: str
    backup_path: str
    added: dict[str, Any] = field(default_factory=dict)
    removed: dict[str, Any] = field(default_factory=dict)
    changed: dict[str, tuple[Any, Any]] = field(default_factory=dict)


@dataclass(frozen=True)
class BacktestSpec:
    strategy_class: str
    vt_symbol: str
    start: str  # ISO date
    end: str
    setting: dict[str, Any]
    #: Adapter-specific extras (rate, slippage, size, ...). Kept opaque so
    #: the contract does not have to model every runtime's engine options.
    extra: dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class JobEvent:
    at: datetime
    level: str  # info | warn | error | progress | result
    message: str
    payload: dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class Position:
    symbol: str
    direction: str
    volume: float
    price: float
    pnl: float = 0.0


@dataclass(frozen=True)
class Order:
    orderid: str
    symbol: str
    direction: str
    offset: str
    price: float
    volume: float
    traded: float
    status: str


@dataclass(frozen=True)
class LiveState:
    """Read-only view of the running system.

    Nothing here comes from a state-handover file. On 1.x that means the
    exchange API and the log, never `config/cta_strategy_data.json`, which
    is a restart-handover artefact and not a source of truth.
    """

    as_of: datetime
    positions: tuple[Position, ...] = ()
    orders: tuple[Order, ...] = ()
    balance: float | None = None
    source: str = ""  # where these numbers came from, shown in the UI
