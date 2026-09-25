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
use oq_deck_core::{attribution, auth, capabilities, live, markout, ops, runs, sweeps};
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
    // Fetch Metadata, on top of Origin rather than instead of it: a
    // browser that sends `Sec-Fetch-Site` says where the request came
    // from in words a page cannot choose. Anything but this origin or
    // the user's own navigation is refused. Its absence — an older
    // browser, a script — leaves the Origin check to decide.
    let site = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok());
    if site.is_some_and(|s| !matches!(s, "same-origin" | "none")) {
        return Err(Refusal::new(
            StatusCode::FORBIDDEN,
            "该写入请求来自其他站点（Sec-Fetch-Site），跨站点的写入会被拒绝。",
        ));
    }
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
                "把 TOTP secret 录入验证器应用，并设为 OQ_DECK_TOTP_SECRET；deck 对外可达时（非回环地址、反向代理或 OQ_DECK_EXTRA_HOSTS）它是必需的。".to_owned(),
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
        deck.settings.ticks_dir.as_deref(),
        deck.settings.agent_socket.as_deref(),
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
    /// `null` when no total means anything: a run would not read, or the
    /// runs are of different kinds.
    total_pnl: Option<f64>,
}

fn unreadable_dir(why: String) -> Refusal {
    Refusal::new(
        StatusCode::SERVICE_UNAVAILABLE,
        format!("目录无法读取，所以无法判断其中有什么：{why}"),
    )
}

async fn list_runs(State(deck): State<Deck>, headers: HeaderMap) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    match dir_of(deck.settings.runs_dir.as_ref(), "OQ_DECK_RUNS_DIR") {
        Ok(dir) => match runs::list(&dir) {
            Ok(entries) => {
                let total_pnl = runs::total_pnl(&entries);
                axum::Json(Listing { entries, total_pnl }).into_response()
            }
            Err(why) => unreadable_dir(why).into_response(),
        },
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

/// The sweeps a caller's program wrote beside its runs.
async fn list_sweeps(State(deck): State<Deck>, headers: HeaderMap) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    match dir_of(deck.settings.runs_dir.as_ref(), "OQ_DECK_RUNS_DIR") {
        Ok(dir) => match sweeps::list(&dir) {
            Ok(entries) => axum::Json(entries).into_response(),
            Err(why) => unreadable_dir(why).into_response(),
        },
        Err(refusal) => refusal.into_response(),
    }
}

async fn sweep_detail(
    State(deck): State<Deck>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    match dir_of(deck.settings.runs_dir.as_ref(), "OQ_DECK_RUNS_DIR")
        .and_then(|dir| sweeps::detail(&dir, &id).map_err(Refusal::not_found))
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

// -- markout ----------------------------------------------------------------

async fn list_ticks(State(deck): State<Deck>, headers: HeaderMap) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    match dir_of(deck.settings.ticks_dir.as_ref(), "OQ_DECK_TICKS_DIR") {
        Ok(dir) => match markout::list_ticks(&dir) {
            Ok(ids) => axum::Json(ids).into_response(),
            Err(why) => unreadable_dir(why).into_response(),
        },
        Err(refusal) => refusal.into_response(),
    }
}

#[derive(Deserialize)]
struct MarkoutQuery {
    baseline: String,
    candidate: String,
    ticks: String,
}

async fn markout_runs(
    State(deck): State<Deck>,
    headers: HeaderMap,
    Query(query): Query<MarkoutQuery>,
) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    let dirs = dir_of(deck.settings.runs_dir.as_ref(), "OQ_DECK_RUNS_DIR").and_then(|runs| {
        dir_of(deck.settings.ticks_dir.as_ref(), "OQ_DECK_TICKS_DIR").map(|ticks| (runs, ticks))
    });
    // Reading a tick file is the one request here that is not bounded by
    // the size of a run: it walks the whole file. Off the async workers,
    // so a large one does not stall every other request.
    let result = match dirs {
        Ok((runs, ticks)) => tokio::task::spawn_blocking(move || {
            markout::compare(
                &runs,
                &ticks,
                &query.baseline,
                &query.candidate,
                &query.ticks,
            )
        })
        .await
        .map_err(|e| Refusal::not_found(e.to_string()))
        .and_then(|r| r.map_err(Refusal::not_found)),
        Err(refusal) => Err(refusal),
    };
    match result {
        Ok(comparison) => axum::Json(comparison).into_response(),
        Err(refusal) => refusal.into_response(),
    }
}

