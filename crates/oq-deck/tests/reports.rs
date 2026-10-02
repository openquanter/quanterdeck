//! Scheduled reports, through the HTTP surface: who may read them, how a
//! page is served, what an id from a URL can reach, and the rate limit.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use http_body_util::BodyExt;
use oq_deck::app::{REPORT_CSP, router};
use oq_deck::settings::Settings;
use oq_deck_core::ops::{AgentRequest, AgentResponse, Op};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tower::ServiceExt;

const PASSWORD: &str = "a long enough passphrase";
const HOST: &str = "127.0.0.1:8899";
const ORIGIN: &str = "http://127.0.0.1:8899";
const HOSTILE: &str = "<script>alert(1)</script>";

/// An agent that answers every request with `reply` and keeps what it
/// was sent.
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

/// A black box window with a trader sample and an alert carrying a
/// hostile message, timed relative to now so it falls in the period.
fn window() -> Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_millis();
    let now = i64::try_from(now).expect("fits");
    json!({
        "trader": [
            {"at": now - 3_600_000, "trader": {"halted": false, "resting": 2, "positions": [],
             "pnl": {"since_ms": now - 7_200_000, "realized": "1", "fees": "0.1", "funding": "0",
                     "net": "0.9", "equity": "100"}}},
            {"at": now - 60_000, "trader": {"halted": false, "resting": 3, "positions": [],
             "pnl": {"since_ms": now - 7_200_000, "realized": "2", "fees": "0.2", "funding": "0",
                     "net": "1.8", "equity": "101"}}},
        ],
        "host": [],
        "events": [
            {"at": now - 120_000, "k": "event", "what": "alert_raised", "key": "k",
             "message": HOSTILE, "message_en": HOSTILE},
        ],
    })
}

struct Deck {
    app: axum::Router,
    cookie: String,
    reports: tempfile::TempDir,
}

fn settings(reports: &std::path::Path, agent: Option<std::path::PathBuf>) -> Settings {
    Settings {
        password_hash: Some(oq_deck_core::auth::hash_password(PASSWORD).expect("hash")),
        agent_socket: agent,
        reports_dir: Some(reports.to_path_buf()),
        ..Settings::default()
    }
}

impl Deck {
    async fn new(agent: Option<std::path::PathBuf>) -> Self {
        let reports = tempfile::tempdir().expect("dir");
        Self::with(settings(reports.path(), agent), reports).await
    }

    async fn with(settings: Settings, reports: tempfile::TempDir) -> Self {
        let app = router(settings, None, None, None);
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
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|c| c.split(';').next())
            .expect("cookie")
            .to_owned();
        Self {
            app,
            cookie,
            reports,
        }
    }

    async fn send(&self, req: Request<Body>) -> (StatusCode, HeaderMap, String) {
        let response = self.app.clone().oneshot(req).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            headers,
            String::from_utf8_lossy(&bytes).into_owned(),
        )
    }

    fn get(&self, uri: &str) -> Request<Body> {
        Request::builder()
            .uri(uri)
            .header(header::HOST, HOST)
            .header(header::COOKIE, &self.cookie)
            .body(Body::empty())
            .unwrap()
    }

    fn generate(&self) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/api/v1/reports/generate")
            .header(header::HOST, HOST)
            .header(header::ORIGIN, ORIGIN)
            .header(header::COOKIE, &self.cookie)
            .body(Body::empty())
            .unwrap()
    }

    async fn json(&self, uri: &str) -> (StatusCode, Value) {
        let (status, _, body) = self.send(self.get(uri)).await;
        (status, serde_json::from_str(&body).unwrap_or(Value::Null))
    }
}

