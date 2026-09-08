from pathlib import Path

import pytest

FIXTURE = Path(__file__).resolve().parents[1] / "examples" / "fixtures" / "fake_runtime"


@pytest.fixture
def runtime(tmp_path: Path) -> Path:
    """A throwaway copy of the fake 1.x checkout.

    Copied per test because half of these tests write to it, and a test
    that leaves a `.bak.` file behind would change what the next one sees.
    """
    import shutil

    target = tmp_path / "runtime"
    shutil.copytree(FIXTURE, target)
    return target
