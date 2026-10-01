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
///
/// Which is why it is not printed when stderr is *not* a terminal. Under
/// a service manager stderr is the journal: it persists, every log
/// collector copies it, and a group may be able to read it — a token
/// printed there has been written down, and this comment used to claim
/// otherwise while doing exactly that.
fn announce_setup(token: &str, host: &str, port: u16) {
    use std::io::IsTerminal;
    if !std::io::stderr().is_terminal() {
        let runtime = std::env::var_os("RUNTIME_DIRECTORY").map(std::path::PathBuf::from);
        match runtime {
            Some(dir) if oq_deck_core::write_private(&dir.join("setup-token"), token).is_ok() => {
                let path = dir.join("setup-token");
                eprintln!();
                eprintln!("  ┌─ 首次运行 ─────────────────────────────────────────────");
                eprintln!("  │ 尚未设置密码。stderr 不是终端（服务管理器会把它写进日志），");
                eprintln!("  │ 所以令牌没有打印出来，而是写在这里：");
                eprintln!("  │");
                eprintln!("  │   http://{host}:{port}/setup");
                eprintln!("  │   一次性令牌在 {}", path.display());
                eprintln!("  │   （仅本用户可读，进程结束即消失。）");
                eprintln!("  └────────────────────────────────────────────────────────");
                eprintln!();
                return;
            }
            _ => {
                eprintln!();
                eprintln!("  ┌─ 首次运行 ─────────────────────────────────────────────");
                eprintln!("  │ 尚未设置密码，但 stderr 不是终端——服务管理器会把它写进日志，");
                eprintln!("  │ 而日志是会被复制的东西，所以令牌没有打印、也没有写成文件。");
                eprintln!("  │");
                eprintln!("  │ 二选一：在终端里手动启动一次本进程取得令牌；");
                eprintln!("  │ 或给本服务加一个可写的 RuntimeDirectory，令牌会写进那里。");
                eprintln!("  └────────────────────────────────────────────────────────");
                eprintln!();
                return;
            }
        }
    }
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

    // Opened here rather than inside the router: a file that will not
    // parse is a refusal to start, which is a decision, and a library
    // that panics on bad input is not one.
    let devices = match settings.state_dir.as_deref() {
        Some(dir) => match std::fs::create_dir_all(dir)
            .map_err(|e| e.to_string())
            .and_then(|()| oq_deck_core::devices::Devices::open(dir))
        {
            Ok(store) => Some(std::sync::Arc::new(std::sync::Mutex::new(store))),
            Err(error) => {
                eprintln!(
                    "oq-deck: 无法读取已登记的设备（{}）：{error}",
                    dir.display()
                );
                eprintln!("oq-deck: 拒绝启动——把它当成“没有设备”会把已登记的浏览器全部登出。");
                return ExitCode::FAILURE;
            }
        },
        None => None,
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

    // Started here, not in the router: the tests build routers too, and
    // a test must never reach GitHub. Off, it makes no request at all.
    let upstream = oq_deck::upstream::Upstream::from_settings(&settings);
    if upstream.enabled() {
        tracing::info!(
            "checking {} for new releases every {} h",
            settings.upstream_repo,
            settings.upstream_every_hours
        );
    }
    upstream.spawn();
    let router = app::router_with_upstream(settings, web_dist(), setup_token, devices, upstream);
    let listener = match tokio::net::TcpListener::bind(address).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("oq-deck: cannot bind {}:{}: {error}", address.0, address.1);
            return ExitCode::FAILURE;
        }
    };

    // With the peer address attached, so failed logins are counted per
    // source rather than across everyone who can reach the port.
    if let Err(error) = axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
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

#[cfg(test)]
mod setup_token {

    /// The token is readable by this user and no other. A file whose
    /// mode depends on the umask is a file somebody else may read.
    #[test]
    fn the_token_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("setup-token");
        oq_deck_core::write_private(&path, "0123456789abcdef").expect("written");
        let mode = std::fs::metadata(&path).expect("stat").permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{mode:o}");
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            "0123456789abcdef\n"
        );
    }
}
