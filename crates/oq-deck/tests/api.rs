//! The HTTP surface, from behind a session.
//!
//! Authentication itself is exercised in `security.rs`; here it is set
//! up once and got out of the way, so these tests are about what the
//! endpoints return.

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
const ORIGIN: &str = "http://127.0.0.1:8899";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/fixtures/runs")
}

struct Client {
    app: axum::Router,
    cookie: String,
}

impl Client {
    async fn new(settings: Settings) -> Self {
        let app = router(settings, None, None);
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

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let response = self
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header(header::HOST, HOST)
                    .header(header::COOKIE, &self.cookie)
                    .body(Body::empty())
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
}

fn settings() -> Settings {
    Settings {
        runs_dir: Some(fixtures()),
        password_hash: Some(oq_deck_core::auth::hash_password(PASSWORD).expect("hashes")),
        ..Settings::default()
    }
}

async fn client() -> Client {
    Client::new(settings()).await
}

#[tokio::test]
async fn capabilities_explain_what_is_off() {
    let (_, body) = client().await.get("/api/v1/runtime/capabilities").await;
    assert_eq!(body["runs"]["available"], true);
    assert_eq!(body["live"]["available"], false);
    assert!(
        body["live"]["reason"]
            .as_str()
            .unwrap()
            .contains("OQ_DECK_JOURNALS_DIR"),
        "a capability that is off must name what would turn it on"
    );
    assert_eq!(
        body["writes"]["available"], false,
        "read-only is the default"
    );
}

#[tokio::test]
async fn a_deck_with_no_runs_directory_says_so_rather_than_showing_nothing() {
    let bare = Settings {
        runs_dir: None,
        ..settings()
    };
    let (status, body) = Client::new(bare).await.get("/api/v1/runs").await;
    assert_eq!(status, StatusCode::PRECONDITION_REQUIRED);
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("OQ_DECK_RUNS_DIR")
    );
}

#[tokio::test]
async fn the_listing_includes_the_file_that_would_not_parse() {
    let (status, body) = client().await.get("/api/v1/runs").await;
    assert_eq!(status, StatusCode::OK);
    let unreadable: Vec<_> = body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["state"] == "unreadable")
        .collect();
    assert_eq!(unreadable.len(), 1, "the truncated fixture must be listed");
    assert_eq!(unreadable[0]["id"], "truncated");
}

#[tokio::test]
async fn a_run_detail_carries_its_identity() {
    let (status, body) = client().await.get("/api/v1/runs/baseline").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["identity"]["code_commit"], "a1b2c3d");
    assert_eq!(body["fills"].as_array().unwrap().len(), 2);
    assert!(body["fills"][0]["tag"].is_null());
}

#[tokio::test]
async fn a_traversal_attempt_is_a_missing_run_not_a_file() {
    let (status, _) = client()
        .await
        .get("/api/v1/runs/..%2F..%2Fetc%2Fpasswd")
        .await;
    assert_ne!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_stale_baseline_never_reports_as_agreement() {
    let (status, body) = client()
        .await
        .get("/api/v1/runs/compare?baseline=baseline&candidate=config-moved")
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["verdict"]["status"], "invalidated");
    assert_eq!(body["passes"], false);
    assert_eq!(body["differences"], 0);
}

#[tokio::test]
async fn identical_output_under_new_code_passes() {
    let (_, body) = client()
        .await
        .get("/api/v1/runs/compare?baseline=baseline&candidate=same-experiment")
        .await;
    assert_eq!(body["verdict"]["status"], "code_changed");
    assert_eq!(body["passes"], true);
}

// -- attribution ----------------------------------------------------------

/// The precision the amounts are computed at is the caller's to give;
/// there is no default that would be right for a contract it never saw.
#[tokio::test]
async fn attribution_without_a_precision_is_refused() {
    let (status, body) = client()
        .await
        .get("/api/v1/attribution?live=config-moved&model=baseline")
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body["detail"].as_str().unwrap().contains("price_scale"));
}