// -- attribution ----------------------------------------------------------

#[derive(Deserialize)]
struct AttributionQuery {
    live: String,
    model: String,
    /// Required, and checked after the session so an unauthenticated
    /// request still reads 401. They defaulted to 2 — a precision nobody
    /// chose, which the tolerance beside it says is indefensible: set
    /// wrongly, every explained component was off by a power of ten and
    /// still shown as measured.
    price_scale: Option<u8>,
    qty_scale: Option<u8>,
    venue_funding: Option<f64>,
    model_funding: Option<f64>,
    venue_fees: Option<f64>,
    model_fees: Option<f64>,
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
    let (Some(price_scale), Some(qty_scale)) = (query.price_scale, query.qty_scale) else {
        return Refusal::bad_request(
            "需要 price_scale 和 qty_scale：归因金额按合约的价格与数量精度计算，\
             没有默认值可以替你选。",
        )
        .into_response();
    };
    let inputs = attribution::Inputs {
        price_scale,
        qty_scale,
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
        Ok(dir) => match live::list(&dir) {
            Ok(entries) => axum::Json(entries).into_response(),
            Err(why) => unreadable_dir(why).into_response(),
        },
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

/// The newest journal against the newest venue reading, with no pasting.
///
/// The reading is the file `oq-recon --watch --latest` keeps; its age is
/// returned beside the verdict, because a comparison against a reading an
/// hour old is a statement about an hour ago.
async fn reconcile_latest(State(deck): State<Deck>, headers: HeaderMap) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    let Some(record_path) = deck.settings.venue_record.clone() else {
        return Refusal::not_found("尚未配置交易所最新记录（OQ_DECK_VENUE_RECORD）")
            .into_response();
    };
    let dir = match dir_of(deck.settings.journals_dir.as_ref(), "OQ_DECK_JOURNALS_DIR") {
        Ok(d) => d,
        Err(refusal) => return refusal.into_response(),
    };
    let newest = std::fs::read_dir(&dir).ok().and_then(|rd| {
        rd.filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "oqj"))
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .max_by_key(|(t, _)| *t)
            .and_then(|(_, p)| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
    });
    let Some(id) = newest else {
        return Refusal::not_found("日志目录里还没有交易日志").into_response();
    };
    let text = match std::fs::read_to_string(&record_path) {
        Ok(t) => t,
        Err(e) => {
            return Refusal::not_found(format!("读不到交易所记录 {}：{e}", record_path.display()))
                .into_response();
        }
    };
    match live::reconcile(&dir, &id, &text) {
        Ok(result) => {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0));
            let age_ms = now_ms - result.venue.read_at_ms;
            axum::Json(serde_json::json!({"reconciliation": result, "record_age_ms": age_ms}))
                .into_response()
        }
        Err(e) => Refusal::bad_request(e).into_response(),
    }
}

// -- operations, through the host agent ------------------------------------
//
// The deck authenticates the person; the agent decides what is allowed and
// verifies the step-up code for anything risky. These handlers only carry
// requests across and name who asked.

async fn ask_agent(
    deck: &Deck,
    op: ops::Op,
    reason: Option<String>,
    step_up: Option<String>,
    actor: String,
) -> Response {
    let Some(sock) = deck.settings.agent_socket.clone() else {
        return Refusal::not_found("尚未配置主机代理（OQ_DECK_AGENT_SOCKET）").into_response();
    };
    let nonce = match auth::new_token() {
        Ok(n) => n,
        Err(e) => return misconfigured(&format!("无法生成请求编号：{e}。")).into_response(),
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0));
    let req = ops::AgentRequest {
        op,
        actor,
        nonce,
        expires_ms: now_ms + 30_000,
        reason,
        step_up,
    };
    match agent_call(&sock, &req).await {
        Ok(resp) if resp.ok => axum::Json(resp.data).into_response(),
        Ok(resp) => Refusal::new(
            StatusCode::CONFLICT,
            resp.error.unwrap_or_else(|| "主机代理拒绝了请求".into()),
        )
        .into_response(),
        Err(e) => {
            Refusal::new(StatusCode::BAD_GATEWAY, format!("主机代理无应答：{e}")).into_response()
        }
    }
}

