//! `oq-deck` — the OpenQuanter console.

use std::path::PathBuf;
use std::process::ExitCode;

use oq_deck::app;
use oq_deck::settings::Settings;

/// Where the built interface lives next to the binary, when it does.
fn web_dist() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("OQ_DECK_WEB_DIST") {
        return Some(PathBuf::from(dir));
    }
    let candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../web/dist")
        .canonicalize()
        .ok()?;
    candidate.is_dir().then_some(candidate)
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "oq_deck=info,tower_http=warn".into()),
        )
        .init();

    let settings = match Settings::from_env() {
        Ok(settings) => settings,
        Err(error) => {
            // Refusing to start is the feature. Say what to do about it.
            eprintln!("oq-deck: {error}");
            return ExitCode::FAILURE;
        }
    };

    let address = (settings.host, settings.port);
    let mode = if settings.allow_writes {
        "writes enabled"
    } else {
        "read-only"
    };
    tracing::info!("listening on http://{}:{} ({mode})", address.0, address.1);

    let router = app::router(settings, web_dist());
    let listener = match tokio::net::TcpListener::bind(address).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("oq-deck: cannot bind {}:{}: {error}", address.0, address.1);
            return ExitCode::FAILURE;
        }
    };

    if let Err(error) = axum::serve(listener, router)
        .with_graceful_shutdown(shutdown())
        .await
    {
        eprintln!("oq-deck: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}
