"""The road from a draft strategy to a live one.

This is the part of the deck that exists to say no. Every rule below was
paid for somewhere: a parameter that went to production without a
backtest, a restart that cancelled resting orders, a config edited with no
copy of what it had been. Writing them down does not prevent them; the
button being unavailable does.

The stages advance one at a time and never skip. `can_advance` returns the
reason a step is unavailable, and the UI shows that reason instead of a
disabled control with no explanation.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC, datetime, timedelta

from oq_adapters import GateStage

ORDER: tuple[GateStage, ...] = (
    GateStage.DRAFT,
    GateStage.BACKTESTED,
    GateStage.OBSERVING,
    GateStage.CONFIRMED,
    GateStage.LIVE,
)

#: How long an instance must run on paper or testnet before it may be
#: confirmed. Configurable per deployment, never zero.
DEFAULT_OBSERVATION = timedelta(days=3)


@dataclass(frozen=True)
class GateEvidence:
    """What the deck knows about one strategy instance's progress."""

    backtest_job_id: str | None = None
    backtest_passed: bool = False
    observation_started_at: datetime | None = None
    observation_trades: int = 0
    confirmed_by: str | None = None
    confirmed_at: datetime | None = None
    #: Set when the instance's settings changed after evidence was
    #: gathered. Any change to the parameters invalidates the backtest
    #: that justified them, so the instance falls back to DRAFT.
    settings_fingerprint: str = ""


@dataclass(frozen=True)
class GateDecision:
    allowed: bool
    reason: str = ""


def next_stage(current: GateStage) -> GateStage | None:
    index = ORDER.index(current)
    if index + 1 >= len(ORDER):
        return None
    return ORDER[index + 1]


def can_advance(
    current: GateStage,
    evidence: GateEvidence,
    *,
    now: datetime | None = None,
    observation: timedelta = DEFAULT_OBSERVATION,
    min_trades: int = 1,
) -> GateDecision:
    """Whether this instance may take exactly one step forward."""
    now = now or datetime.now(UTC)
    target = next_stage(current)
    if target is None:
        return GateDecision(False, "already live")

    if target is GateStage.BACKTESTED:
        if not evidence.backtest_job_id:
            return GateDecision(
                False, "no backtest has been run for these parameters"
            )
        if not evidence.backtest_passed:
            return GateDecision(
                False,
                f"backtest {evidence.backtest_job_id} did not pass; "
                "review the result before advancing",
            )
        return GateDecision(True)

    if target is GateStage.OBSERVING:
        return GateDecision(True)

    if target is GateStage.CONFIRMED:
        started = evidence.observation_started_at
        if started is None:
            return GateDecision(False, "observation has not started")
        elapsed = now - started
        if elapsed < observation:
            remaining = observation - elapsed
            hours = int(remaining.total_seconds() // 3600)
            return GateDecision(
                False,
                f"{hours}h of the observation window remain "
                f"({observation.days}d required)",
            )
        if evidence.observation_trades < min_trades:
            return GateDecision(
                False,
                f"observation produced {evidence.observation_trades} "
                f"trades; at least {min_trades} is required to say the "
                "strategy did anything at all",
            )
        return GateDecision(True)

    if target is GateStage.LIVE:
        if not evidence.confirmed_by:
            return GateDecision(False, "nobody has signed off")
        return GateDecision(True)

    return GateDecision(False, f"no rule for {target}")


def invalidated_by_settings_change(
    stage: GateStage, evidence: GateEvidence, fingerprint: str
) -> bool:
    """True when a parameter edit has voided the evidence behind a stage.

    A backtest justifies the parameters it ran with, and nothing else. If
    the parameters move, the instance goes back to DRAFT — including one
    that was already live, whose operator then has to walk it through
    again. That is the intended cost.
    """
    if stage is GateStage.DRAFT:
        return False
    if not evidence.settings_fingerprint:
        return False
    return evidence.settings_fingerprint != fingerprint
