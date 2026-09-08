"""The OpenQuanter 2.0 adapter — a placeholder with a real shape.

The 2.0 runtime is reachable from Python in two ways, and this adapter
will use both:

* the `oq` command, which execs `oq-<tool>` (capture, ingest, data,
  parity, book-check, ...), and
* the `oq-py` wheel, which exposes the overfitting statistics and the
  tick reader as a native extension.

What is missing is a machine-readable output contract: `oq` today prints
for a human, and parsing that would make the deck break on a wording
change. The upstream work is a `--json` flag on the tools the deck reads,
tracked as an issue rather than worked around here. Until then this
adapter reports honest capabilities — almost all off — and the UI renders
nothing rather than something broken.
"""

from __future__ import annotations

import shutil
from pathlib import Path
from typing import Any

from ..models import (
    ActionResult,
    BacktestSpec,
    Capabilities,
    ConfigDiff,
    ConfigDoc,
    LiveState,
    RuntimeKind,
    Service,
    ServiceAction,
    StrategyClass,
    StrategyInstance,
)
from ..protocol import NotSupported


class OqCliAdapter:
    kind = RuntimeKind.OQ_CLI

    def __init__(self, root: str | Path, *, oq_binary: str = "oq") -> None:
        self.root = Path(root).expanduser().resolve()
        self.oq = shutil.which(oq_binary) or oq_binary

    def capabilities(self) -> Capabilities:
        found = shutil.which(self.oq) is not None
        return Capabilities(
            kind=self.kind,
            version="unknown",
            services=False,
            config_read=False,
            config_write=False,
            strategy_schema=False,
            strategy_edit=False,
            backtest=False,
            sweep=False,
            live_state=False,
            log_stream=False,
            notes={
                "*": (
                    "the oq tools print for a human, not for a program; "
                    "the deck needs --json before it can read them"
                )
                if found
                else "the `oq` command was not found on PATH",
            },
        )

    def list_services(self) -> list[Service]:
        raise NotSupported("2.0 has no process manager the deck drives")

    def service_action(
        self, name: str, action: ServiceAction, *, confirmed: bool = False
    ) -> ActionResult:
        raise NotSupported("2.0 has no process manager the deck drives")

    def list_strategy_classes(self) -> list[StrategyClass]:
        raise NotSupported("awaiting a structured strategy listing")

    def list_strategy_instances(self) -> list[StrategyInstance]:
        raise NotSupported("awaiting a structured strategy listing")

    def list_configs(self) -> list[str]:
        raise NotSupported("awaiting the 2.0 configuration contract")

    def read_config(self, key: str) -> ConfigDoc:
        raise NotSupported("awaiting the 2.0 configuration contract")

    def write_config(
        self, key: str, content: dict[str, Any], *, etag: str, backup: bool = True
    ) -> ConfigDiff:
        raise NotSupported("awaiting the 2.0 configuration contract")

    def submit_backtest(self, spec: BacktestSpec) -> str:
        raise NotSupported("awaiting `oq backtest --json`")

    def job_events(self, job_id: str):
        raise NotSupported("awaiting `oq backtest --json`")

    def live_state(self) -> LiveState:
        raise NotSupported("awaiting a journal reader binding")

    def tail_log(self, service: str, *, lines: int = 200):
        raise NotSupported("awaiting a journal reader binding")
