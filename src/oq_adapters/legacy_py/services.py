"""Drive the 1.x services through `manager.sh`, and only through it.

`manager.sh <service> <action>` is the single entry point the runtime
supports. The per-tool shell scripts next to it can start a process but
cannot correctly restart or stop one, so this module never calls them: the
command is assembled from a fixed template and a whitelist, and there is
no path by which caller-supplied text reaches a shell.
"""

from __future__ import annotations

import re
import subprocess
from datetime import datetime
from pathlib import Path

from ..models import ActionResult, Service, ServiceAction, ServiceStatus
from ..protocol import AdapterError, RefusedError

#: The services `manager.sh` knows, in the order the overview shows them.
SERVICES: tuple[str, ...] = (
    "daemon",
    "monitor",
    "mail",
    "ticker",
    "sync",
    "ticker_hyper",
    "sync_hyper",
    "ticker_aster",
    "sync_aster",
    "market_monitor",
)

#: Services whose stop/start carries a consequence the operator must be
#: told about, mapped to what that consequence is. The UI puts these
#: behind a typed confirmation and shows the text verbatim.
DESTRUCTIVE_STOP: dict[str, str] = {
    "daemon": (
        "Stopping the trading daemon cancels every resting order and drops "
        "order ownership; positions opened by the strategy come back as "
        "unmanaged. Use Restart, which signals the daemon to hand its "
        "orders over instead."
    ),
}

_TIMEOUT = 30
_PID = re.compile(r"\bPID[:\s]+(\d+)", re.IGNORECASE)
_RUNNING = re.compile(r"运行中|running|active", re.IGNORECASE)
_STOPPED = re.compile(r"未运行|已停止|stopped|not running", re.IGNORECASE)


def manager_path(root: Path) -> Path:
    path = root / "manager.sh"
    if not path.is_file():
        raise AdapterError(f"no manager.sh under {root}")
    return path


def actions_for(service: str) -> tuple[ServiceAction, ...]:
    """What the UI may offer for this service.

    STOP and START are present for every service so the danger zone can
    render them, but `run` refuses both without an explicit confirmation.
    """
    return (
        ServiceAction.STATUS,
        ServiceAction.RESTART,
        ServiceAction.START,
        ServiceAction.STOP,
    )


def _parse_status(service: str, stdout: str) -> Service:
    status = ServiceStatus.UNKNOWN
    if _RUNNING.search(stdout):
        status = ServiceStatus.RUNNING
    elif _STOPPED.search(stdout):
        status = ServiceStatus.STOPPED

    pid = None
    match = _PID.search(stdout)
    if match:
        pid = int(match.group(1))

    return Service(
        name=service,
        status=status,
        pid=pid,
        actions=actions_for(service),
        detail=stdout.strip()[:2000],
    )


def _run(root: Path, service: str, action: str) -> subprocess.CompletedProcess:
    script = manager_path(root)
    # A list, never a string, and never `shell=True`: the only values that
    # reach this call are the two whitelisted constants above.
    return subprocess.run(
        [str(script), service, action],
        cwd=str(root),
        capture_output=True,
        text=True,
        timeout=_TIMEOUT,
        check=False,
    )


def status(root: Path, service: str) -> Service:
    if service not in SERVICES:
        raise AdapterError(f"unknown service {service!r}")
    proc = _run(root, service, "status")
    return _parse_status(service, proc.stdout + proc.stderr)


def list_services(root: Path) -> list[Service]:
    return [status(root, name) for name in SERVICES]


def run(
    root: Path,
    service: str,
    action: ServiceAction,
    *,
    confirmed: bool = False,
) -> ActionResult:
    """Perform one whitelisted action.

    `restart` is passed through to `manager.sh`, which signals the process
    rather than replacing it. This function will not emulate a restart as
    stop-then-start under any circumstances, because the two differ in
    what happens to the open orders.
    """
    if service not in SERVICES:
        raise AdapterError(f"unknown service {service!r}")
    if action in (ServiceAction.STOP, ServiceAction.START) and not confirmed:
        reason = DESTRUCTIVE_STOP.get(
            service, f"{action.value} on {service} requires confirmation"
        )
        raise RefusedError(reason)

    proc = _run(root, service, action.value)
    return ActionResult(
        ok=proc.returncode == 0,
        stdout=proc.stdout,
        stderr=proc.stderr,
        exit_code=proc.returncode,
    )


def tail_log(root: Path, service: str, *, lines: int = 200):
    """Yield the tail of today's log for a service.

    Reading, not following: the streaming endpoint polls this. A `tail -f`
    child per browser tab is a process leak waiting to happen.
    """
    log_dir = root / "data" / "logs"
    if not log_dir.is_dir():
        raise AdapterError(f"no log directory under {root}")
    stamp = datetime.now().strftime("%Y%m%d")
    matches = sorted(log_dir.glob(f"*{service}*{stamp}*")) or sorted(
        log_dir.glob(f"*{service}*")
    )
    if not matches:
        return
    with matches[-1].open("r", encoding="utf-8", errors="replace") as handle:
        tail = handle.readlines()[-lines:]
    yield from (line.rstrip("\n") for line in tail)
