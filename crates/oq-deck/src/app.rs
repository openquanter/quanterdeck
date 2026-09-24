//! Application assembly, and the order the checks run in.
//!
//! Every request passes the same gauntlet, outermost first:
//!
//! 1. **`Host`** — is this a name we answer to? Stops DNS rebinding
//!    before anything else looks at the request.
//! 2. **`Origin`**, on writes — did this come from our own pages?
//! 3. **Session** — is there a live one? Only `/health`, the setup
//!    routes and the static files are exempt, and the setup routes have
//!    their own token.
//! 4. **Write mode** — is this deck allowed to change anything?
//!
//! The order matters: a request that fails the `Host` check never
//! reaches the session lookup, so an attacker cannot use timing there to
//! learn whether a session exists.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::{ConnectInfo, Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use oq_deck_core::{attribution, auth, capabilities, live, runs};
use serde::{Deserialize, Serialize};
use tower_http::services::{ServeDir, ServeFile};

use crate::guard::{Hosts, origin_permitted};
use crate::session::{self, Denied, Sessions};
use crate::settings::Settings;

#[derive(Clone)]
pub struct Deck {
    pub settings: Arc<Settings>,
    pub sessions: Arc<Sessions>,
    pub hosts: Arc<Hosts>,
    /// The one-time token printed at startup when no password is set.
    /// `None` once setup is done.
    /// Taken, not read, by a setup that succeeds: the token is single
    /// use, as the notes and the startup message say it is.
    pub setup_token: Arc<std::sync::Mutex<Option<String>>>,
}

/// A refusal the interface can act on.
///
/// Every failure carries a sentence meant for a person, because the
/// operator reading it is the one who has to do something about it —
/// with one deliberate exception: a failed login says only that it
/// failed, since anything more specific is how an attacker learns.
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

    fn bad_request(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, detail)
    }
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        (self.status, axum::Json(self)).into_response()
    }
}

// -- the gauntlet ---------------------------------------------------------

fn check_host(deck: &Deck, headers: &HeaderMap) -> Result<(), Refusal> {
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    if deck.hosts.permits(host) {
        return Ok(());
    }
    Err(Refusal::new(
        StatusCode::MISDIRECTED_REQUEST,
        format!(
            "拒绝服务该 Host。本 deck 只应答 {}。\
             一个指向 127.0.0.1 的外部域名正是 DNS rebinding 的形状；\
             若你在用反向代理，请把它的域名加进 OQ_DECK_EXTRA_HOSTS。",
            deck.hosts.names().join(", ")
        ),
    ))
}

fn check_origin(deck: &Deck, headers: &HeaderMap) -> Result<(), Refusal> {
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    if origin_permitted(origin, &deck.hosts, deck.settings.behind_tls) {
        return Ok(());
    }
    Err(Refusal::new(
        StatusCode::FORBIDDEN,
        "该写入请求的 Origin 不属于本 deck。跨站点的写入会被拒绝。",
    ))
}

fn check_session(deck: &Deck, headers: &HeaderMap) -> Result<(), Refusal> {
    let cookie = headers.get(header::COOKIE).and_then(|v| v.to_str().ok());
    let Some(token) = session::token_from_cookies(cookie) else {
        return Err(Refusal::new(StatusCode::UNAUTHORIZED, "请先登录。"));
    };
    match deck.sessions.touch(&token) {
        Ok(()) => Ok(()),
        Err(Denied::NoSession) => Err(Refusal::new(
            StatusCode::UNAUTHORIZED,
            "会话已失效，请重新登录。",
        )),
    }
}

/// Where a request came from, for counting failed attempts per source.
///
/// The peer address of the connection. Not `X-Forwarded-For`: a header
/// the client writes is not evidence of who the client is. Behind a
/// reverse proxy every request is the proxy's, and counting falls back
/// to one source — no worse than before, and live sessions are spared
/// either way.
fn source_of(peer: Option<&axum::Extension<ConnectInfo<SocketAddr>>>) -> String {
    peer.map_or_else(|| "unknown".to_owned(), |p| p.0.0.ip().to_string())
}

/// The deck's own configuration is broken, not the request.
fn misconfigured(what: &str) -> Refusal {
    Refusal::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("{what}这是配置问题，不是密码错误；本次尝试不计入失败次数。"),
    )
}

