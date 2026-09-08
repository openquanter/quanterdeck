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


@pytest.fixture
def web_client(runtime, tmp_path, monkeypatch):
    """A client with a stand-in for the built web app.

    The real bundle is a build artefact that is not in git, so a test
    that depended on it would pass on a developer's machine and fail in
    CI — which is exactly what happened the first time these two tests
    ran. What is under test here is the routing, not the bundle, so the
    bundle is faked and the test says the same thing everywhere.
    """
    from oq_adapters.legacy_py import LegacyPyAdapter
    from oq_deck import app as app_module

    dist = tmp_path / "web"
    (dist / "assets").mkdir(parents=True)
    (dist / "index.html").write_text('<div id="root"></div>', encoding="utf-8")
    (dist / "assets" / "index-AAAA1111.js").write_text("//", encoding="utf-8")
    monkeypatch.setattr(app_module, "WEB_DIST", dist)

    app = app_module.create_app()
    settings = Settings(runtime_root=runtime)
    app.dependency_overrides[get_settings] = lambda: settings
    app.dependency_overrides[get_adapter] = lambda: LegacyPyAdapter(runtime)
    return TestClient(app)


def test_deep_links_reach_the_single_page_app(web_client):
    """A history-routed URL must survive a reload.

    `/services` has no file behind it; the router owns it. Answering
    404 there means every bookmark and every refresh breaks.
    """
    for path in ("/services", "/backtests/42", "/strategies/BTC-AG/edit"):
        response = web_client.get(path)
        assert response.status_code == 200, path
        assert '<div id="root">' in response.text


def test_a_real_asset_is_served_as_itself(web_client):
    assert web_client.get("/assets/index-AAAA1111.js").status_code == 200


def test_a_missing_asset_is_still_a_404(web_client):
    """The fallback must not hand back HTML under a .js name."""
    assert web_client.get("/assets/gone-BXXXXXXX.js").status_code == 404


def test_the_api_is_never_swallowed_by_the_fallback(web_client):
    body = web_client.get("/api/v1/health").json()
    assert body["status"] == "ok"
