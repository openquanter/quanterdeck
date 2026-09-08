"""Deck configuration, and the guards that run before it is accepted."""

from __future__ import annotations

import ipaddress
import os
from dataclasses import dataclass, field
from pathlib import Path


class ConfigurationError(Exception):
    """The deck will not start with this configuration, and why."""


def _default_state_dir() -> Path:
    return Path(
        os.environ.get("OQ_DECK_STATE_DIR")
        or Path.home() / ".local" / "state" / "quanterdeck"
    )


@dataclass
class Settings:
    """Everything the deck needs to come up.

    Defaults are the safe ones. Reaching an unsafe configuration takes a
    deliberate act with a name attached, and `validate()` refuses the
    combinations that would put a trading console on a network with no
    authentication.
    """

    #: Loopback by default. A console that can move money does not get a
    #: convenient default that exposes it.
    host: str = "127.0.0.1"
    port: int = 8899

    runtime_kind: str = "legacy_py"
    runtime_root: Path | None = None
    #: The interpreter the *runtime* uses. Left empty, the deck's own is
    #: used, which is right for a developer checkout and wrong for a box
    #: where the runtime lives in its own conda environment.
    runtime_python: str = ""

    state_dir: Path = field(default_factory=_default_state_dir)

    #: Set once, at setup. Empty means nobody has completed setup yet, and
    #: every route except /setup and /health returns 428.
    password_hash: str = ""
    totp_secret: str = ""

    #: Off by default; every mutating route is refused while it is off.
    #: The wizard turns it on once the operator has read what it means.
    allow_writes: bool = False

    dev_mode: bool = False

    @property
    def db_path(self) -> Path:
        return self.state_dir / "deck.sqlite3"

    @property
    def is_loopback(self) -> bool:
        try:
            return ipaddress.ip_address(self.host).is_loopback
        except ValueError:
            return self.host in {"localhost", ""}

    def validate(self) -> None:
        if not self.is_loopback and not self.password_hash:
            raise ConfigurationError(
                f"refusing to listen on {self.host} without a password. "
                "Either bind 127.0.0.1, or run `oq-deck set-password` "
                "first. A console that can place orders is not put on a "
                "network unauthenticated."
            )
        if not self.is_loopback and not self.totp_secret and not self.dev_mode:
            raise ConfigurationError(
                f"refusing to listen on {self.host} without a second "
                "factor. Run `oq-deck set-totp`, or bind 127.0.0.1."
            )
        if self.runtime_root is not None and not self.runtime_root.is_dir():
            raise ConfigurationError(
                f"runtime_root {self.runtime_root} is not a directory"
            )

    @classmethod
    def from_env(cls) -> Settings:
        root = os.environ.get("OQ_DECK_RUNTIME_ROOT")
        settings = cls(
            host=os.environ.get("OQ_DECK_HOST", "127.0.0.1"),
            port=int(os.environ.get("OQ_DECK_PORT", "8899")),
            runtime_kind=os.environ.get("OQ_DECK_RUNTIME", "legacy_py"),
            runtime_root=Path(root).expanduser() if root else None,
            runtime_python=os.environ.get("OQ_DECK_RUNTIME_PYTHON", ""),
            password_hash=os.environ.get("OQ_DECK_PASSWORD_HASH", ""),
            totp_secret=os.environ.get("OQ_DECK_TOTP_SECRET", ""),
            allow_writes=os.environ.get("OQ_DECK_ALLOW_WRITES") == "1",
            dev_mode=os.environ.get("OQ_DECK_DEV") == "1",
        )
        settings.validate()
        return settings