fn locked_out(deck: &Deck, source: &str) -> Refusal {
    let minutes = deck
        .sessions
        .lockout_remaining(source)
        .map_or(0, |left| left.as_secs().div_ceil(60));
    Refusal::new(
        StatusCode::TOO_MANY_REQUESTS,
        format!("失败次数过多，请在 {minutes} 分钟后再试。"),
    )
}

fn check_writes(deck: &Deck) -> Result<(), Refusal> {
    if deck.settings.allow_writes {
        return Ok(());
    }
    Err(Refusal::new(
        StatusCode::FORBIDDEN,
        "本 deck 处于只读模式；要修改任何东西，请先在设置中开启写入。",
    ))
}

/// Every check a read endpoint must pass.
fn guard_read(deck: &Deck, headers: &HeaderMap) -> Result<(), Refusal> {
    check_host(deck, headers)?;
    check_session(deck, headers)
}

/// Every check a write endpoint must pass.
#[allow(dead_code)]
fn guard_write(deck: &Deck, headers: &HeaderMap) -> Result<(), Refusal> {
    check_host(deck, headers)?;
    check_origin(deck, headers)?;
    check_session(deck, headers)?;
    check_writes(deck)
}

// -- unauthenticated ------------------------------------------------------

#[derive(Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
}

/// Liveness. The only route with no session, and it says nothing.
///
/// Deliberately silent about the runtime: a probe that went red when the
/// watched process went down would put a restart loop on the console at
/// the moment an operator needs it most. Deliberately silent about setup
/// state too, so it cannot be used to find a deck that has not been
/// configured yet.
async fn health() -> axum::Json<Health> {
    axum::Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[derive(Serialize)]
struct SessionState {
    authenticated: bool,
    /// True before the first password is set. The interface sends the
    /// operator to setup rather than to a login form they cannot pass.
    setup_required: bool,
    totp_required: bool,
}

/// Whether the caller is logged in. Safe to call without a session.
async fn whoami(State(deck): State<Deck>, headers: HeaderMap) -> Response {
    if let Err(refusal) = check_host(&deck, &headers) {
        return refusal.into_response();
    }
    let authenticated = check_session(&deck, &headers).is_ok();
    axum::Json(SessionState {
        authenticated,
        setup_required: !deck.settings.configured(),
        totp_required: deck.settings.totp_secret.is_some(),
    })
    .into_response()
}

#[derive(Deserialize)]
struct Login {
    password: String,
    #[serde(default)]
    totp: String,
}

/// Exchange a password for a session.
///
/// Failures are deliberately uninformative and deliberately slow to
/// repeat: the response says only that it failed, and five of them close
/// the door for fifteen minutes.
async fn login(
    State(deck): State<Deck>,
    peer: Option<axum::Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<Login>,
) -> Response {
    let source = source_of(peer.as_ref());
    if let Err(refusal) = check_host(&deck, &headers) {
        return refusal.into_response();
    }
    if let Err(refusal) = check_origin(&deck, &headers) {
        return refusal.into_response();
    }
    if deck.sessions.lockout_remaining(&source).is_some() {
        return locked_out(&deck, &source).into_response();
    }

    let Some(stored) = deck.settings.password_hash.as_ref() else {
        return Refusal::new(
            StatusCode::PRECONDITION_REQUIRED,
            "本 deck 尚未完成初始设置。",
        )
        .into_response();
    };

    let rejected = Refusal::new(StatusCode::UNAUTHORIZED, "密码或验证码不正确。");

    match auth::verify_password(&body.password, stored) {
        Ok(()) => {}
        Err(auth::AuthError::Rejected) => {
            deck.sessions.record_failure(&source);
            return rejected.into_response();
        }
        // The stored hash, not the attempt. Reported as itself and not
        // counted: read as a wrong password it sent the operator after a
        // password that was never the problem, and locked them out.
        Err(_) => return misconfigured("OQ_DECK_PASSWORD_HASH 无法解析。").into_response(),
    }

    if let Some(secret) = deck.settings.totp_secret.as_ref() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
        match auth::verify_totp(secret, &body.totp, now) {
            Ok(()) => {}
            Err(auth::AuthError::Rejected) => {
                deck.sessions.record_failure(&source);
                return rejected.into_response();
            }
            Err(_) => {
                return misconfigured("OQ_DECK_TOTP_SECRET 无法解析。").into_response();
            }
        }
    }

    match deck.sessions.issue(&source) {
        Ok(token) => (
            [(
                header::SET_COOKIE,
                session::set_cookie(&token, deck.settings.behind_tls),
            )],
            axum::Json(serde_json::json!({ "authenticated": true })),
        )
            .into_response(),
        Err(error) => Refusal::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("无法签发会话：{error}"),
        )
        .into_response(),
    }
}

