import pytest

from oq_adapters import ServiceAction, ServiceStatus
from oq_adapters.legacy_py import services
from oq_adapters.protocol import AdapterError, RefusedError


def test_status_is_parsed_from_manager_output(runtime):
    service = services.status(runtime, "daemon")
    assert service.status is ServiceStatus.RUNNING
    assert service.pid == 4242


def test_restart_needs_no_confirmation(runtime):
    result = services.run(runtime, "daemon", ServiceAction.RESTART)
    assert result.ok
    assert "SIGUSR1" in result.stdout


def test_stopping_the_daemon_is_refused_without_confirmation(runtime):
    with pytest.raises(RefusedError) as caught:
        services.run(runtime, "daemon", ServiceAction.STOP)
    # The refusal must say what would have happened, not just "denied".
    assert "resting order" in caught.value.reason


def test_stopping_the_daemon_proceeds_once_confirmed(runtime):
    result = services.run(
        runtime, "daemon", ServiceAction.STOP, confirmed=True
    )
    assert result.ok


def test_unknown_service_is_rejected_before_any_process_starts(runtime):
    # The whitelist, not quoting, is what makes this safe: the name never
    # reaches a shell because it never gets past this check.
    with pytest.raises(AdapterError):
        services.run(runtime, "; rm -rf /", ServiceAction.STATUS)
