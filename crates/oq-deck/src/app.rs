//! Application assembly.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use oq_deck_core::{capabilities, runs};
use serde::{Deserialize, Serialize};
use tower_http::services::{ServeDir, ServeFile};

use crate::settings::Settings;

#[derive(Clone)]
pub struct Deck {
    pub settings: Arc<Settings>,
}

/// An error the interface can act on.
///
/// Every failure carries a sentence meant for a person, because the
/// operator reading it is the one who has to do something about it.
/// A small type rather than a built `Response`, so a handler can return
/// it through `?` without carrying a kilobyte of HTTP machinery in its
/// error variant.
#[derive(Debug, Serialize)]
pub struct Refusal {
    #[serde(skip)]
    status: StatusCode,
    detail: String,
}

impl Refusal {
    fn new(status: StatusCode, detail: impl Into<String>) -> Self {
        Self {
            status,
            detail: detail.into(),
        }
    }

    fn not_found(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, detail)
    }
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        (self.status, axum::Json(self)).into_response()
    }
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
}

/// Liveness. Deliberately says nothing about the runtime.
///
/// A probe that reported the runtime's health here would go red when the
/// thing being watched went down, and a restart loop on the console is
/// the last thing an operator needs at that moment.
async fn health() -> axum::Json<Health> {
    axum::Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn caps(State(deck): State<Deck>) -> axum::Json<capabilities::Capabilities> {
    axum::Json(capabilities::detect(
        deck.settings.runs_dir.as_deref(),
        deck.settings.allow_writes,
    ))
}

fn runs_dir(deck: &Deck) -> Result<PathBuf, Refusal> {
    deck.settings.runs_dir.clone().ok_or_else(|| {
        Refusal::new(
            StatusCode::PRECONDITION_REQUIRED,
            "no runs directory configured; set OQ_DECK_RUNS_DIR",
        )
    })
}

#[derive(Serialize)]
struct Listing {
    entries: Vec<runs::Entry>,
    total_pnl: f64,
}

async fn list_runs(State(deck): State<Deck>) -> Result<axum::Json<Listing>, Refusal> {
    let entries = runs::list(&runs_dir(&deck)?);
    let total_pnl = runs::total_pnl(&entries);
    Ok(axum::Json(Listing { entries, total_pnl }))
}

async fn run_detail(
    State(deck): State<Deck>,
    AxumPath(id): AxumPath<String>,
) -> Result<axum::Json<runs::RunDetail>, Refusal> {
    let detail = runs::detail(&runs_dir(&deck)?, &id).map_err(Refusal::not_found)?;
    Ok(axum::Json(detail))
}

#[derive(Deserialize)]
struct CompareQuery {
    baseline: String,
    candidate: String,
    /// Relative P&L tolerance. Defaults to exact, because a tolerance
    /// nobody chose is a tolerance nobody can defend.
    #[serde(default)]
    tolerance: f64,
}

async fn compare_runs(
    State(deck): State<Deck>,
    Query(query): Query<CompareQuery>,
) -> Result<axum::Json<runs::Comparison>, Refusal> {
    let dir = runs_dir(&deck)?;
    let comparison = runs::compare_ids(&dir, &query.baseline, &query.candidate, query.tolerance)
        .map_err(Refusal::not_found)?;
    Ok(axum::Json(comparison))
}

/// Build the router.
///
/// `web_dist` is the built interface. It is optional so the API can be
/// developed and tested without Node having ever run.
pub fn router(settings: Settings, web_dist: Option<PathBuf>) -> Router {
    let deck = Deck {
        settings: Arc::new(settings),
    };

    let api = Router::new()
        .route("/health", get(health))
        .route("/runtime/capabilities", get(caps))
        .route("/runs", get(list_runs))
        .route("/runs/compare", get(compare_runs))
        .route("/runs/{id}", get(run_detail))
        .with_state(deck);

    let router = Router::new().nest("/api/v1", api);

    match web_dist {
        // The interface is history-routed, so `/runs/42` is a URL a user
        // can reload or paste to a colleague and there is no file behind
        // it. `not_found_service` is what turns those into the app
        // instead of a 404; the API is nested above and never reaches it.
        Some(dist) if dist.is_dir() => {
            let index = dist.join("index.html");
            router.fallback_service(ServeDir::new(dist).fallback(ServeFile::new(index)))
        }
        _ => router,
    }
}