async fn logout(State(deck): State<Deck>, headers: HeaderMap) -> Response {
    // A write like any other: it ends a session, and a page elsewhere
    // should not be able to end the operator's.
    if let Err(refusal) = check_origin(&deck, &headers) {
        return refusal.into_response();
    }
    let cookie = headers.get(header::COOKIE).and_then(|v| v.to_str().ok());
    if let Some(token) = session::token_from_cookies(cookie) {
        deck.sessions.revoke(&token);
    }
    (
        [(header::SET_COOKIE, session::clear_cookie())],
        axum::Json(serde_json::json!({ "authenticated": false })),
    )
        .into_response()
}

#[derive(Deserialize)]
struct SetupBody {
    /// The one-time token printed to the terminal at startup.
    token: String,
    password: String,
}

#[derive(Serialize)]
struct SetupDone {
    /// The Argon2id hash to persist, and the TOTP secret to enrol.
    ///
    /// Returned rather than written: this build keeps no configuration
    /// file, so the operator puts these in the environment. When there
    /// is a config file the deck will write them itself, and this shape
    /// will change.
    password_hash: String,
    totp_secret: String,
    next_steps: Vec<String>,
}

/// Turn the one-time token into credentials.
///
/// Only reachable while no password is set, and only by whoever can read
/// the terminal the deck was started from. That is the bootstrap: it
/// requires local access the operator already has, and it expires the
/// moment setup completes.
async fn setup(
    State(deck): State<Deck>,
    peer: Option<axum::Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<SetupBody>,
) -> Response {
    let source = source_of(peer.as_ref());
    if let Err(refusal) = check_host(&deck, &headers) {
        return refusal.into_response();
    }
    if let Err(refusal) = check_origin(&deck, &headers) {
        return refusal.into_response();
    }
    // Checked and taken under one lock, so two requests racing with the
    // same token cannot both be answered. It stayed valid after a setup
    // succeeded, until the process restarted: a second setup with it
    // issued a second hash and a second secret.
    let token = {
        let mut slot = deck.setup_token.lock().unwrap_or_else(|e| e.into_inner());
        let Some(expected) = slot.as_ref() else {
            return Refusal::new(StatusCode::CONFLICT, "初始设置已经完成。").into_response();
        };
        if !auth::secrets_match(expected, &body.token) {
            deck.sessions.record_failure(&source);
            return Refusal::new(StatusCode::UNAUTHORIZED, "一次性令牌不正确。").into_response();
        }
        // A password that is refused does not spend the token.
        if let Some(complaint) = auth::password_complaint(&body.password) {
            return Refusal::bad_request(complaint).into_response();
        }
        slot.take()
    };

    match (auth::hash_password(&body.password), auth::new_totp_secret()) {
        (Ok(password_hash), Ok(totp_secret)) => axum::Json(SetupDone {
            password_hash,
            totp_secret: totp_secret.clone(),
            next_steps: vec![
                "把 OQ_DECK_PASSWORD_HASH 设为上面的 hash，重启 deck。".to_owned(),
                "把 TOTP secret 录入验证器应用；要在非回环地址监听时它是必需的。".to_owned(),
                "两者都不要提交进 git，也不要写进任何日志。".to_owned(),
            ],
        })
        .into_response(),
        (Err(error), _) | (_, Err(error)) => {
            // Nothing was issued, so the token is given back.
            *deck.setup_token.lock().unwrap_or_else(|e| e.into_inner()) = token;
            Refusal::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("初始设置失败：{error}"),
            )
            .into_response()
        }
    }
}

// -- authenticated --------------------------------------------------------

async fn caps(State(deck): State<Deck>, headers: HeaderMap) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    axum::Json(capabilities::detect(
        deck.settings.runs_dir.as_deref(),
        deck.settings.journals_dir.as_deref(),
        deck.settings.allow_writes,
    ))
    .into_response()
}

fn dir_of(configured: Option<&PathBuf>, variable: &str) -> Result<PathBuf, Refusal> {
    configured.cloned().ok_or_else(|| {
        Refusal::new(
            StatusCode::PRECONDITION_REQUIRED,
            format!("尚未配置目录；请设置 {variable}"),
        )
    })
}