#[tokio::test]
async fn attribution_declines_a_residual_it_cannot_compute() {
    let (status, body) = client()
        .await
        .get("/api/v1/attribution?price_scale=2&qty_scale=2&live=config-moved&model=baseline")
        .await;
    assert_eq!(status, StatusCode::OK);

    // A run file records no prevailing price, so slippage and latency
    // cannot be separated; funding and fees were not supplied. With any
    // component unavailable the residual must be null, never zero.
    assert!(
        body["residual"].is_null(),
        "an incomplete decomposition has an unknown residual, got {}",
        body["residual"]
    );
    assert!(body["residual_share"].is_null());

    let unavailable: Vec<_> = body["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| !c["unavailable"].is_null())
        .collect();
    assert!(
        !unavailable.is_empty(),
        "the causes that could not be measured must say so"
    );
    for component in unavailable {
        assert!(
            component["amount"].is_null(),
            "an unavailable cause must not carry an amount"
        );
    }
}

#[tokio::test]
async fn attribution_reports_the_gap_it_can_compute() {
    let (_, body) = client()
        .await
        .get("/api/v1/attribution?price_scale=2&qty_scale=2&live=config-moved&model=baseline")
        .await;
    // 481.5 - 123.456, computed by the framework from the two P&Ls.
    let gap = body["gap"].as_f64().unwrap();
    assert!((gap - 358.044).abs() < 1e-6, "gap was {gap}");
    assert_eq!(body["method"], "run-files");
    assert!(
        !body["missing_inputs"].as_array().unwrap().is_empty(),
        "the report must say what would make the missing causes available"
    );
}

#[tokio::test]
async fn half_a_funding_pair_is_not_taken_as_a_pair() {
    // Supplying only the venue's number would leave the model's at a
    // zero nobody provided, and produce a fee difference out of thin air.
    let (_, body) = client()
        .await
        .get("/api/v1/attribution?price_scale=2&qty_scale=2&live=config-moved&model=baseline&venue_fees=1.0")
        .await;
    let fee_component = body["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "fee tier")
        .expect("a fee component");
    assert!(
        !fee_component["unavailable"].is_null(),
        "half a pair must leave the component unavailable"
    );
}

#[tokio::test]
async fn a_supplied_fee_pair_becomes_a_measured_component() {
    let (_, body) = client()
        .await
        .get("/api/v1/attribution?price_scale=2&qty_scale=2&live=config-moved&model=baseline&venue_fees=2.5&model_fees=1.0")
        .await;
    let fee_component = body["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "fee tier")
        .expect("a fee component");
    assert!(
        fee_component["unavailable"].is_null(),
        "a supplied pair must be measured: {fee_component}"
    );
    assert!(fee_component["amount"].as_f64().is_some());
}

// -- live -----------------------------------------------------------------

#[tokio::test]
async fn a_deck_with_no_journal_directory_says_which_variable_to_set() {
    let (status, body) = client().await.get("/api/v1/journals").await;
    assert_eq!(status, StatusCode::PRECONDITION_REQUIRED);
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("OQ_DECK_JOURNALS_DIR")
    );
}

#[tokio::test]
async fn a_journal_that_will_not_read_is_listed_with_its_reason() {
    let dir = std::env::temp_dir().join(format!("oq-deck-journals-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("not-a-journal.oqj"), b"this is not a journal").unwrap();

    let client = Client::new(Settings {
        journals_dir: Some(dir.clone()),
        ..settings()
    })
    .await;
    let (status, body) = client.get("/api/v1/journals").await;
    assert_eq!(status, StatusCode::OK);
    let entries = body.as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["state"], "unreadable");
    assert!(!entries[0]["error"].as_str().unwrap().is_empty());

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn a_journal_id_cannot_escape_its_directory() {
    let client = Client::new(Settings {
        journals_dir: Some(fixtures()),
        ..settings()
    })
    .await;
    let (status, _) = client
        .get("/api/v1/journals/..%2F..%2Fetc%2Fpasswd/belief")
        .await;
    assert_ne!(status, StatusCode::OK);
}

/// A directory holding two runs and the day they traded on.
fn markout_dir() -> PathBuf {
    use oq_parity::manifest::RunManifest;
    use oq_parity::wire::Run;
    const S: i64 = 1_000_000_000;
    let dir = std::env::temp_dir().join(format!("oq-deck-markout-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ticks: Vec<oq_engine::Tick> = (0..120)
        .map(|i| {
            let p = 10_000 + i;
            oq_engine::Tick::trades_only(oq_types::Stamp::new(i * S, i * S), p, p, p)
        })
        .collect();
    std::fs::write(dir.join("day.oqtk"), oq_data::encode(1, &ticks)).unwrap();
    for (id, offset) in [("model", 0), ("live", 5)] {
        let fills = (0..40)
            .map(|i| oq_parity::Fill::new(i * S / 10, "X", oq_types::Side::Buy, 10_000 + offset, 1))
            .collect();
        let run = Run::new(
            RunManifest::from_content("c", b"d", b"g", "L0"),
            oq_parity::RunOutput::new(fills, 0.0),
        );
        std::fs::write(dir.join(format!("{id}.run")), run.render()).unwrap();
    }
    dir
}

#[tokio::test]
async fn markout_is_off_until_there_are_ticks_to_price_against() {
    let (_, body) = client().await.get("/api/v1/runtime/capabilities").await;
    assert_eq!(body["markout"]["available"], false);
    assert!(
        body["markout"]["reason"]
            .as_str()
            .unwrap()
            .contains("OQ_DECK_TICKS_DIR")
    );
}

#[tokio::test]
async fn two_runs_are_compared_by_markout_against_a_chosen_tick_file() {
    let dir = markout_dir();
    let client = Client::new(Settings {
        runs_dir: Some(dir.clone()),
        ticks_dir: Some(dir.clone()),
        ..settings()
    })
    .await;
    let (_, caps) = client.get("/api/v1/runtime/capabilities").await;
    assert_eq!(caps["markout"]["available"], true);

    let (status, ticks) = client.get("/api/v1/ticks").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ticks, serde_json::json!(["day"]));

    let (status, body) = client
        .get("/api/v1/runs/markout?baseline=model&candidate=live&ticks=day")
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["ticks"], "day");
    assert_eq!(body["baseline"]["horizons"][0]["samples"], 40);
    let difference = body["contrast"][0]["difference_bps"].as_f64().unwrap();
    assert!((difference + 5.0).abs() < 0.01, "{body}");

    let (status, _) = client
        .get("/api/v1/runs/markout?baseline=model&candidate=live&ticks=..%2Fday")
        .await;
    assert_ne!(
        status,
        StatusCode::OK,
        "a tick id cannot leave its directory"
    );
    std::fs::remove_dir_all(&dir).ok();
}
