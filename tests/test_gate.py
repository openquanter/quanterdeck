from datetime import UTC, datetime, timedelta

from oq_adapters import GateStage
from oq_deck.domain.gate import (
    GateEvidence,
    can_advance,
    invalidated_by_settings_change,
)

NOW = datetime(2026, 9, 8, 12, 0, tzinfo=UTC)


def test_draft_cannot_advance_without_a_backtest():
    decision = can_advance(GateStage.DRAFT, GateEvidence(), now=NOW)
    assert not decision.allowed
    assert "no backtest" in decision.reason


def test_a_failed_backtest_does_not_count():
    evidence = GateEvidence(backtest_job_id="job-1", backtest_passed=False)
    decision = can_advance(GateStage.DRAFT, evidence, now=NOW)
    assert not decision.allowed
    assert "did not pass" in decision.reason


def test_observation_window_must_elapse():
    evidence = GateEvidence(
        observation_started_at=NOW - timedelta(days=1), observation_trades=10
    )
    decision = can_advance(GateStage.OBSERVING, evidence, now=NOW)
    assert not decision.allowed
    assert "observation window remain" in decision.reason


def test_observation_with_no_trades_proves_nothing():
    evidence = GateEvidence(
        observation_started_at=NOW - timedelta(days=5), observation_trades=0
    )
    decision = can_advance(GateStage.OBSERVING, evidence, now=NOW)
    assert not decision.allowed
    assert "trades" in decision.reason


def test_a_completed_observation_may_be_confirmed():
    evidence = GateEvidence(
        observation_started_at=NOW - timedelta(days=5), observation_trades=12
    )
    assert can_advance(GateStage.OBSERVING, evidence, now=NOW).allowed


def test_live_needs_a_human():
    assert not can_advance(GateStage.CONFIRMED, GateEvidence(), now=NOW).allowed
    signed = GateEvidence(confirmed_by="operator", confirmed_at=NOW)
    assert can_advance(GateStage.CONFIRMED, signed, now=NOW).allowed


def test_changing_the_parameters_voids_the_evidence():
    evidence = GateEvidence(settings_fingerprint="abc")
    assert invalidated_by_settings_change(GateStage.LIVE, evidence, "def")
    assert not invalidated_by_settings_change(GateStage.LIVE, evidence, "abc")
