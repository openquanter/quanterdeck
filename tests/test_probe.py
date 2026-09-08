from oq_adapters.legacy_py import LegacyPyAdapter


def test_parameters_come_from_the_runtime(runtime):
    adapter = LegacyPyAdapter(runtime)
    classes = {c.class_name: c for c in adapter.list_strategy_classes()}

    assert "DemoGridStrategy" in classes
    grid = classes["DemoGridStrategy"]
    names = [p.name for p in grid.parameters]
    assert names == [
        "grid_step",
        "grid_levels",
        "order_volume",
        "use_trailing",
        "label",
    ]

    by_name = {p.name: p for p in grid.parameters}
    assert by_name["grid_step"].default == 500.0
    assert by_name["grid_levels"].type == "int"
    assert by_name["use_trailing"].type == "bool"


def test_a_strategy_that_will_not_import_does_not_hide_the_others(runtime):
    adapter = LegacyPyAdapter(runtime)
    classes = adapter.list_strategy_classes()
    assert any(c.class_name == "DemoGridStrategy" for c in classes)


def test_capabilities_report_what_this_checkout_actually_has(runtime):
    caps = LegacyPyAdapter(runtime).capabilities()
    assert caps.services is True
    assert caps.config_write is True
    assert caps.live_state is False
    assert caps.version == "2.2.7"


def test_instances_are_read_from_the_setting_file(runtime):
    instances = LegacyPyAdapter(runtime).list_strategy_instances()
    assert [i.name for i in instances] == ["DEMO-GRID"]
    assert instances[0].vt_symbol == "BTCUSDT.BINANCE"