#[derive(Serialize)]
struct Listing {
    entries: Vec<runs::Entry>,
    total_pnl: f64,
}

async fn list_runs(State(deck): State<Deck>, headers: HeaderMap) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    match dir_of(deck.settings.runs_dir.as_ref(), "OQ_DECK_RUNS_DIR") {
        Ok(dir) => {
            let entries = runs::list(&dir);
            let total_pnl = runs::total_pnl(&entries);
            axum::Json(Listing { entries, total_pnl }).into_response()
        }
        Err(refusal) => refusal.into_response(),
    }
}

async fn run_detail(
    State(deck): State<Deck>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    match dir_of(deck.settings.runs_dir.as_ref(), "OQ_DECK_RUNS_DIR")
        .and_then(|dir| runs::detail(&dir, &id).map_err(Refusal::not_found))
    {
        Ok(detail) => axum::Json(detail).into_response(),
        Err(refusal) => refusal.into_response(),
    }
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
    headers: HeaderMap,
    Query(query): Query<CompareQuery>,
) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    match dir_of(deck.settings.runs_dir.as_ref(), "OQ_DECK_RUNS_DIR").and_then(|dir| {
        runs::compare_ids(&dir, &query.baseline, &query.candidate, query.tolerance)
            .map_err(Refusal::not_found)
    }) {
        Ok(comparison) => axum::Json(comparison).into_response(),
        Err(refusal) => refusal.into_response(),
    }
}

// -- attribution ----------------------------------------------------------

#[derive(Deserialize)]
struct AttributionQuery {
    live: String,
    model: String,
    #[serde(default = "default_scale")]
    price_scale: u8,
    #[serde(default = "default_scale")]
    qty_scale: u8,
    venue_funding: Option<f64>,
    model_funding: Option<f64>,
    venue_fees: Option<f64>,
    model_fees: Option<f64>,
}

const fn default_scale() -> u8 {
    2
}

impl AttributionQuery {
    /// Funding and fees are each a *pair*. Half a pair is not a partial
    /// answer, it is a missing one, and taking it would put a number in
    /// the report that was computed against a zero nobody supplied.
    fn pair(venue: Option<f64>, model: Option<f64>) -> Option<attribution::VenueVsModel> {
        match (venue, model) {
            (Some(venue), Some(model)) => Some(attribution::VenueVsModel { venue, model }),
            _ => None,
        }
    }
}

async fn attribution_report(
    State(deck): State<Deck>,
    headers: HeaderMap,
    Query(query): Query<AttributionQuery>,
) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    let inputs = attribution::Inputs {
        price_scale: query.price_scale,
        qty_scale: query.qty_scale,
        funding: AttributionQuery::pair(query.venue_funding, query.model_funding),
        fees: AttributionQuery::pair(query.venue_fees, query.model_fees),
    };
    match dir_of(deck.settings.runs_dir.as_ref(), "OQ_DECK_RUNS_DIR").and_then(|dir| {
        attribution::from_runs(&dir, &query.live, &query.model, inputs).map_err(Refusal::not_found)
    }) {
        Ok(report) => axum::Json(report).into_response(),
        Err(refusal) => refusal.into_response(),
    }
}

// -- live -----------------------------------------------------------------

async fn list_journals(State(deck): State<Deck>, headers: HeaderMap) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    match dir_of(deck.settings.journals_dir.as_ref(), "OQ_DECK_JOURNALS_DIR") {
        Ok(dir) => axum::Json(live::list(&dir)).into_response(),
        Err(refusal) => refusal.into_response(),
    }
}

async fn journal_belief(
    State(deck): State<Deck>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    match dir_of(deck.settings.journals_dir.as_ref(), "OQ_DECK_JOURNALS_DIR")
        .and_then(|dir| live::belief(&dir, &id).map_err(Refusal::not_found))
    {
        Ok(belief) => axum::Json(belief).into_response(),
        Err(refusal) => refusal.into_response(),
    }
}

#[derive(Deserialize)]
struct ReconcileBody {
    /// The text `oq-recon --record` wrote. Pasted by the operator, since
    /// the console holds no venue credentials and will not grow any.
    venue_record: String,
}

