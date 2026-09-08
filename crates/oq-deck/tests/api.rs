//! The HTTP surface.

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oq_deck::app::router;
use oq_deck::settings::Settings;
use serde_json::Value;
use tower::ServiceExt;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/fixtures/runs")
}

fn deck(runs: bool) -> axum::Router {
    let settings = Settings {
        runs_dir: runs.then(fixtures),
        ..Settings::default()
    };
    router(settings, None)
}

async fn get(app: axum::Router, uri: &str) -> (StatusCode, Value) {
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn health_says_nothing_about_the_runtime() {
    let (status, body) = get(deck(true), "/api/v1/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert!(body.get("runs").is_none());
}

#[tokio::test]
async fn capabilities_explain_what_is_off() {
    let (_, body) = get(deck(true), "/api/v1/runtime/capabilities").await;
    assert_eq!(body["runs"]["available"], true);
    assert_eq!(body["attribution"]["available"], false);
    assert!(
        !body["attribution"]["reason"].as_str().unwrap().is_empty(),
        "a capability that is off must say why"
    );
    assert_eq!(
        body["writes"]["available"], false,
        "read-only is the default"
    );
}

#[tokio::test]
async fn a_deck_with_no_runs_directory_says_so_rather_than_showing_nothing() {
    let (status, body) = get(deck(false), "/api/v1/runs").await;
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
    let (status, body) = get(deck(true), "/api/v1/runs").await;
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
    let (status, body) = get(deck(true), "/api/v1/runs/baseline").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["identity"]["code_commit"], "a1b2c3d");
    assert_eq!(body["fills"].as_array().unwrap().len(), 2);
    assert!(body["fills"][0]["tag"].is_null());
}

#[tokio::test]
async fn a_traversal_attempt_is_a_missing_run_not_a_file() {
    let (status, _) = get(deck(true), "/api/v1/runs/..%2F..%2Fetc%2Fpasswd").await;
    assert_ne!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_stale_baseline_never_reports_as_agreement() {
    let (status, body) = get(
        deck(true),
        "/api/v1/runs/compare?baseline=baseline&candidate=config-moved",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["verdict"]["status"], "invalidated");
    assert_eq!(body["passes"], false);
    assert_eq!(body["differences"], 0);
}

#[tokio::test]
async fn identical_output_under_new_code_passes() {
    let (_, body) = get(
        deck(true),
        "/api/v1/runs/compare?baseline=baseline&candidate=same-experiment",
    )
    .await;
    assert_eq!(body["verdict"]["status"], "code_changed");
    assert_eq!(body["passes"], true);
}
