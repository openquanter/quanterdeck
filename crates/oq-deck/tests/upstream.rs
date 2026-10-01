//! The upstream release check, through the HTTP surface.
//!
//! The fetcher answers from fixtures and counts what it was asked: no
//! test here reaches GitHub, and "served from memory" is checked by the
//! count not moving.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use oq_deck::app::router_with_upstream;
use oq_deck::settings::Settings;
use oq_deck::upstream::{DECK_FRAMEWORK_REV, DECK_VERSION, Upstream};
use oq_deck_core::upstream::{Fetch, Reply, Version};
use serde_json::Value;
use tower::ServiceExt;

const PASSWORD: &str = "a long enough passphrase";
const HOST: &str = "127.0.0.1:8899";
const ORIGIN: &str = "http://127.0.0.1:8899";
const REPO: &str = "openquanter/openquanter";
const SELF_REPO: &str = "openquanter/quanterdeck";
const TAG_SHA: &str = "1111111111111111111111111111111111111111";

/// A tag for this console's own repository, `patch_by` patch releases
/// away from the version this test binary was built as — so the tests
/// say "behind" and "current" without naming a version that the next
/// release would make wrong.
fn self_tag(patch_by: i64) -> String {
    let v = Version::parse(DECK_VERSION).expect("the workspace version parses");
    let patch = v.patch.checked_add_signed(patch_by).expect("in range");
    format!("v{}.{}.{patch}", v.major, v.minor)
}

fn self_release(tag: &str) -> String {
    format!(
        r#"{{"tag_name":"{tag}","name":"{tag}","prerelease":false,
            "published_at":"2026-10-02T08:00:00Z",
            "html_url":"https://github.com/{SELF_REPO}/releases/tag/{tag}"}}"#
    )
}

fn self_latest() -> String {
    format!("/repos/{SELF_REPO}/releases/latest")
}

#[derive(Default)]
struct Canned {
    replies: HashMap<String, Reply>,
    asked: Mutex<Vec<String>>,
}

impl Canned {
    fn with(mut self, path: String, status: u16, body: &str) -> Self {
        self.replies.insert(
            path,
            Reply {
                status,
                body: body.to_owned(),
                ..Reply::default()
            },
        );
        self
    }

    fn asked(&self) -> usize {
        self.asked.lock().unwrap().len()
    }
}

impl Fetch for Canned {
    fn get(&self, path: &str) -> Result<Reply, String> {
        self.asked.lock().unwrap().push(path.to_owned());
        self.replies
            .get(path)
            .cloned()
            .ok_or_else(|| "connection refused".to_owned())
    }
}

/// A release `v2.0.1` that the deck's own revision is three behind, and
/// a console release the same version as this build.
fn behind_by_three() -> Canned {
    framework_behind_by_three().with(self_latest(), 200, &self_release(&self_tag(0)))
}

/// The framework half of [`behind_by_three`] alone: the console's own
/// request then has no reply, which is a transport failure.
fn framework_behind_by_three() -> Canned {
    Canned::default()
        .with(
            format!("/repos/{REPO}/releases/latest"),
            200,
            r#"{"tag_name":"v2.0.1","name":"2.0.1","prerelease":false,
                "published_at":"2026-10-01T08:00:00Z",
                "html_url":"https://github.com/openquanter/openquanter/releases/tag/v2.0.1"}"#,
        )
        .with(
            format!("/repos/{REPO}/commits/v2.0.1"),
            200,
            &format!(r#"{{"sha":"{TAG_SHA}"}}"#),
        )
        .with(
            format!("/repos/{REPO}/compare/{TAG_SHA}...{DECK_FRAMEWORK_REV}?per_page=1"),
            200,
            r#"{"status":"behind","ahead_by":0,"behind_by":3}"#,
        )
}

