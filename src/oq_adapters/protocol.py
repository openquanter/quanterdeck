"""The contract between the deck and a runtime.

Deliberately small. Every method here is something the UI needs; nothing
here exists because a particular runtime happens to offer it. When a
runtime offers more, it goes behind `Capabilities` and an adapter-specific
route, not into this protocol.

Two rules constrain every implementation:

1. **A reader never changes the runtime.** Anything that reads state must
   be free of side effects on the running system. The 2.0 requirement
   that journal readers are observers (FR-CORE-7) is the same rule.
2. **A writer never guesses.** `write_config` takes the etag the caller
   read; a mismatch is a conflict, not a merge.
"""

from __future__ import annotations

from collections.abc import Iterator
from typing import Any, Protocol, runtime_checkable

from .models import (
    ActionResult,
    BacktestSpec,
    Capabilities,
    ConfigDiff,
    ConfigDoc,
    JobEvent,
    LiveState,
    Service,
    ServiceAction,
    StrategyClass,
    StrategyInstance,
)


class AdapterError(Exception):
    """Base for every failure an adapter reports to the deck."""


class NotSupported(AdapterError):
    """The runtime cannot do this. The UI should not have offered it."""


class ConflictError(AdapterError):
    """The file changed underneath us; the caller's etag is stale."""


class RefusedError(AdapterError):
    """The adapter declined on purpose, and `reason` says why.

    Used for the rules that must not be overridable from the UI, such as
    an action that would cancel resting orders being requested without
    the explicit danger-zone confirmation.
    """

    def __init__(self, reason: str) -> None:
        super().__init__(reason)
        self.reason = reason


@runtime_checkable
class RuntimeAdapter(Protocol):
    """One OpenQuanter installation, as the deck sees it."""

    # -- identity -------------------------------------------------------
    def capabilities(self) -> Capabilities: ...

    # -- processes ------------------------------------------------------
    def list_services(self) -> list[Service]: ...

    def service_action(
        self,
        name: str,
        action: ServiceAction,
        *,
        confirmed: bool = False,
    ) -> ActionResult:
        """Act on a service.

        `confirmed` is required for anything outside `SAFE_ACTIONS`, and
        an adapter must raise `RefusedError` rather than proceed without
        it. The flag is set only by a route that received a typed
        confirmation from the operator.
        """
        ...

    # -- strategies -----------------------------------------------------
    def list_strategy_classes(self) -> list[StrategyClass]:
        """Discover strategy classes and their declared parameters.

        The parameter list is read from the runtime, never maintained
        here; a form the deck renders is therefore always in step with
        the code that will run.
        """
        ...

    def list_strategy_instances(self) -> list[StrategyInstance]: ...

    # -- configuration --------------------------------------------------
    def list_configs(self) -> list[str]: ...

    def read_config(self, key: str) -> ConfigDoc: ...

    def write_config(
        self,
        key: str,
        content: dict[str, Any],
        *,
        etag: str,
        backup: bool = True,
    ) -> ConfigDiff:
        """Write a config, after backing the current one up.

        `backup=False` exists for tests only. A caller in the server
        never passes it; see `oq_deck.api.v1.configs`.
        """
        ...

    # -- research -------------------------------------------------------
    def submit_backtest(self, spec: BacktestSpec) -> str: ...

    def job_events(self, job_id: str) -> Iterator[JobEvent]: ...

    # -- live -----------------------------------------------------------
    def live_state(self) -> LiveState: ...

    def tail_log(self, service: str, *, lines: int = 200) -> Iterator[str]: ...
