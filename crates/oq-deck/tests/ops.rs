//! The operations routes, against a stand-in agent on a Unix socket.
//!
//! What matters here is the deck's half: it names who asked, carries the
//! reason and step-up code across untouched, refuses writes it is not
//! allowed to make, and shows the agent's refusal as a refusal.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use oq_deck::app::router;
use oq_deck::settings::Settings;
use oq_deck_core::ops::{AgentRequest, AgentResponse, Op};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tower::ServiceExt;

const PASSWORD: &str = "a long enough passphrase";
const HOST: &str = "127.0.0.1:8899";
const ORIGIN: &str = "http://127.0.0.1:8899";

/// An agent that answers every request with `reply` and keeps what it was
/// sent.
fn fake_agent(
    dir: &std::path::Path,
    reply: AgentResponse,
) -> (std::path::PathBuf, Arc<Mutex<Vec<AgentRequest>>>) {
    let path = dir.join("agent.sock");
    let listener = tokio::net::UnixListener::bind(&path).expect("bind");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let (read, mut write) = stream.into_split();
            let mut line = String::new();
            BufReader::new(read)
                .read_line(&mut line)
                .await
                .expect("read");
            kept.lock()
                .expect("lock")
                .push(serde_json::from_str(&line).expect("a request"));
            let mut out = serde_json::to_string(&reply).expect("encode");
            out.push('\n');
            write.write_all(out.as_bytes()).await.expect("write");
        }
    });
    (path, seen)
}

async fn login(app: &axum::Router) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/session/login")
                .header(header::HOST, HOST)
                .header(header::ORIGIN, ORIGIN)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "password": PASSWORD }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|c| c.split(';').next())
        .expect("cookie")
        .to_owned()
}

async fn call(
    app: &axum::Router,
    cookie: &str,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, HOST)
        .header(header::ORIGIN, ORIGIN)
        .header(header::COOKIE, cookie);
    if body.is_some() {
        req = req.header(header::CONTENT_TYPE, "application/json");
    }
    let response = app
        .clone()
        .oneshot(
            req.body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn settings(sock: std::path::PathBuf, writes: bool) -> Settings {
    Settings {
        password_hash: Some(oq_deck_core::auth::hash_password(PASSWORD).expect("hash")),
        agent_socket: Some(sock),
        allow_writes: writes,
        ..Settings::default()
    }
}

#[tokio::test]
async fn a_read_is_carried_across_with_a_fresh_nonce_and_the_askers_name() {
    let dir = tempfile::tempdir().expect("dir");
    let (sock, seen) = fake_agent(dir.path(), AgentResponse::ok(json!({"halted": false})));
    let app = router(settings(sock, false), None, None);
    let cookie = login(&app).await;

    let (status, body) = call(&app, &cookie, "GET", "/api/v1/ops/status", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["halted"], false);
    let (status, _) = call(&app, &cookie, "GET", "/api/v1/runtime/capabilities", None).await;
    assert_eq!(status, StatusCode::OK);

    call(&app, &cookie, "GET", "/api/v1/ops/status", None).await;
    let seen = seen.lock().expect("lock");
    assert_eq!(seen[0].op, Op::Status);
    assert!(
        seen[0].actor.starts_with("deck:operator@"),
        "{}",
        seen[0].actor
    );
    assert_ne!(seen[0].nonce, seen[1].nonce, "a nonce is never reused");
    assert!(seen[0].reason.is_none() && seen[0].step_up.is_none());
}

#[tokio::test]
async fn an_action_needs_writes_on_and_a_reason() {
    let dir = tempfile::tempdir().expect("dir");
    let (sock, seen) = fake_agent(dir.path(), AgentResponse::ok(json!({"state": "halted"})));

    let read_only = router(settings(sock.clone(), false), None, None);
    let cookie = login(&read_only).await;
    let (status, _) = call(
        &read_only,
        &cookie,
        "POST",
        "/api/v1/ops/action",
        Some(json!({"action": "halt", "reason": "looking"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let app = router(settings(sock, true), None, None);
    let cookie = login(&app).await;
    let (status, _) = call(
        &app,
        &cookie,
        "POST",
        "/api/v1/ops/action",
        Some(json!({"action": "halt", "reason": "  "})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, body) = call(
        &app,
        &cookie,
        "POST",
        "/api/v1/ops/action",
        Some(
            json!({"action": "unit", "unit": "trader.service", "verb": "restart",
                    "reason": "new config", "step_up": "123456"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let seen = seen.lock().expect("lock");
    assert_eq!(
        seen.len(),
        1,
        "the refused requests never reached the agent"
    );
    assert_eq!(
        seen[0].op,
        Op::Unit {
            unit: "trader.service".into(),
            verb: "restart".into()
        }
    );
    assert_eq!(seen[0].reason.as_deref(), Some("new config"));
    assert_eq!(seen[0].step_up.as_deref(), Some("123456"));
}

#[tokio::test]
async fn the_agents_refusal_is_shown_as_a_refusal() {
    let dir = tempfile::tempdir().expect("dir");
    let (sock, _) = fake_agent(
        dir.path(),
        AgentResponse::refused("the step-up code is wrong"),
    );
    let app = router(settings(sock, true), None, None);
    let cookie = login(&app).await;
    let (status, body) = call(
        &app,
        &cookie,
        "POST",
        "/api/v1/ops/action",
        Some(json!({"action": "resume", "reason": "fine", "step_up": "000000"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["detail"], "the step-up code is wrong");
}

#[tokio::test]
async fn no_agent_is_a_named_absence() {
    let app = router(
        Settings {
            password_hash: Some(oq_deck_core::auth::hash_password(PASSWORD).expect("hash")),
            ..Settings::default()
        },
        None,
        None,
    );
    let cookie = login(&app).await;
    let (status, body) = call(&app, &cookie, "GET", "/api/v1/ops/units", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or("")
            .contains("OQ_DECK_AGENT_SOCKET")
    );
}
