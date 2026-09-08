from core.app.cta_strategy.template import CtaTemplate


class DemoGridStrategy(CtaTemplate):
    """A toy strategy that exists so the tests have something to find."""

    author = "quanterdeck"

    grid_step = 500.0
    grid_levels = 5
    order_volume = 0.001
    use_trailing = False
    label = "demo"

    parameters = [
        "grid_step",
        "grid_levels",
        "order_volume",
        "use_trailing",
        "label",
    ]
    variables = ["pos", "avg_price"]