fn settings(hours: u64) -> Settings {
    Settings {
        password_hash: Some(oq_deck_core::auth::hash_password(PASSWORD).expect("hashes")),
        upstream_every_hours: hours,
        ..Settings::default()
    }
}

struct Client {
    app: axum::Router,
    cookie: String,
}

impl Client {
    async fn new(settings: Settings, fetch: Arc<Canned>) -> Self {
        let upstream = Upstream::new(&settings, fetch);
        let app = router_with_upstream(settings, None, None, None, upstream);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/session/login")
                    .header(header::HOST, HOST)
                    .header(header::ORIGIN, ORIGIN)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::json!({ "password": PASSWORD }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "login");
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|c| c.split(';').next())
            .expect("a session cookie")
            .to_owned();
        Self { app, cookie }
    }

    async fn send(&self, method: &str, uri: &str, origin: bool) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, HOST)
            .header(header::COOKIE, &self.cookie);
        if origin {
            req = req.header(header::ORIGIN, ORIGIN);
        }
        let response = self
            .app
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        self.send("GET", uri, false).await
    }

    async fn refresh(&self) -> (StatusCode, Value) {
        self.send("POST", "/api/v1/upstream/refresh", true).await
    }
}

#[tokio::test]
async fn the_report_needs_a_session() {
    let fetch = Arc::new(Canned::default());
    let s = settings(6);
    let upstream = Upstream::new(&s, fetch.clone());
    let app = router_with_upstream(s, None, None, None, upstream);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/upstream")
                .header(header::HOST, HOST)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(fetch.asked(), 0);
}

#[tokio::test]
async fn before_a_check_nothing_is_claimed() {
    let fetch = Arc::new(Canned::default());
    let c = Client::new(settings(6), fetch.clone()).await;
    let (status, body) = c.get("/api/v1/upstream").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["enabled"], true);
    assert_eq!(body["repo"], REPO);
    assert_eq!(body["checked_at_ms"], Value::Null);
    assert_eq!(body["published"], Value::Null, "unknown, not 'no release'");
    assert_eq!(body["behind"], false);
    assert_eq!(fetch.asked(), 0, "reading the report never reaches out");
}

#[tokio::test]
async fn a_refresh_checks_and_the_report_serves_the_result_from_memory() {
    let fetch = Arc::new(behind_by_three());
    let c = Client::new(settings(6), fetch.clone()).await;
    let (status, body) = c.refresh().await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // The release, its commit, and one comparison: the trader has no
    // agent here, so it is not compared. Then the console's own release.
    assert_eq!(fetch.asked(), 4);

    let (_, body) = c.get("/api/v1/upstream").await;
    assert_eq!(fetch.asked(), 4, "served from memory");
    let console = &body["console"];
    assert_eq!(console["repo"], SELF_REPO);
    assert_eq!(console["version"], DECK_VERSION);
    assert_eq!(console["verdict"], "current");
    assert_eq!(console["behind"], false);
    assert_eq!(console["latest"]["tag"], self_tag(0));
    assert_eq!(body["error"], Value::Null);
    assert_eq!(body["published"], true);
    assert_eq!(body["latest"]["tag"], "v2.0.1");
    assert_eq!(body["latest"]["sha"], TAG_SHA);
    assert_eq!(body["behind"], true);
    let revs = body["revisions"].as_array().expect("revisions");
    assert_eq!(revs[0]["what"], "deck");
    assert_eq!(revs[0]["rev"], DECK_FRAMEWORK_REV);
    assert_eq!(revs[0]["status"], "behind");
    assert_eq!(revs[0]["behind_by"], 3);
    assert_eq!(revs[0]["verdict"], "behind");
    // No agent: the trader is "cannot tell", with the reason, never a
    // verdict it did not earn.
    assert_eq!(revs[1]["what"], "trader");
    assert_eq!(revs[1]["verdict"], "unknown");
    assert!(
        revs[1]["reason_en"]
            .as_str()
            .unwrap()
            .contains("OQ_DECK_AGENT_SOCKET")
    );
}