async fn agent_call(
    sock: &std::path::Path,
    req: &ops::AgentRequest,
) -> Result<ops::AgentResponse, String> {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
    let work = async {
        let stream = tokio::net::UnixStream::connect(sock)
            .await
            .map_err(|e| e.to_string())?;
        let (read, mut write) = stream.into_split();
        let mut line = serde_json::to_string(req).map_err(|e| e.to_string())?;
        line.push('\n');
        write
            .write_all(line.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        let mut answer = String::new();
        tokio::io::BufReader::new(read.take(8 << 20))
            .read_line(&mut answer)
            .await
            .map_err(|e| e.to_string())?;
        serde_json::from_str::<ops::AgentResponse>(&answer)
            .map_err(|e| format!("无法读取应答：{e}"))
    };
    tokio::time::timeout(std::time::Duration::from_secs(60), work)
        .await
        .map_err(|_| "超时".to_string())?
}

/// Who is asking, for the agent's audit trail: the deck's one operator,
/// and where from.
fn actor_of(
    peer: Option<&axum::Extension<ConnectInfo<SocketAddr>>>,
    headers: &HeaderMap,
) -> String {
    // Behind the proxy the socket peer is the proxy; the address it
    // forwards is the person's.
    let forwarded = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|v| !v.is_empty());
    format!(
        "deck:operator@{}",
        forwarded.map_or_else(|| source_of(peer), str::to_owned)
    )
}

macro_rules! ops_read {
    ($name:ident, $op:expr) => {
        async fn $name(
            State(deck): State<Deck>,
            peer: Option<axum::Extension<ConnectInfo<SocketAddr>>>,
            headers: HeaderMap,
        ) -> Response {
            if let Err(refusal) = guard_read(&deck, &headers) {
                return refusal.into_response();
            }
            let actor = actor_of(peer.as_ref(), &headers);
            ask_agent(&deck, $op, None, None, actor).await
        }
    };
}

ops_read!(ops_host, ops::Op::Host);
ops_read!(ops_units, ops::Op::Units);
ops_read!(ops_status, ops::Op::Status);
ops_read!(ops_orders, ops::Op::Orders);
ops_read!(ops_alerts, ops::Op::Alerts);
ops_read!(ops_logs, ops::Op::Logs);
ops_read!(ops_releases, ops::Op::Releases);
ops_read!(ops_attribution, ops::Op::Attribution);
ops_read!(ops_accounts, ops::Op::Accounts);
ops_read!(ops_configs, ops::Op::ConfigList);
ops_read!(ops_strategies, ops::Op::Strategies);

#[derive(Deserialize)]
struct ResourcesQuery {
    #[serde(default = "default_hours")]
    hours: i64,
}

const fn default_hours() -> i64 {
    24
}

async fn ops_resources(
    State(deck): State<Deck>,
    peer: Option<axum::Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Query(q): Query<ResourcesQuery>,
) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    let actor = actor_of(peer.as_ref(), &headers);
    ask_agent(
        &deck,
        ops::Op::Resources { hours: q.hours },
        None,
        None,
        actor,
    )
    .await
}

#[derive(Deserialize)]
struct BlackboxQuery {
    from: i64,
    to: i64,
    #[serde(default = "default_points")]
    points: usize,
}

const fn default_points() -> usize {
    400
}

async fn ops_blackbox(
    State(deck): State<Deck>,
    peer: Option<axum::Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Query(q): Query<BlackboxQuery>,
) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    let actor = actor_of(peer.as_ref(), &headers);
    let op = ops::Op::Blackbox {
        from_ms: q.from,
        to_ms: q.to,
        points: q.points,
    };
    ask_agent(&deck, op, None, None, actor).await
}

#[derive(Deserialize)]
struct AtQuery {
    at: i64,
}

async fn ops_blackbox_at(
    State(deck): State<Deck>,
    peer: Option<axum::Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Query(q): Query<AtQuery>,
) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    let actor = actor_of(peer.as_ref(), &headers);
    ask_agent(
        &deck,
        ops::Op::BlackboxAt { at_ms: q.at },
        None,
        None,
        actor,
    )
    .await
}

#[derive(Deserialize)]
struct JournalLogQuery {
    unit: String,
    since: Option<i64>,
    until: Option<i64>,
    #[serde(default = "default_lines")]
    lines: usize,
    #[serde(default)]
    grep: Option<String>,
}

async fn ops_journal(
    State(deck): State<Deck>,
    peer: Option<axum::Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Query(q): Query<JournalLogQuery>,
) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    let actor = actor_of(peer.as_ref(), &headers);
    let op = ops::Op::JournalLog {
        unit: q.unit,
        since_ms: q.since,
        until_ms: q.until,
        lines: q.lines,
        grep: q.grep.filter(|g| !g.is_empty()),
    };
    ask_agent(&deck, op, None, None, actor).await
}

