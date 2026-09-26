//! The gauntlet: what gets in, and what does not.

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use oq_deck::app::router;
use oq_deck::settings::Settings;
use serde_json::Value;
use tower::ServiceExt;

const PASSWORD: &str = "a long enough passphrase";
const HOST: &str = "127.0.0.1:8899";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/fixtures/runs")
}

fn configured() -> Settings {
    Settings {
        runs_dir: Some(fixtures()),
        password_hash: Some(oq_deck_core::auth::hash_password(PASSWORD).expect("hashes")),
        ..Settings::default()
    }
}

fn app(settings: Settings) -> axum::Router {
    router(settings, None, None)
}

async fn send(app: &axum::Router, request: Request<Body>) -> (StatusCode, Value, Vec<String>) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let cookies = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok().map(ToOwned::to_owned))
        .collect();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        cookies,
    )
}

fn get(uri: &str, host: &str, cookie: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().uri(uri).header(header::HOST, host);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    builder.body(Body::empty()).unwrap()
}

fn post(uri: &str, host: &str, origin: Option<&str>, body: Value) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::HOST, host)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(origin) = origin {
        builder = builder.header(header::ORIGIN, origin);
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

async fn log_in(app: &axum::Router) -> String {
    let (status, _, cookies) = send(
        app,
        post(
            "/api/v1/session/login",
            HOST,
            Some("http://127.0.0.1:8899"),
            serde_json::json!({ "password": PASSWORD }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the login should succeed");
    cookies
        .first()
        .and_then(|c| c.split(';').next())
        .expect("a session cookie")
        .to_owned()
}

// -- authentication is unconditional -------------------------------------

#[tokio::test]
async fn every_read_needs_a_session_even_on_loopback() {
    let app = app(configured());
    for uri in [
        "/api/v1/runs",
        "/api/v1/runs/baseline",
        "/api/v1/runtime/capabilities",
        "/api/v1/attribution?live=baseline&model=same-experiment",
        "/api/v1/journals",
    ] {
        let (status, _, _) = send(&app, get(uri, HOST, None)).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{uri} must not be readable without a session"
        );
    }
}

#[tokio::test]
async fn health_is_the_only_thing_open_and_it_says_nothing() {
    let (status, body, _) = send(&app(configured()), get("/api/v1/health", HOST, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    // No hint about whether setup is done, which would be a way to find
    // an unconfigured deck to race for.
    assert!(body.get("setup_required").is_none());
}

#[tokio::test]
async fn logging_in_opens_the_reads() {
    let app = app(configured());
    let cookie = log_in(&app).await;
    let (status, body, _) = send(&app, get("/api/v1/runs", HOST, Some(&cookie))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["entries"].as_array().unwrap().len() >= 3);
}

#[tokio::test]
async fn logging_out_closes_them_again() {
    let app = app(configured());
    let cookie = log_in(&app).await;
    let mut request = post(
        "/api/v1/session/logout",
        HOST,
        Some("http://127.0.0.1:8899"),
        serde_json::json!({}),
    );
    request
        .headers_mut()
        .insert(header::COOKIE, cookie.parse().unwrap());
    send(&app, request).await;

    let (status, _, _) = send(&app, get("/api/v1/runs", HOST, Some(&cookie))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// -- DNS rebinding --------------------------------------------------------

#[tokio::test]
async fn a_foreign_host_header_is_refused() {
    // The shape of DNS rebinding: a name the attacker controls, pointed
    // at 127.0.0.1. The socket cannot tell; the Host header can.
    let app = app(configured());
    let cookie = log_in(&app).await;
    let (status, body, _) =
        send(&app, get("/api/v1/runs", "evil.example.com", Some(&cookie))).await;
    assert_eq!(status, StatusCode::MISDIRECTED_REQUEST);
    assert!(body["detail"].as_str().unwrap().contains("rebinding"));
}

#[tokio::test]
async fn a_request_with_no_host_header_is_refused() {
    let app = app(configured());
    let request = Request::builder()
        .uri("/api/v1/runs")
        .body(Body::empty())
        .unwrap();
    let (status, _, _) = send(&app, request).await;
    assert_eq!(status, StatusCode::MISDIRECTED_REQUEST);
}

#[tokio::test]
async fn localhost_and_the_address_both_work() {
    let app = app(configured());
    let cookie = log_in(&app).await;
    for host in ["127.0.0.1:8899", "localhost:8899"] {
        let (status, _, _) = send(&app, get("/api/v1/runs", host, Some(&cookie))).await;
        assert_eq!(status, StatusCode::OK, "{host} should be allowed");
    }
}

#[tokio::test]
async fn a_proxy_name_can_be_allowed_explicitly() {
    let settings = Settings {
        extra_hosts: vec!["deck.internal".to_owned()],
        ..configured()
    };
    let app = app(settings);
    let cookie = log_in(&app).await;
    let (status, _, _) = send(&app, get("/api/v1/runs", "deck.internal", Some(&cookie))).await;
    assert_eq!(status, StatusCode::OK);
}

// -- cross-site writes ----------------------------------------------------

#[tokio::test]
async fn a_write_from_another_origin_is_refused() {
    let app = app(configured());
    let (status, _, _) = send(
        &app,
        post(
            "/api/v1/session/login",
            HOST,
            Some("http://evil.example.com"),
            serde_json::json!({ "password": PASSWORD }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_write_with_no_origin_is_refused() {
    let app = app(configured());
    let (status, _, _) = send(
        &app,
        post(
            "/api/v1/session/login",
            HOST,
            None,
            serde_json::json!({ "password": PASSWORD }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// -- the session cookie itself -------------------------------------------

#[tokio::test]
async fn the_session_cookie_is_locked_down() {
    let app = app(configured());
    let (_, _, cookies) = send(
        &app,
        post(
            "/api/v1/session/login",
            HOST,
            Some("http://127.0.0.1:8899"),
            serde_json::json!({ "password": PASSWORD }),
        ),
    )
    .await;
    let cookie = cookies.first().expect("a session cookie");
    assert!(cookie.contains("HttpOnly"), "script must not read it");
    assert!(cookie.contains("SameSite=Strict"), "no cross-site sends");
    assert!(
        !cookie.contains("Secure"),
        "plain http would drop a Secure cookie"
    );
}

#[tokio::test]
async fn behind_tls_the_cookie_becomes_secure() {
    let app = app(Settings {
        behind_tls: true,
        ..configured()
    });
    let (_, _, cookies) = send(
        &app,
        post(
            "/api/v1/session/login",
            HOST,
            // Behind TLS the browser's own origin is https.
            Some("https://127.0.0.1:8899"),
            serde_json::json!({ "password": PASSWORD }),
        ),
    )
    .await;
    assert!(cookies.first().expect("cookie").contains("Secure"));
}

/// The origin's authority must be the one the request was addressed
/// to.
///
/// The allow-list holds names without a port beside the names with one —
/// `localhost` next to `localhost:8899` — so that a Host header may name
/// the machine without naming a port. An Origin of `http://localhost` is
/// a page on port **80**: a different origin from this console's, one
/// anybody else on the machine can serve, and same-site to the browser,
/// so the session cookie rides along on the write.
#[tokio::test]
async fn an_origin_without_the_port_is_not_ours() {
    let plain = app(configured());
    for origin in ["http://localhost", "http://127.0.0.1"] {
        let (status, _, _) = send(
            &plain,
            post(
                "/api/v1/session/login",
                HOST,
                Some(origin),
                serde_json::json!({ "password": PASSWORD }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{origin}");
    }

    // And the authority this request was addressed to still is its own.
    let (status, _, _) = send(
        &plain,
        post(
            "/api/v1/session/login",
            HOST,
            Some("http://127.0.0.1:8899"),
            serde_json::json!({ "password": PASSWORD }),
        ),
    )
    .await;
    assert_ne!(status, StatusCode::FORBIDDEN, "the console's own origin");
}

/// An origin is its scheme too. Compared by authority alone, a bare
/// authority, another scheme, and plain http behind TLS all passed.
#[tokio::test]
async fn an_origin_with_the_wrong_scheme_is_not_ours() {
    let plain = app(configured());
    for origin in [
        "ftp://127.0.0.1:8899",
        "127.0.0.1:8899",
        "https://127.0.0.1:8899",
    ] {
        let (status, _, _) = send(
            &plain,
            post(
                "/api/v1/session/login",
                HOST,
                Some(origin),
                serde_json::json!({ "password": PASSWORD }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{origin}");
    }
    let tls = app(Settings {
        behind_tls: true,
        ..configured()
    });
    let (status, _, _) = send(
        &tls,
        post(
            "/api/v1/session/login",
            HOST,
            Some("http://127.0.0.1:8899"),
            serde_json::json!({ "password": PASSWORD }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "plain http behind TLS");
}

/// A browser that says a write came from another site is believed, even
/// when the Origin it sent would pass.
#[tokio::test]
async fn a_write_the_browser_marks_cross_site_is_refused() {
    let app = app(configured());
    for (site, expected) in [
        ("cross-site", StatusCode::FORBIDDEN),
        ("same-site", StatusCode::FORBIDDEN),
        ("same-origin", StatusCode::OK),
    ] {
        let mut request = post(
            "/api/v1/session/login",
            HOST,
            Some("http://127.0.0.1:8899"),
            serde_json::json!({ "password": PASSWORD }),
        );
        request
            .headers_mut()
            .insert("sec-fetch-site", site.parse().unwrap());
        let (status, _, _) = send(&app, request).await;
        assert_eq!(status, expected, "{site}");
    }
}

/// No route is reached with a Host this deck does not answer to — not
/// health, not logout, not a body parse.
#[tokio::test]
async fn a_foreign_host_reaches_no_route_at_all() {
    let app = app(configured());
    let (status, _, _) = send(&app, get("/api/v1/health", "evil.example", None)).await;
    assert_eq!(status, StatusCode::MISDIRECTED_REQUEST);
    let (status, _, _) = send(
        &app,
        post(
            "/api/v1/session/logout",
            "evil.example",
            None,
            serde_json::json!({}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::MISDIRECTED_REQUEST);
    let malformed = Request::builder()
        .method("POST")
        .uri("/api/v1/session/login")
        .header(header::HOST, "evil.example")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{not json"))
        .unwrap();
    let (status, _, _) = send(&app, malformed).await;
    assert_eq!(
        status,
        StatusCode::MISDIRECTED_REQUEST,
        "refused before the body is read"
    );
}

/// A mistyped API path is a 404, and every answer carries the headers
/// that keep it out of frames, sniffers, referrers and caches.
#[tokio::test]
async fn unknown_api_paths_are_404_and_responses_are_hardened() {
    let app = app(configured());
    let response = app
        .clone()
        .oneshot(get("/api/v1/nope", HOST, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let h = response.headers();
    assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(h[header::X_FRAME_OPTIONS], "DENY");
    assert_eq!(h[header::CACHE_CONTROL], "no-store");
    assert!(
        h[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .contains("frame-ancestors 'none'")
    );
}

// -- guessing -------------------------------------------------------------

#[tokio::test]
async fn a_wrong_password_says_nothing_useful() {
    let app = app(configured());
    let (status, body, _) = send(
        &app,
        post(
            "/api/v1/session/login",
            HOST,
            Some("http://127.0.0.1:8899"),
            serde_json::json!({ "password": "wrong but long enough" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let detail = body["detail"].as_str().unwrap();
    assert!(!detail.contains("hash") && !detail.contains("argon"));
}

#[tokio::test]
async fn repeated_failures_close_the_door() {
    let app = app(configured());
    for _ in 0..5 {
        send(
            &app,
            post(
                "/api/v1/session/login",
                HOST,
                Some("http://127.0.0.1:8899"),
                serde_json::json!({ "password": "wrong but long enough" }),
            ),
        )
        .await;
    }
    let (status, body, _) = send(
        &app,
        post(
            "/api/v1/session/login",
            HOST,
            Some("http://127.0.0.1:8899"),
            serde_json::json!({ "password": PASSWORD }),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "the correct password must not get in during a lockout"
    );
    assert!(body["detail"].as_str().unwrap().contains("分钟"));
}

/// Someone else's failures do not evict the operator.
///
/// The lockout used to refuse every session while it lasted, so anything
/// that could reach the port — another local process, anyone past a
/// reverse proxy — locked the operator out of a session they already had
/// with five bad requests, and could repeat it every fifteen minutes.
#[tokio::test]
async fn a_lockout_does_not_end_a_session_already_granted() {
    let app = app(configured());
    let cookie = log_in(&app).await;
    for _ in 0..5 {
        send(
            &app,
            post(
                "/api/v1/session/login",
                HOST,
                Some("http://127.0.0.1:8899"),
                serde_json::json!({ "password": "wrong but long enough" }),
            ),
        )
        .await;
    }
    let (status, _, _) = send(&app, get("/api/v1/runs", HOST, Some(&cookie))).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the operator's session is still theirs"
    );
}

/// A hash that does not parse is the deck's fault, reported as such and
/// not counted: read as a wrong password it locked the operator out.
#[tokio::test]
async fn a_corrupt_hash_is_not_a_wrong_password() {
    let app = app(Settings {
        password_hash: Some("not a hash".into()),
        ..configured()
    });
    for _ in 0..6 {
        let (status, body, _) = send(
            &app,
            post(
                "/api/v1/session/login",
                HOST,
                Some("http://127.0.0.1:8899"),
                serde_json::json!({ "password": PASSWORD }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
        assert!(
            body["detail"]
                .as_str()
                .unwrap()
                .contains("OQ_DECK_PASSWORD_HASH")
        );
    }
}

// -- setup ----------------------------------------------------------------

#[tokio::test]
async fn an_unconfigured_deck_serves_nothing_but_setup() {
    let settings = Settings {
        runs_dir: Some(fixtures()),
        ..Settings::default()
    };
    let app = router(settings, None, Some("the-one-time-token".to_owned()));
    let (status, _, _) = send(&app, get("/api/v1/runs", HOST, None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn setup_needs_the_token_from_the_terminal() {
    let settings = Settings {
        runs_dir: Some(fixtures()),
        ..Settings::default()
    };
    let app = router(settings, None, Some("the-one-time-token".to_owned()));

    let (status, _, _) = send(
        &app,
        post(
            "/api/v1/setup",
            HOST,
            Some("http://127.0.0.1:8899"),
            serde_json::json!({ "token": "guessed", "password": PASSWORD }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, body, _) = send(
        &app,
        post(
            "/api/v1/setup",
            HOST,
            Some("http://127.0.0.1:8899"),
            serde_json::json!({ "token": "the-one-time-token", "password": PASSWORD }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["password_hash"]
            .as_str()
            .unwrap()
            .starts_with("$argon2id$")
    );
    assert!(!body["totp_secret"].as_str().unwrap().is_empty());
}

/// The token is single use: a second setup with it is refused. It used
/// to stay valid until the process restarted, and each use issued a new
/// hash and secret.
#[tokio::test]
async fn the_setup_token_is_spent_by_a_setup_that_succeeds() {
    let app = router(Settings::default(), None, Some("once".to_owned()));
    let attempt = || {
        post(
            "/api/v1/setup",
            HOST,
            Some("http://127.0.0.1:8899"),
            serde_json::json!({ "token": "once", "password": PASSWORD }),
        )
    };
    assert_eq!(send(&app, attempt()).await.0, StatusCode::OK);
    assert_eq!(send(&app, attempt()).await.0, StatusCode::CONFLICT);
}

/// A password that is refused does not spend the token.
#[tokio::test]
async fn a_refused_password_leaves_the_token_usable() {
    let app = router(Settings::default(), None, Some("t".to_owned()));
    let with = |password: &str| {
        post(
            "/api/v1/setup",
            HOST,
            Some("http://127.0.0.1:8899"),
            serde_json::json!({ "token": "t", "password": password }),
        )
    };
    assert_eq!(send(&app, with("short")).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(send(&app, with(PASSWORD)).await.0, StatusCode::OK);
}

#[tokio::test]
async fn setup_refuses_a_weak_password_with_a_reason() {
    let app = router(Settings::default(), None, Some("t".to_owned()));
    let (status, body, _) = send(
        &app,
        post(
            "/api/v1/setup",
            HOST,
            Some("http://127.0.0.1:8899"),
            serde_json::json!({ "token": "t", "password": "short" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["detail"].as_str().unwrap().contains("12"));
}