#[tokio::test]
async fn a_second_refresh_within_a_minute_is_refused_without_a_request() {
    let fetch = Arc::new(behind_by_three());
    let c = Client::new(settings(6), fetch.clone()).await;
    assert_eq!(c.refresh().await.0, StatusCode::OK);
    let before = fetch.asked();
    let (status, body) = c.refresh().await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(body["detail"].as_str().is_some());
    assert_eq!(fetch.asked(), before);
}

#[tokio::test]
async fn a_refresh_is_a_write_for_origin_but_not_for_write_mode() {
    let fetch = Arc::new(behind_by_three());
    let s = settings(6);
    assert!(!s.allow_writes, "a read-only deck");
    let c = Client::new(s, fetch.clone()).await;
    // A page elsewhere cannot make the deck send requests.
    let (status, _) = c.send("POST", "/api/v1/upstream/refresh", false).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(fetch.asked(), 0);
    // Its own page can, with writes off: nothing on the host changes.
    assert_eq!(c.refresh().await.0, StatusCode::OK);
}

#[tokio::test]
async fn a_failed_check_is_an_error_not_up_to_date() {
    // Nothing canned: every request is a transport failure.
    let fetch = Arc::new(Canned::default());
    let c = Client::new(settings(6), fetch.clone()).await;
    let (status, body) = c.refresh().await;
    assert_eq!(status, StatusCode::OK, "the refresh ran; the check failed");
    assert!(body["checked_at_ms"].as_i64().is_some());
    assert!(body["error_en"].as_str().unwrap().contains("Cannot reach"));
    assert_eq!(body["succeeded_at_ms"], Value::Null);
    assert_eq!(body["published"], Value::Null);
    assert_eq!(body["behind"], false);
    assert_eq!(body["revisions"], serde_json::json!([]));
    let console = &body["console"];
    assert!(
        console["error_en"]
            .as_str()
            .unwrap()
            .contains("Cannot reach")
    );
    assert_eq!(console["published"], Value::Null);
    assert_eq!(console["verdict"], "unknown", "not 'current'");
    assert_eq!(console["behind"], false);
}

#[tokio::test]
async fn a_newer_console_release_is_reported_behind() {
    let fetch =
        Arc::new(framework_behind_by_three().with(self_latest(), 200, &self_release(&self_tag(1))));
    let c = Client::new(settings(6), fetch.clone()).await;
    let (status, body) = c.refresh().await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let console = &body["console"];
    assert_eq!(console["error"], Value::Null);
    assert_eq!(console["published"], true);
    assert_eq!(console["latest"]["tag"], self_tag(1));
    assert!(
        console["latest"]["url"]
            .as_str()
            .unwrap()
            .starts_with("https://github.com/")
    );
    assert_eq!(console["latest"].get("sha"), None, "compared by version");
    assert_eq!(console["verdict"], "behind");
    assert_eq!(console["behind"], true);
    // The framework's own field still means the framework.
    assert_eq!(body["behind"], true);
}

#[tokio::test]
async fn no_console_release_yet_is_its_own_state() {
    let fetch = Arc::new(framework_behind_by_three().with(
        self_latest(),
        404,
        r#"{"message":"Not Found"}"#,
    ));
    let c = Client::new(settings(6), fetch.clone()).await;
    let (_, body) = c.refresh().await;
    let console = &body["console"];
    assert_eq!(console["error"], Value::Null, "an answer, not a failure");
    assert_eq!(console["published"], false);
    assert_eq!(console["latest"], Value::Null);
    assert_eq!(console["verdict"], "unknown");
    assert!(
        console["reason_en"]
            .as_str()
            .unwrap()
            .contains("no release yet")
    );
}