async fn reconcile(
    State(deck): State<Deck>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    axum::Json(body): axum::Json<ReconcileBody>,
) -> Response {
    // A read, but by POST because the venue record does not belong in a
    // URL: query strings reach logs, shell history and referrers, and
    // this one describes a live account.
    if let Err(refusal) = check_host(&deck, &headers) {
        return refusal.into_response();
    }
    if let Err(refusal) = check_origin(&deck, &headers) {
        return refusal.into_response();
    }
    if let Err(refusal) = check_session(&deck, &headers) {
        return refusal.into_response();
    }
    match dir_of(deck.settings.journals_dir.as_ref(), "OQ_DECK_JOURNALS_DIR").and_then(|dir| {
        live::reconcile(&dir, &id, &body.venue_record).map_err(Refusal::bad_request)
    }) {
        Ok(result) => axum::Json(result).into_response(),
        Err(refusal) => refusal.into_response(),
    }
}

// -- assembly -------------------------------------------------------------

/// Build the router.
///
/// `web_dist` is the built interface. It is optional so the API can be
/// developed and tested without Node having ever run.
pub fn router(
    settings: Settings,
    web_dist: Option<PathBuf>,
    setup_token: Option<String>,
) -> Router {
    let hosts = Hosts::for_bind(settings.host, settings.port, &settings.extra_hosts);
    let deck = Deck {
        settings: Arc::new(settings),
        sessions: Arc::new(Sessions::new()),
        hosts: Arc::new(hosts),
        setup_token: Arc::new(std::sync::Mutex::new(setup_token)),
    };

    let api = Router::new()
        .route("/health", get(health))
        .route("/session", get(whoami))
        .route("/session/login", post(login))
        .route("/session/logout", post(logout))
        .route("/setup", post(setup))
        .route("/runtime/capabilities", get(caps))
        .route("/runs", get(list_runs))
        .route("/runs/compare", get(compare_runs))
        .route("/runs/{id}", get(run_detail))
        .route("/attribution", get(attribution_report))
        .route("/journals", get(list_journals))
        .route("/journals/{id}/belief", get(journal_belief))
        .route("/journals/{id}/reconcile", post(reconcile))
        // An API path that does not exist is a 404 in the API's own
        // shape, not the interface's index page with a 200 — which told a
        // client asking for a mistyped route that it had succeeded.
        .fallback(api_not_found)
        .layer(axum::middleware::from_fn(no_store))
        .with_state(deck.clone());

    let router = Router::new().nest("/api/v1", api);

    let router = match web_dist {
        // The interface is history-routed, so `/runs/42` is a URL a user
        // can reload or paste to a colleague and there is no file behind
        // it. `fallback_service` turns those into the app instead of a
        // 404; the API is nested above and never reaches it.
        Some(dist) if dist.is_dir() => {
            let index = dist.join("index.html");
            router.fallback_service(ServeDir::new(dist).fallback(ServeFile::new(index)))
        }
        _ => router,
    };

    // Outermost, so nothing — a static file, `/health`, a body being
    // parsed — is reached by a request with a Host this deck does not
    // answer to. The handlers' own checks stay; this is the one that
    // cannot be forgotten on a new route.
    router
        .layer(axum::middleware::from_fn(security_headers))
        .layer(axum::middleware::from_fn_with_state(deck, host_first))
}

async fn host_first(
    State(deck): State<Deck>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    if let Err(refusal) = check_host(&deck, request.headers()) {
        return refusal.into_response();
    }
    next.run(request).await
}

/// Headers every response carries.
///
/// The console shows account figures and can, with writes enabled, place
/// orders; it has no reason to be framed, to have its types sniffed, to
/// leak its URLs in a referrer, or to load a script from anywhere but
/// itself.
async fn security_headers(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let mut response = next.run(request).await;
    let h = response.headers_mut();
    let set = |h: &mut HeaderMap, name: header::HeaderName, value: &'static str| {
        h.insert(name, header::HeaderValue::from_static(value));
    };
    set(h, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    set(h, header::X_FRAME_OPTIONS, "DENY");
    set(h, header::REFERRER_POLICY, "no-referrer");
    set(
        h,
        header::CONTENT_SECURITY_POLICY,
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; \
         base-uri 'none'; form-action 'self'",
    );
    response
}

/// API answers are about the account; no cache should keep them.
async fn no_store(request: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

async fn api_not_found() -> Response {
    Refusal::new(StatusCode::NOT_FOUND, "没有这个 API 路径。").into_response()
}