#[tokio::test]
async fn a_report_is_written_listed_and_served_as_a_locked_down_page() {
    let agent_dir = tempfile::tempdir().expect("dir");
    let (sock, seen) = fake_agent(agent_dir.path(), AgentResponse::ok(window()));
    let deck = Deck::new(Some(sock)).await;

    let (status, _, body) = deck.send(deck.generate()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let entry: Value = serde_json::from_str(&body).expect("json");
    let id = entry["id"].as_str().expect("id").to_owned();
    assert_eq!(entry["trigger"], "manual");
    // Without a venue record the verdict is absent, not "agree".
    assert_eq!(entry["verdict"], Value::Null);
    // The run began inside the period, so its whole P&L counts.
    assert!((entry["net"].as_f64().expect("net") - 1.8).abs() < 1e-9);
    {
        let seen = seen.lock().expect("lock");
        assert_eq!(seen.len(), 1);
        assert!(matches!(seen[0].op, Op::Blackbox { .. }));
        assert_eq!(seen[0].actor, "deck:report");
    }
    assert!(
        deck.reports.path().join(format!("{id}.json")).is_file(),
        "kept as a file"
    );

    let (status, list) = deck.json("/api/v1/reports").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["every_hours"], 24);
    assert_eq!(list["reports"][0]["id"], id.as_str());

    let (status, headers, html) = deck
        .send(deck.get(&format!("/api/v1/reports/{id}?lang=en")))
        .await;
    assert_eq!(status, StatusCode::OK, "{html}");
    assert_eq!(
        headers[header::CONTENT_TYPE].to_str().unwrap(),
        "text/html; charset=utf-8"
    );
    assert_eq!(
        headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap(),
        REPORT_CSP,
        "the report's own policy, not the console's"
    );
    assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("Trading report") && html.contains("Net P&amp;L"));
    assert!(!html.contains(HOSTILE), "escaped");
    assert!(html.contains("OQ_DECK_JOURNALS_DIR"), "the gap says why");

    // The language follows the reader: the header, and the parameter over it.
    let mut zh = deck.get(&format!("/api/v1/reports/{id}"));
    zh.headers_mut()
        .insert(header::ACCEPT_LANGUAGE, "zh-CN".parse().unwrap());
    let (_, _, html) = deck.send(zh).await;
    assert!(html.contains("交易报告"));
    let mut en = deck.get(&format!("/api/v1/reports/{id}"));
    en.headers_mut()
        .insert(header::ACCEPT_LANGUAGE, "en".parse().unwrap());
    assert!(deck.send(en).await.2.contains("Trading report"));
    let (status, _, _) = deck
        .send(deck.get(&format!("/api/v1/reports/{id}?lang=fr")))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, data) = deck.json(&format!("/api/v1/reports/{id}/data")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data["format"], 1);
    assert_eq!(data["reconciliation"], Value::Null);
    assert_eq!(data["unavailable"][0]["section"], "host");
}

#[tokio::test]
async fn a_second_report_within_a_minute_is_refused_with_retry_after() {
    let deck = Deck::new(None).await;
    let (status, _, _) = deck.send(deck.generate()).await;
    assert_eq!(status, StatusCode::OK);
    let (status, headers, body) = deck.send(deck.generate()).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    let secs: u64 = headers[header::RETRY_AFTER]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=60).contains(&secs), "{secs}");
}

#[tokio::test]
async fn without_an_agent_the_report_says_what_it_could_not_read() {
    let deck = Deck::new(None).await;
    let (status, _, body) = deck.send(deck.generate()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id = serde_json::from_str::<Value>(&body).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, data) = deck.json(&format!("/api/v1/reports/{id}/data")).await;
    assert_eq!(data["trader"], Value::Null);
    assert_eq!(data["events"], Value::Null, "unread, not empty");
    let sections: Vec<&str> = data["unavailable"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["section"].as_str().unwrap())
        .collect();
    assert_eq!(sections, ["trader", "events", "host", "reconciliation"]);
    assert!(
        data["unavailable"][0]["reason"]["en"]
            .as_str()
            .unwrap()
            .contains("OQ_DECK_AGENT_SOCKET")
    );
}

#[tokio::test]
async fn ids_from_the_url_reach_nothing_outside_the_listing() {
    let deck = Deck::new(None).await;
    let dir = deck.reports.path();
    // A file just outside the directory, and a link inside it pointing
    // there, named like a report.
    let outside = dir.parent().unwrap().join("1-2.json");
    std::fs::write(&outside, "{}").unwrap();
    std::os::unix::fs::symlink(&outside, dir.join("3-4.json")).unwrap();
    for uri in [
        "/api/v1/reports/..%2f1-2",
        "/api/v1/reports/..%2F..%2Fetc%2Fpasswd",
        "/api/v1/reports/%2Fetc%2Fpasswd",
        "/api/v1/reports//etc/passwd",
        "/api/v1/reports/1-2",
        "/api/v1/reports/3-4",
        "/api/v1/reports/3-4/data",
        "/api/v1/reports/..%2f1-2/data",
        "/api/v1/reports/..",
    ] {
        let (status, _, body) = deck.send(deck.get(uri)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {body}");
    }
    let (_, list) = deck.json("/api/v1/reports").await;
    assert_eq!(list["reports"], json!([]), "a link is not a report");
    std::fs::remove_file(outside).ok();
}

#[tokio::test]
async fn reading_needs_a_session_and_a_bad_host_is_refused_before_it() {
    let deck = Deck::new(None).await;
    for uri in [
        "/api/v1/reports",
        "/api/v1/reports/1-2",
        "/api/v1/reports/1-2/data",
    ] {
        let req = Request::builder()
            .uri(uri)
            .header(header::HOST, HOST)
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            deck.send(req).await.0,
            StatusCode::UNAUTHORIZED,
            "{uri} needs a session"
        );
        // A Host this deck does not answer to is refused first, cookie or
        // not: the session lookup is never reached.
        let req = Request::builder()
            .uri(uri)
            .header(header::HOST, "evil.example:8899")
            .header(header::COOKIE, &deck.cookie)
            .body(Body::empty())
            .unwrap();
        let status = deck.send(req).await.0;
        assert_eq!(status, StatusCode::MISDIRECTED_REQUEST, "{uri}");
    }
}