#[tokio::test]
async fn a_console_failure_does_not_touch_the_framework_answer() {
    let mut fetch = framework_behind_by_three();
    fetch.replies.insert(
        self_latest(),
        Reply {
            status: 429,
            body: r#"{"message":"secondary rate limit"}"#.into(),
            ..Reply::default()
        },
    );
    let c = Client::new(settings(6), Arc::new(fetch)).await;
    let (_, body) = c.refresh().await;
    assert_eq!(body["error"], Value::Null);
    assert_eq!(body["published"], true);
    assert_eq!(body["revisions"][0]["verdict"], "behind");
    let console = &body["console"];
    assert!(console["error_en"].as_str().unwrap().contains("rate limit"));
    assert_eq!(console["verdict"], "unknown");
}

#[tokio::test]
async fn a_framework_failure_does_not_touch_the_console_answer() {
    let fetch = Arc::new(
        Canned::default()
            .with(format!("/repos/{REPO}/releases/latest"), 500, "{}")
            .with(self_latest(), 200, &self_release(&self_tag(1))),
    );
    let c = Client::new(settings(6), fetch.clone()).await;
    let (_, body) = c.refresh().await;
    assert!(body["error_en"].as_str().unwrap().contains("HTTP 500"));
    assert_eq!(body["published"], Value::Null);
    let console = &body["console"];
    assert_eq!(console["error"], Value::Null);
    assert_eq!(console["verdict"], "behind");
    assert_eq!(console["behind"], true);
}

#[tokio::test]
async fn zero_hours_turns_it_off_and_nothing_is_fetched() {
    let fetch = Arc::new(behind_by_three());
    let c = Client::new(settings(0), fetch.clone()).await;
    let (status, body) = c.get("/api/v1/upstream").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["enabled"], false);
    assert!(
        body["reason_en"]
            .as_str()
            .unwrap()
            .contains("OQ_DECK_UPSTREAM_CHECK_HOURS=0")
    );
    let (status, _) = c.refresh().await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (_, caps) = c.get("/api/v1/runtime/capabilities").await;
    assert_eq!(caps["upstream"]["available"], false);
    assert!(caps["upstream"]["reason"].as_str().unwrap().contains("=0"));
    assert_eq!(fetch.asked(), 0, "off means no request at all");
}

#[test]
fn the_upstream_settings_are_bounded() {
    let base = Settings::default();
    assert!(base.validate().is_ok());
    assert_eq!(base.upstream_every_hours, 6);
    assert_eq!(base.upstream_repo, REPO);
    let bad = |s: Settings| s.validate().unwrap_err().0;
    assert!(
        bad(Settings {
            upstream_every_hours: 169,
            ..Settings::default()
        })
        .contains("OQ_DECK_UPSTREAM_CHECK_HOURS")
    );
    assert!(
        bad(Settings {
            upstream_repo: "not a repo".into(),
            ..Settings::default()
        })
        .contains("OQ_DECK_UPSTREAM_REPO")
    );
    assert_eq!(base.self_repo, SELF_REPO);
    assert!(
        bad(Settings {
            self_repo: "a/../b".into(),
            ..Settings::default()
        })
        .contains("OQ_DECK_SELF_REPO")
    );
    let proxy = bad(Settings {
        upstream_proxy: Some("socks5://user:secret@host:1080".into()),
        ..Settings::default()
    });
    assert!(proxy.contains("OQ_DECK_UPSTREAM_PROXY"));
    assert!(!proxy.contains("secret"), "a proxy URL is not echoed");
    assert!(
        Settings {
            upstream_proxy: Some("http://127.0.0.1:3128".into()),
            upstream_every_hours: 0,
            ..Settings::default()
        }
        .validate()
        .is_ok()
    );
}