#[derive(Deserialize)]
struct ConfigQuery {
    name: String,
    backup: Option<String>,
}

async fn ops_config(
    State(deck): State<Deck>,
    peer: Option<axum::Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Query(q): Query<ConfigQuery>,
) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    let actor = actor_of(peer.as_ref(), &headers);
    let op = ops::Op::ConfigGet {
        name: q.name,
        backup: q.backup.filter(|b| !b.is_empty()),
    };
    ask_agent(&deck, op, None, None, actor).await
}

#[derive(Deserialize)]
struct RecordsQuery {
    /// Comma-separated kinds; empty for all.
    #[serde(default)]
    kinds: String,
    #[serde(default = "default_lines")]
    limit: usize,
    before: Option<u64>,
    /// Only records at or after / before these, in nanoseconds.
    from_ns: Option<i64>,
    to_ns: Option<i64>,
}

async fn journal_records(
    State(deck): State<Deck>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Query(q): Query<RecordsQuery>,
) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    let kinds: Vec<String> = q
        .kinds
        .split(',')
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(str::to_owned)
        .collect();
    let dir = match dir_of(deck.settings.journals_dir.as_ref(), "OQ_DECK_JOURNALS_DIR") {
        Ok(d) => d,
        Err(refusal) => return refusal.into_response(),
    };
    // Reading a journal is file work, off the async workers.
    let result = tokio::task::spawn_blocking(move || {
        live::records_between(&dir, &id, &kinds, q.limit, q.before, q.from_ns, q.to_ns)
    })
    .await;
    match result {
        Ok(Ok(page)) => axum::Json(page).into_response(),
        Ok(Err(e)) => Refusal::not_found(e).into_response(),
        Err(e) => Refusal::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// What this deck is configured with, for the settings page. Paths and
/// switches only; nothing secret is here to show.
async fn runtime_settings(State(deck): State<Deck>, headers: HeaderMap) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    let s = &deck.settings;
    let path = |p: &Option<PathBuf>| p.as_ref().map(|p| p.display().to_string());
    axum::Json(serde_json::json!({
        "listen": format!("{}:{}", s.host, s.port),
        "behind_tls": s.behind_tls,
        "extra_hosts": s.extra_hosts,
        "totp": s.totp_secret.is_some(),
        "allow_writes": s.allow_writes,
        "runs_dir": path(&s.runs_dir),
        "journals_dir": path(&s.journals_dir),
        "ticks_dir": path(&s.ticks_dir),
        "agent_socket": path(&s.agent_socket),
        "venue_record": path(&s.venue_record),
        "session": {"idle_minutes": 60, "absolute_hours": 12},
        "version": env!("CARGO_PKG_VERSION"),
    }))
    .into_response()
}

#[derive(Deserialize)]
struct LogQuery {
    name: String,
    #[serde(default = "default_lines")]
    lines: usize,
    #[serde(default)]
    grep: Option<String>,
}

const fn default_lines() -> usize {
    200
}

async fn ops_log(
    State(deck): State<Deck>,
    peer: Option<axum::Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<LogQuery>,
) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    let actor = actor_of(peer.as_ref(), &headers);
    let op = ops::Op::LogTail {
        name: q.name,
        lines: q.lines,
        grep: q.grep.filter(|g| !g.is_empty()),
    };
    ask_agent(&deck, op, None, None, actor).await
}

#[derive(Deserialize)]
struct AuditQuery {
    #[serde(default = "default_lines")]
    lines: usize,
}

async fn ops_audit(
    State(deck): State<Deck>,
    peer: Option<axum::Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<AuditQuery>,
) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    let actor = actor_of(peer.as_ref(), &headers);
    ask_agent(&deck, ops::Op::Audit { lines: q.lines }, None, None, actor).await
}

#[derive(Deserialize)]
struct ActionBody {
    action: String,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    verb: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    base_sha: Option<String>,
    #[serde(default)]
    backup: Option<String>,
    #[serde(default)]
    config: Option<String>,
    #[serde(default)]
    run: Option<String>,
    #[serde(default)]
    passed: Option<bool>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    minutes: Option<i64>,
    reason: String,
    #[serde(default)]
    step_up: Option<String>,
}

