"""The OpenQuanter 1.x adapter."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from collections.abc import Iterator
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from ..models import (
    ActionResult,
    BacktestSpec,
    Capabilities,
    ConfigDiff,
    ConfigDoc,
    GateStage,
    LiveState,
    ParamField,
    RuntimeKind,
    Service,
    ServiceAction,
    StrategyClass,
    StrategyInstance,
)
from ..protocol import AdapterError, NotSupported
from . import configs, services

PROBE = Path(__file__).with_name("probe.py")
PROBE_TIMEOUT = 60


class LegacyPyAdapter:
    """Talks to a 1.x checkout on the local filesystem.

    `python_executable` is the interpreter the *runtime* uses, which is
    usually not the one running this code — a production box may be on
    3.9 while the deck is on 3.12. Nothing from the runtime is ever
    imported into this process; see `probe.py` for why.
    """

    kind = RuntimeKind.LEGACY_PY

    def __init__(
        self,
        root: str | Path,
        *,
        python_executable: str | None = None,
    ) -> None:
        self.root = Path(root).expanduser().resolve()
        if not self.root.is_dir():
            raise AdapterError(f"no such directory: {self.root}")
        self.python = python_executable or sys.executable

    # -- identity -------------------------------------------------------
    def capabilities(self) -> Capabilities:
        notes: dict[str, str] = {}
        has_manager = (self.root / "manager.sh").is_file()
        if not has_manager:
            notes["services"] = "no manager.sh in this checkout"
        has_config = (self.root / "config").is_dir()
        if not has_config:
            notes["config_read"] = "no config/ directory in this checkout"
        return Capabilities(
            kind=self.kind,
            version=self._version(),
            services=has_manager,
            config_read=has_config,
            config_write=has_config,
            strategy_schema=True,
            strategy_edit=True,
            backtest=(self.root / "tools" / "backtest").is_dir(),
            sweep=False,
            live_state=False,
            log_stream=(self.root / "data" / "logs").is_dir(),
            notes=notes
            | {
                "sweep": "not wired yet; the runner exists but has no "
                "structured result contract",
                "live_state": "reads from the exchange API, which needs "
                "credentials the deck has not been given",
            },
        )

    def _version(self) -> str:
        init = self.root / "core" / "__init__.py"
        if init.is_file():
            for line in init.read_text(encoding="utf-8").splitlines():
                if line.startswith("__version__"):
                    return line.split("=", 1)[1].strip().strip("\"'")
        return "unknown"

    # -- processes ------------------------------------------------------
    def list_services(self) -> list[Service]:
        return services.list_services(self.root)

    def service_action(
        self,
        name: str,
        action: ServiceAction,
        *,
        confirmed: bool = False,
    ) -> ActionResult:
        return services.run(self.root, name, action, confirmed=confirmed)

    def tail_log(self, service: str, *, lines: int = 200) -> Iterator[str]:
        return services.tail_log(self.root, service, lines=lines)

    # -- strategies -----------------------------------------------------
    def list_strategy_classes(self) -> list[StrategyClass]:
        result = self._probe()
        classes = []
        for item in result.get("classes", []):
            classes.append(
                StrategyClass(
                    class_name=item["class_name"],
                    module=item["module"],
                    author=item.get("author", ""),
                    parameters=tuple(
                        ParamField(
                            name=field["name"],
                            type=field["type"],
                            default=field["default"],
                            description=field.get("description", ""),
                        )
                        for field in item.get("parameters", [])
                    ),
                    variables=tuple(item.get("variables", [])),
                )
            )
        return classes

    def _probe(self) -> dict[str, Any]:
        """Run the probe and read its answer out of a file.

        Not out of stdout: importing 1.x prints a configuration banner
        first, so stdout carries the runtime's noise and the answer needs
        a channel the runtime does not write to. stdout and stderr are
        still captured, because when the probe fails they are the only
        explanation there is.
        """
        with tempfile.TemporaryDirectory(prefix="oq-deck-probe-") as tmp:
            out_path = Path(tmp) / "probe.json"
            proc = subprocess.run(
                [self.python, str(PROBE), str(self.root), str(out_path)],
                capture_output=True,
                text=True,
                timeout=PROBE_TIMEOUT,
                cwd=str(self.root),
                check=False,
            )
            if not out_path.is_file():
                raise AdapterError(
                    f"probe wrote no result (exit {proc.returncode}): "
                    f"{(proc.stderr or proc.stdout).strip()[-400:]}"
                )
            result = json.loads(out_path.read_text())

        if not result.get("ok"):
            raise AdapterError(result.get("error", "probe failed"))
        return result

    def list_strategy_instances(self) -> list[StrategyInstance]:
        try:
            doc = configs.read_config(self.root, "cta_strategy_setting")
        except AdapterError:
            return []
        instances = []
        for name, body in doc.content.items():
            if not isinstance(body, dict):
                continue
            instances.append(
                StrategyInstance(
                    name=name,
                    class_name=body.get("class_name", ""),
                    vt_symbol=body.get("vt_symbol", ""),
                    setting=body.get("setting", {}),
                    # The file records what to run, not how far it has
                    # come through the gate. The stage lives in the deck's
                    # own store and is joined on in the API layer.
                    stage=GateStage.DRAFT,
                )
            )
        return instances

    # -- configuration --------------------------------------------------
    def list_configs(self) -> list[str]:
        return configs.list_configs(self.root)

    def read_config(self, key: str) -> ConfigDoc:
        return configs.read_config(self.root, key)

    def write_config(
        self,
        key: str,
        content: dict[str, Any],
        *,
        etag: str,
        backup: bool = True,
    ) -> ConfigDiff:
        return configs.write_config(
            self.root, key, content, etag=etag, backup=backup
        )

    # -- research -------------------------------------------------------
    def submit_backtest(self, spec: BacktestSpec) -> str:
        raise NotSupported(
            "backtest submission arrives in M2; the runner has no "
            "structured job contract yet"
        )

    def job_events(self, job_id: str):
        raise NotSupported("no jobs to report on until M2")

    # -- live -----------------------------------------------------------
    def live_state(self) -> LiveState:
        # Deliberately not implemented by reading a state file. The
        # authority for a position is the exchange; a handover artefact on
        # disk is neither current nor complete, and a console that showed
        # it as truth would be wrong exactly when it mattered.
        raise NotSupported(
            "live state comes from the exchange API, which needs "
            "credentials configured under Exchanges first"
        )

    def now(self) -> datetime:
        return datetime.now(UTC)