/// The trader's revision is read from the current release's manifest
/// through the agent, with the same read the releases page uses.
#[tokio::test]
async fn the_trader_is_compared_at_the_revision_its_release_names() {
    use oq_deck_core::ops::{AgentRequest, AgentResponse, Op};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    const TRADER: &str = "cccccccccccccccccccccccccccccccccccccccc";
    let dir = tempfile::tempdir().expect("dir");
    let sock = dir.path().join("agent.sock");
    let listener = tokio::net::UnixListener::bind(&sock).expect("bind");
    let seen: Arc<Mutex<Vec<AgentRequest>>> = Arc::default();
    let kept = Arc::clone(&seen);
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let (read, mut write) = stream.into_split();
            let mut line = String::new();
            BufReader::new(read)
                .read_line(&mut line)
                .await
                .expect("read");
            kept.lock()
                .unwrap()
                .push(serde_json::from_str(&line).expect("a request"));
            let reply = AgentResponse::ok(serde_json::json!({
                "current": "r42",
                "staged": [{"id": "r42", "verified": true,
                            "manifest": {"id": "r42", "framework": TRADER}}],
            }));
            let mut out = serde_json::to_string(&reply).expect("encode");
            out.push('\n');
            write.write_all(out.as_bytes()).await.expect("write");
        }
    });

    let fetch = Arc::new(behind_by_three().with(
        format!("/repos/{REPO}/compare/{TAG_SHA}...{TRADER}?per_page=1"),
        200,
        r#"{"status":"identical","ahead_by":0,"behind_by":0}"#,
    ));
    let c = Client::new(
        Settings {
            agent_socket: Some(sock),
            ..settings(6)
        },
        fetch.clone(),
    )
    .await;
    let (status, body) = c.refresh().await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let trader = &body["revisions"][1];
    assert_eq!(trader["what"], "trader");
    assert_eq!(trader["release"], "r42");
    assert_eq!(trader["rev"], TRADER);
    assert_eq!(trader["verdict"], "includes");
    let asked = seen.lock().unwrap();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].op, Op::Releases, "a read, and only that");
}

fn touch(path: &std::path::Path, secs_ago: u64) {
    std::fs::write(path, b"").expect("write");
    let f = std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open");
    f.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(secs_ago))
        .expect("mtime");
}

/// The running journal is the one being written: newest by modification
/// time, not by name, and only `.oqj` files count.
#[test]
fn the_running_journal_is_the_most_recently_written() {
    let dir = tempfile::tempdir().expect("dir");
    touch(&dir.path().join("zzz-old.oqj"), 600);
    touch(&dir.path().join("aaa-new.oqj"), 5);
    touch(&dir.path().join("aaa-new.lock"), 0);
    assert_eq!(
        oq_deck::upstream::newest_journal(dir.path()).as_deref(),
        Some("aaa-new.oqj")
    );
    let empty = tempfile::tempdir().expect("dir");
    assert_eq!(oq_deck::upstream::newest_journal(empty.path()), None);
}

/// A deploy restarts the trader, which opens a new journal; that is what
/// brings the next check forward instead of waiting out the schedule.
#[tokio::test]
async fn a_trader_restart_is_noticed_after_a_check() {
    let dir = tempfile::tempdir().expect("dir");
    touch(&dir.path().join("oqp-live-20261001-163813.oqj"), 600);
    let s = Settings {
        journals_dir: Some(dir.path().to_path_buf()),
        ..settings(6)
    };
    let fetch = Arc::new(behind_by_three());
    let up = Upstream::new(&s, fetch.clone());

    assert!(
        !up.trader_restarted().await,
        "nothing to compare before a check"
    );
    up.check().await;
    let asked = fetch.asked();
    assert!(!up.trader_restarted().await, "the same run");

    touch(&dir.path().join("oqp-live-20261001-175804.oqj"), 0);
    assert!(up.trader_restarted().await, "a new journal is a restart");
    assert_eq!(fetch.asked(), asked, "noticing it asks GitHub nothing");

    up.check().await;
    assert!(
        !up.trader_restarted().await,
        "seen by the check that followed"
    );
}