/// Every state change: halt, shutdown, resume, a unit action, a deploy or
/// a rollback. A write, so it needs writes on, a session and our own
/// origin; the reason is required here and again by the agent.
async fn ops_action(
    State(deck): State<Deck>,
    peer: Option<axum::Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<ActionBody>,
) -> Response {
    if let Err(refusal) = guard_write(&deck, &headers) {
        return refusal.into_response();
    }
    if body.reason.trim().is_empty() {
        return Refusal::bad_request("请填写原因").into_response();
    }
    let op = match body.action.as_str() {
        "halt" => ops::Op::Halt,
        "shutdown" => ops::Op::Shutdown,
        "resume" => ops::Op::Resume,
        "rollback" => ops::Op::Rollback,
        "unit" => match (body.unit, body.verb) {
            (Some(unit), Some(verb)) => ops::Op::Unit { unit, verb },
            _ => return Refusal::bad_request("缺少单元或动作").into_response(),
        },
        "deploy" => match body.id {
            Some(id) => ops::Op::Deploy { id },
            None => return Refusal::bad_request("缺少发布编号").into_response(),
        },
        "config_put" => match (body.name, body.content, body.base_sha) {
            (Some(name), Some(content), Some(base_sha)) => ops::Op::ConfigPut {
                name,
                content,
                base_sha,
            },
            _ => return Refusal::bad_request("缺少文件名、内容或读取时的版本").into_response(),
        },
        "config_rollback" => match (body.name, body.backup, body.base_sha) {
            (Some(name), Some(backup), Some(base_sha)) => ops::Op::ConfigRollback {
                name,
                backup,
                base_sha,
            },
            _ => return Refusal::bad_request("缺少文件名、备份或当前版本").into_response(),
        },
        "strategy_create" => match (body.name, body.config) {
            (Some(name), Some(config)) => ops::Op::StrategyCreate { name, config },
            _ => return Refusal::bad_request("缺少名称或配置文件").into_response(),
        },
        "strategy_backtest" => match (body.id, body.run, body.passed) {
            (Some(id), Some(run), Some(passed)) => ops::Op::StrategyBacktest { id, run, passed },
            _ => return Refusal::bad_request("缺少实例、回测 run 或是否通过").into_response(),
        },
        "strategy_advance" => match body.id {
            Some(id) => ops::Op::StrategyAdvance { id },
            None => return Refusal::bad_request("缺少实例").into_response(),
        },
        "alert_test" => ops::Op::AlertTest,
        "alert_silence" => match (body.key, body.minutes) {
            (Some(key), Some(minutes)) => ops::Op::AlertSilence { key, minutes },
            _ => return Refusal::bad_request("缺少告警或时长").into_response(),
        },
        other => return Refusal::bad_request(format!("未知操作 {other}")).into_response(),
    };
    let actor = actor_of(peer.as_ref(), &headers);
    ask_agent(&deck, op, Some(body.reason), body.step_up, actor).await
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
        .route("/runs/markout", get(markout_runs))
        .route("/ticks", get(list_ticks))
        .route("/runs/{id}", get(run_detail))
        .route("/sweeps", get(list_sweeps))
        .route("/sweeps/{id}", get(sweep_detail))
        .route("/attribution", get(attribution_report))
        .route("/journals", get(list_journals))
        .route("/journals/{id}/belief", get(journal_belief))
        .route("/journals/{id}/reconcile", post(reconcile))
        .route("/live/latest", get(reconcile_latest))
        .route("/journals/{id}/records", get(journal_records))
        .route("/runtime/settings", get(runtime_settings))
        .route("/ops/attribution", get(ops_attribution))
        .route("/ops/accounts", get(ops_accounts))
        .route("/ops/resources", get(ops_resources))
        .route("/ops/blackbox", get(ops_blackbox))
        .route("/ops/blackbox/at", get(ops_blackbox_at))
        .route("/ops/journal", get(ops_journal))
        .route("/ops/configs", get(ops_configs))
        .route("/ops/config", get(ops_config))
        .route("/ops/strategies", get(ops_strategies))
        .route("/ops/host", get(ops_host))
        .route("/ops/units", get(ops_units))
        .route("/ops/status", get(ops_status))
        .route("/ops/orders", get(ops_orders))
        .route("/ops/alerts", get(ops_alerts))
        .route("/ops/logs", get(ops_logs))
        .route("/ops/log", get(ops_log))
        .route("/ops/audit", get(ops_audit))
        .route("/ops/releases", get(ops_releases))
        .route("/ops/action", post(ops_action))
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
