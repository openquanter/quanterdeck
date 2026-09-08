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
            Some("http://127.0.0.1:8899"),
            serde_json::json!({ "password": PASSWORD }),
        ),
    )
    .await;
    assert!(cookies.first().expect("cookie").contains("Secure"));
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
