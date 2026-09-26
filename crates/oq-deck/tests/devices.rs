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

/// Enrolling a machine by proving a key, which is what an operator does
/// on a laptop they have never typed the password on.
///
/// The whole exchange, against a real `ssh-keygen`: ask for a challenge,
/// sign it, and trade the claim code for the cookie.
#[tokio::test]
async fn a_proven_key_enrols_a_browser() {
    use std::process::Command;

    let dir = tempfile::tempdir().expect("dir");
    let key = dir.path().join("id");
    let made = Command::new("ssh-keygen")
        .args(["-t", "ed25519", "-N", "", "-C", "enrol-test", "-f"])
        .arg(&key)
        .output()
        .expect("ssh-keygen");
    assert!(made.status.success());
    let public = std::fs::read_to_string(key.with_extension("pub")).expect("public");
    let signers = dir.path().join("allowed_signers");
    std::fs::write(&signers, format!("laptop {public}")).expect("write");

    let settings = Settings {
        state_dir: Some(dir.path().to_path_buf()),
        password_hash: Some(oq_deck_core::auth::hash_password(PASSWORD).expect("hashes")),
        trusted_keys: Some(signers.clone()),
        ..Settings::default()
    };
    let devices = oq_deck_core::devices::Devices::open(dir.path()).expect("opens");
    let app = oq_deck::app::router(settings, None, None, Some(Arc::new(Mutex::new(devices))));

    let post = |uri: &str, body: serde_json::Value| {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header(header::HOST, HOST)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };

    // A command line sends no Origin; the deck treats that as what it is.
    let (status, body, _) = send(
        &app,
        post("/api/v1/session/challenge", serde_json::json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let challenge = body["challenge"].as_str().expect("a challenge").to_owned();
    assert_eq!(body["namespace"], "oq-deck-enrol");

    let signature = sign_as(&key, &challenge);

    let (status, body, _) = send(
        &app,
        post(
            "/api/v1/session/enrol",
            serde_json::json!({ "challenge": challenge, "signature": signature,
                                "identity": "laptop", "label": "the work laptop" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let claim = body["claim"].as_str().expect("a claim code").to_owned();

    // The browser's half. It sends an Origin, because it is one.
    let mut req = post(
        "/api/v1/session/claim",
        serde_json::json!({ "claim": claim }),
    );
    req.headers_mut()
        .insert(header::ORIGIN, ORIGIN.parse().expect("origin"));
    let (status, body, cookies) = send(&app, req).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // The name the operator gave it survived the trip through the
    // browser, which is the only thing that makes the revoke list a list
    // of names rather than a list of hex.
    assert_eq!(body["label"], "the work laptop");
    let device = named(&cookies, "oq_deck_device").expect("a device cookie");

    let (_, body, _) = send(&app, get("/api/v1/session", Some(&device))).await;
    assert_eq!(body["authenticated"], true, "{body}");

    // And the code is spent: a second browser presenting it is refused.
    let mut again = post(
        "/api/v1/session/claim",
        serde_json::json!({ "claim": claim }),
    );
    again
        .headers_mut()
        .insert(header::ORIGIN, ORIGIN.parse().expect("origin"));
    let (status, _, _) = send(&app, again).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// Sign a challenge the way `scripts/deck-enrol.sh` does.
fn sign_as(key: &std::path::Path, challenge: &str) -> String {
    use std::io::Write as _;
    let mut child = std::process::Command::new("ssh-keygen")
        .args(["-Y", "sign", "-n", "oq-deck-enrol", "-f"])
        .arg(key)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(challenge.as_bytes())
        .expect("write");
    let out = child.wait_with_output().expect("wait");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf8")
}
