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

/// The one-time token that bootstraps a deck with no password yet.
///
/// Printed to stderr, which is the terminal the operator started this
/// from. That is the whole security argument: reading it requires the
/// local access they already have, and it is gone when the process is.
/// It is never written to a file and never logged through `tracing`,
/// because a log is a thing that gets copied.
fn announce_setup(token: &str, host: &str, port: u16) {
    eprintln!();
    eprintln!("  ┌─ 首次运行 ─────────────────────────────────────────────");
    eprintln!("  │ 尚未设置密码。用下面这个一次性令牌完成初始设置：");
    eprintln!("  │");
    eprintln!("  │   http://{host}:{port}/setup");
    eprintln!("  │   {token}");
    eprintln!("  │");
    eprintln!("  │ 令牌只存在于本进程内存中，重启即失效。");
    eprintln!("  │ 在此之前，除初始设置外的所有接口都会拒绝请求。");
    eprintln!("  └────────────────────────────────────────────────────────");
    eprintln!();
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

    let setup_token = if settings.configured() {
        None
    } else {
        match oq_deck_core::auth::new_token() {
            Ok(token) => {
                announce_setup(&token, &settings.host.to_string(), settings.port);
                Some(token)
            }
            Err(error) => {
                eprintln!("oq-deck: 无法生成初始设置令牌：{error}");
                return ExitCode::FAILURE;
            }
        }
    };

    let address = (settings.host, settings.port);
    let mode = if settings.allow_writes {
        "writes enabled"
    } else {
        "read-only"
    };
    let auth = if settings.totp_secret.is_some() {
        "password + TOTP"
    } else if settings.configured() {
        "password"
    } else {
        "setup pending"
    };
    tracing::info!(
        "listening on http://{}:{} ({mode}, {auth})",
        address.0,
        address.1
    );

    let router = app::router(settings, web_dist(), setup_token);
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
