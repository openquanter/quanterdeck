import json

import pytest

from oq_adapters.legacy_py import configs
from oq_adapters.protocol import AdapterError, ConflictError


def test_lists_only_files_that_exist(runtime):
    keys = configs.list_configs(runtime)
    assert "cta_strategy_setting" in keys
    assert "app" in keys
    assert "ticker" not in keys  # the fixture has no ticker_setting.json


def test_read_carries_an_etag(runtime):
    doc = configs.read_config(runtime, "cta_strategy_setting")
    assert doc.content["DEMO-GRID"]["class_name"] == "DemoGridStrategy"
    assert len(doc.etag) == 16


def test_write_backs_up_first(runtime):
    doc = configs.read_config(runtime, "cta_strategy_setting")
    body = dict(doc.content)
    body["DEMO-GRID"]["setting"]["grid_step"] = 750.0

    diff = configs.write_config(
        runtime, "cta_strategy_setting", body, etag=doc.etag
    )

    backup = runtime / "config" / diff.backup_path.rsplit("/", 1)[-1]
    assert backup.is_file(), "a write must leave the previous state behind"
    assert json.loads(backup.read_text())["DEMO-GRID"]["setting"][
        "grid_step"
    ] == 500.0
    assert "DEMO-GRID" in diff.changed


def test_two_writes_in_one_minute_keep_both_backups(runtime):
    for step in (750.0, 900.0):
        doc = configs.read_config(runtime, "cta_strategy_setting")
        body = dict(doc.content)
        body["DEMO-GRID"]["setting"]["grid_step"] = step
        configs.write_config(
            runtime, "cta_strategy_setting", body, etag=doc.etag
        )

    backups = list((runtime / "config").glob("cta_strategy_setting.json.bak.*"))
    assert len(backups) == 2, "the second write must not clobber the first backup"


def test_stale_etag_is_a_conflict_not_a_merge(runtime):
    doc = configs.read_config(runtime, "cta_strategy_setting")
    # Someone edits the file outside the deck.
    path = runtime / "config" / "cta_strategy_setting.json"
    path.write_text(json.dumps({"OTHER": {}}), encoding="utf-8")

    with pytest.raises(ConflictError):
        configs.write_config(runtime, "cta_strategy_setting", {}, etag=doc.etag)


def test_engine_owned_files_are_not_writable(runtime):
    with pytest.raises(AdapterError):
        configs.resolve(runtime, "cta_strategy_data")
