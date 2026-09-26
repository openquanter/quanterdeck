//! A browser the operator enrolled, and taking it away again.
//!
//! What this is for: a session dies when it idles and it dies with the
//! process, so an operator who deploys twice in an afternoon signs in
//! twice in an afternoon. A device is the thing they made on purpose.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use oq_deck::settings::Settings;
use serde_json::Value;
use tower::ServiceExt;

const HOST: &str = "127.0.0.1:8899";
const ORIGIN: &str = "http://127.0.0.1:8899";
const PASSWORD: &str = "a long enough passphrase";

fn router_with_devices(dir: &std::path::Path) -> axum::Router {
    let settings = Settings {
        state_dir: Some(dir.to_path_buf()),
        password_hash: Some(oq_deck_core::auth::hash_password(PASSWORD).expect("hashes")),
        ..Settings::default()
    };
    let devices = oq_deck_core::devices::Devices::open(dir).expect("opens");
    oq_deck::app::router(settings, None, None, Some(Arc::new(Mutex::new(devices))))
}

async fn send(app: &axum::Router, request: Request<Body>) -> (StatusCode, Value, Vec<String>) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let cookies: Vec<String> = response
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

fn get(uri: &str, cookie: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().uri(uri).header(header::HOST, HOST);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    builder.body(Body::empty()).unwrap()
}

fn login(remember: bool) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/v1/session/login")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, ORIGIN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({
                "password": PASSWORD,
                "remember": remember,
                "device_label": "the kitchen laptop",
            })
            .to_string(),
        ))
        .unwrap()
}

fn named(cookies: &[String], name: &str) -> Option<String> {
    cookies
        .iter()
        .find(|c| c.starts_with(&format!("{name}=")))
        .map(|c| c.split(';').next().unwrap_or("").to_owned())
}

/// The whole point: after a restart every session is gone and the
/// enrolled browser is not asked again.
#[tokio::test]
async fn an_enrolled_browser_is_let_in_without_a_session() {
    let dir = tempfile::tempdir().expect("dir");
    let app = router_with_devices(dir.path());

    let (status, _, cookies) = send(&app, login(true)).await;
    assert_eq!(status, StatusCode::OK);
    let device = named(&cookies, "oq_deck_device").expect("a device cookie");

    // Dropped the session cookie on purpose: what is left is what a
    // browser holds after the deck has restarted.
    let (status, body, _) = send(&app, get("/api/v1/session", Some(&device))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["authenticated"], true, "{body}");

    // And it is a credential, not a skeleton key: another browser's
    // device token is not one.
    let (status, _, _) = send(
        &app,
        get("/api/v1/session", Some("oq_deck_device=nonsense")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

/// A device that cannot be taken away is one the operator cannot get
/// back, so revocation is the half that makes the other half offerable.
#[tokio::test]
async fn revoking_one_closes_it_and_leaves_the_session_alone() {
    let dir = tempfile::tempdir().expect("dir");
    let app = router_with_devices(dir.path());

    let (_, _, cookies) = send(&app, login(true)).await;
    let device = named(&cookies, "oq_deck_device").expect("a device cookie");
    let session = named(&cookies, "oq_deck_session").expect("a session cookie");

    let (status, _, _) = send(&app, get("/api/v1/session", Some(&device))).await;
    assert_eq!(status, StatusCode::OK);

    let (status, list, _) = send(&app, get("/api/v1/devices", Some(&session))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["available"], true, "{list}");
    let id = list["devices"][0]["id"].as_str().expect("an id").to_owned();
    assert_eq!(list["devices"][0]["label"], "the kitchen laptop");

    let revoke = Request::builder()
        .method("POST")
        .uri("/api/v1/devices/revoke")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, ORIGIN)
        .header(header::COOKIE, &session)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::json!({ "id": id }).to_string()))
        .unwrap();
    let (status, body, _) = send(&app, revoke).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["revoked"], true);

    // The device is out, and the session the operator is holding is not:
    // they are two different things.
    let (status, _, _) = send(&app, get("/api/v1/session", Some(&device))).await;
    assert_eq!(status, StatusCode::OK, "answered, but:");
    let (_, body, _) = send(&app, get("/api/v1/session", Some(&device))).await;
    assert_eq!(body["authenticated"], false, "{body}");
    let (_, body, _) = send(&app, get("/api/v1/session", Some(&session))).await;
    assert_eq!(body["authenticated"], true, "the session still works");
}

/// Nothing is offered that cannot be done: a deck with no state
/// directory says so, and the login form is told before it draws a box
/// that would do nothing.
#[tokio::test]
async fn a_deck_that_cannot_remember_a_browser_says_so() {
    let settings = Settings {
        password_hash: Some(oq_deck_core::auth::hash_password(PASSWORD).expect("hashes")),
        ..Settings::default()
    };
    let app = oq_deck::app::router(settings, None, None, None);

    let (_, _, cookies) = send(&app, login(true)).await;
    assert!(
        named(&cookies, "oq_deck_device").is_none(),
        "no device cookie when there is nowhere to keep one: {cookies:?}"
    );
    let (_, body, _) = send(&app, get("/api/v1/session", None)).await;
    assert_eq!(body["devices"], false, "{body}");

    let (status, body, _) = send(&app, get("/api/v1/devices", None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "still needs a session");
    assert!(body.is_null() || body["detail"].is_string(), "{body}");
}
