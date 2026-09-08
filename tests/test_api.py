import pytest
from fastapi.testclient import TestClient

from oq_deck.api.deps import get_adapter, get_settings
from oq_deck.app import create_app
from oq_deck.settings import Settings


@pytest.fixture
def client(runtime):
    from oq_adapters.legacy_py import LegacyPyAdapter

    app = create_app()
    settings = Settings(runtime_root=runtime, allow_writes=True)
    app.dependency_overrides[get_settings] = lambda: settings
    app.dependency_overrides[get_adapter] = lambda: LegacyPyAdapter(runtime)
    return TestClient(app)


@pytest.fixture
def readonly_client(runtime):
    from oq_adapters.legacy_py import LegacyPyAdapter

    app = create_app()
    settings = Settings(runtime_root=runtime, allow_writes=False)
    app.dependency_overrides[get_settings] = lambda: settings
    app.dependency_overrides[get_adapter] = lambda: LegacyPyAdapter(runtime)
    return TestClient(app)


def test_health_says_nothing_about_the_runtime(client):
    body = client.get("/api/v1/health").json()
    assert body["status"] == "ok"
    assert "runtime" not in body


def test_capabilities_drive_the_ui(client):
    caps = client.get("/api/v1/runtime/capabilities").json()
    assert caps["services"] is True
    assert caps["live_state"] is False
    assert "live_state" in caps["notes"]


def test_services_are_listed(client):
    services = client.get("/api/v1/services").json()
    names = [s["name"] for s in services]
    assert "daemon" in names and "ticker" in names


def test_stopping_the_daemon_returns_the_consequence(client):
    response = client.post("/api/v1/services/daemon/stop", json={"confirm": ""})
    assert response.status_code == 409
    detail = response.json()["detail"]
    assert detail["confirmation_required"] == "daemon"
    assert "resting order" in detail["consequence"]


def test_typing_the_name_lets_it_through(client):
    response = client.post(
        "/api/v1/services/daemon/stop", json={"confirm": "daemon"}
    )
    assert response.status_code == 200


def test_read_only_deck_refuses_every_write(readonly_client):
    response = readonly_client.post(
        "/api/v1/services/daemon/restart", json={"confirm": "daemon"}
    )
    assert response.status_code == 403
    assert "read-only" in response.json()["detail"]


def test_a_stale_write_is_refused(client):
    response = client.put(
        "/api/v1/configs/cta_strategy_setting",
        json={"content": {}, "etag": "nottheetag"},
    )
    assert response.status_code == 412


def test_deep_links_reach_the_single_page_app(client):
    """A history-routed URL must survive a reload.

    `/services` has no file behind it; the router owns it. Answering
    404 there means every bookmark and every refresh breaks.
    """
    response = client.get("/services")
    assert response.status_code == 200
    assert "<div id=\"root\">" in response.text


def test_a_missing_asset_is_still_a_404(client):
    """The fallback must not hand back HTML under a .js name."""
    assert client.get("/assets/gone-BXXXXXXX.js").status_code == 404