#[tokio::test]
async fn generating_is_a_write_for_origin_and_session_but_not_for_write_mode() {
    let deck = Deck::new(None).await;
    let without = |origin: Option<&str>, cookie: Option<&str>, host: &str| {
        let mut b = Request::builder()
            .method("POST")
            .uri("/api/v1/reports/generate")
            .header(header::HOST, host);
        if let Some(o) = origin {
            b = b.header(header::ORIGIN, o);
        }
        if let Some(c) = cookie {
            b = b.header(header::COOKIE, c);
        }
        b.body(Body::empty()).unwrap()
    };
    let cookie = deck.cookie.clone();
    assert_eq!(
        deck.send(without(None, Some(&cookie), HOST)).await.0,
        StatusCode::FORBIDDEN,
        "no Origin"
    );
    assert_eq!(
        deck.send(without(Some("http://evil.example"), Some(&cookie), HOST))
            .await
            .0,
        StatusCode::FORBIDDEN,
        "another Origin"
    );
    assert_eq!(
        deck.send(without(Some(ORIGIN), None, HOST)).await.0,
        StatusCode::UNAUTHORIZED,
        "no session"
    );
    assert_eq!(
        deck.send(without(Some(ORIGIN), Some(&cookie), "evil.example"))
            .await
            .0,
        StatusCode::MISDIRECTED_REQUEST,
        "Host first"
    );
    // The deck is read-only (the default) and it still writes its report.
    assert_eq!(deck.send(deck.generate()).await.0, StatusCode::OK);
}

#[tokio::test]
async fn turned_off_or_with_nowhere_to_keep_them_reports_say_why() {
    let dir = tempfile::tempdir().expect("dir");
    let mut off = settings(dir.path(), None);
    off.report_every_hours = 0;
    let deck = Deck::with(off, dir).await;
    let (_, caps) = deck.json("/api/v1/runtime/capabilities").await;
    assert_eq!(caps["reports"]["available"], false);
    let (status, body) = deck.json("/api/v1/reports").await;
    assert_eq!(status, StatusCode::PRECONDITION_REQUIRED);
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("OQ_DECK_REPORT_HOURS=0")
    );
    assert_eq!(deck.send(deck.generate()).await.0, StatusCode::CONFLICT);

    let dir = tempfile::tempdir().expect("dir");
    let mut nowhere = settings(dir.path(), None);
    nowhere.reports_dir = None;
    let deck = Deck::with(nowhere, dir).await;
    let mut req = deck.get("/api/v1/runtime/capabilities");
    req.headers_mut()
        .insert(header::ACCEPT_LANGUAGE, "en".parse().unwrap());
    let caps: Value = serde_json::from_str(&deck.send(req).await.2).unwrap();
    assert_eq!(caps["reports"]["available"], false);
    assert!(
        caps["reports"]["reason"]
            .as_str()
            .unwrap()
            .contains("OQ_DECK_REPORTS_DIR")
    );

    let deck = Deck::new(None).await;
    let (_, caps) = deck.json("/api/v1/runtime/capabilities").await;
    assert_eq!(caps["reports"]["available"], true);
}

#[test]
fn the_report_settings_are_bounded() {
    let s = Settings {
        report_every_hours: 169,
        ..Settings::default()
    };
    assert!(s.validate().is_err());
    let s = Settings {
        report_every_hours: 168,
        ..Settings::default()
    };
    assert!(s.validate().is_ok());
}
