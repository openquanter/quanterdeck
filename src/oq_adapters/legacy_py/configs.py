"""Read and write the JSON configuration of an OpenQuanter 1.x checkout.

Every write goes through `write_config`, and `write_config` always makes a
backup first. That is not a policy the caller may turn off: the backup and
the write are one operation here precisely so that no future route can do
one without the other.
"""

from __future__ import annotations

import hashlib
import json
import shutil
from datetime import datetime
from pathlib import Path
from typing import Any

from ..models import ConfigDiff, ConfigDoc
from ..protocol import AdapterError, ConflictError

#: Stable key -> filename under `config/`. The key is what the API and the
#: UI use; the filename is a 1.x detail that must not leak into either,
#: because 2.0 keeps the same settings somewhere else entirely.
CONFIG_FILES: dict[str, str] = {
    "app": "app_config.json",
    "cta_strategy_setting": "cta_strategy_setting.json",
    "risk_manager": "risk_manager_setting.json",
    "backtest": "backtest_config.json",
    "ticker": "ticker_setting.json",
    "notifications": "notifications.json",
    "export_report": "export_report_setting.json",
    "process_monitor": "process_monitor_setting.json",
}

#: Files the deck will never write, whatever the caller asks. These are
#: handover artefacts the running engine owns; editing one does not change
#: behaviour, it corrupts a restart. `cta_strategy_data.json` in
#: particular records the position ledger a restart reads back, and the
#: authority for a position is the exchange, not this file.
READ_ONLY_FILES: frozenset[str] = frozenset(
    {
        "cta_strategy_data.json",
        "cta_strategy_data_clean.json",
    }
)

BACKUP_STAMP = "%Y%m%d-%H%M"


def _config_dir(root: Path) -> Path:
    directory = root / "config"
    if not directory.is_dir():
        raise AdapterError(f"no config directory under {root}")
    return directory


def etag_of(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()[:16]


def list_configs(root: Path) -> list[str]:
    """Keys whose file actually exists in this checkout."""
    directory = _config_dir(root)
    return sorted(
        key
        for key, filename in CONFIG_FILES.items()
        if (directory / filename).is_file()
    )


def resolve(root: Path, key: str) -> Path:
    filename = CONFIG_FILES.get(key)
    if filename is None:
        raise AdapterError(f"unknown config key {key!r}")
    if filename in READ_ONLY_FILES:
        raise AdapterError(f"{filename} is engine-owned and never written")
    return _config_dir(root) / filename


def read_config(root: Path, key: str) -> ConfigDoc:
    path = resolve(root, key)
    if not path.is_file():
        raise AdapterError(f"{path} does not exist")
    raw = path.read_bytes()
    try:
        content = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise AdapterError(f"{path} is not valid JSON: {exc}") from exc
    return ConfigDoc(
        key=key, path=str(path), content=content, etag=etag_of(raw)
    )


def diff(before: dict[str, Any], after: dict[str, Any]) -> tuple[dict, dict, dict]:
    """Shallow three-way split of two config bodies.

    Shallow on purpose: a strategy's `setting` block is compared as one
    value, so the UI shows "BTC-TP-AG changed" and the detail beneath it,
    rather than forty leaf paths the reader has to reassemble.
    """
    added = {k: v for k, v in after.items() if k not in before}
    removed = {k: v for k, v in before.items() if k not in after}
    changed = {
        k: (before[k], after[k])
        for k in before.keys() & after.keys()
        if before[k] != after[k]
    }
    return added, removed, changed


def write_config(
    root: Path,
    key: str,
    content: dict[str, Any],
    *,
    etag: str,
    backup: bool = True,
    now: datetime | None = None,
) -> ConfigDiff:
    """Back up, then write. A stale etag is a conflict, never a merge."""
    path = resolve(root, key)
    if not path.is_file():
        raise AdapterError(f"{path} does not exist")

    raw = path.read_bytes()
    current = etag_of(raw)
    if etag != current:
        raise ConflictError(
            f"{path.name} changed outside the deck "
            f"(read {etag}, on disk {current}); reload before saving"
        )

    before = json.loads(raw)
    backup_path = ""
    if backup:
        stamp = (now or datetime.now()).strftime(BACKUP_STAMP)
        target = path.with_name(f"{path.name}.bak.{stamp}")
        # A second save inside the same minute must not overwrite the
        # first backup, or the older state is gone.
        serial = 1
        while target.exists():
            serial += 1
            target = path.with_name(f"{path.name}.bak.{stamp}-{serial}")
        shutil.copy2(path, target)
        backup_path = str(target)

    body = json.dumps(content, indent=4, ensure_ascii=False) + "\n"
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_text(body, encoding="utf-8")
    tmp.replace(path)  # atomic within the filesystem

    added, removed, changed = diff(before, content)
    return ConfigDiff(
        key=key,
        backup_path=backup_path,
        added=added,
        removed=removed,
        changed=changed,
    )


def restore(root: Path, key: str, backup_path: str) -> None:
    """Put a backup back, taking a backup of the current state first."""
    path = resolve(root, key)
    source = Path(backup_path)
    if not source.is_file():
        raise AdapterError(f"no such backup: {backup_path}")
    if source.parent != path.parent:
        raise AdapterError("a backup may only be restored from config/")
    doc = read_config(root, key)
    write_config(root, key, json.loads(source.read_text()), etag=doc.etag)
