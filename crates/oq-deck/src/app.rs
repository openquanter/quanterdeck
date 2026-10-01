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
use oq_deck_core::lang::Lang;
use oq_deck_core::{attribution, auth, capabilities, live, markout, ops, runs, sweeps};
use serde::{Deserialize, Serialize};
use tower_http::services::{ServeDir, ServeFile};

use crate::guard::{Hosts, origin_permitted};
use crate::session::{self, Sessions};
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
    /// The browsers the operator has enrolled. `None` when the deck has
    /// no state directory to keep them in, which is a capability the
    /// console reports rather than one it silently lacks.
    pub devices: Option<Arc<std::sync::Mutex<oq_deck_core::devices::Devices>>>,
    /// Challenges handed out to someone proving a key, and the claim
    /// codes that come back from one. In memory, and deliberately: a
    /// challenge that outlived a restart would be a credential that
    /// outlived the process that minted it.
    pub enrolments: Arc<crate::enrol::Enrolments>,
}

impl Deck {
    /// Whether a token belongs to a device the operator enrolled.
    #[must_use]
    pub fn enrolled(&self, token: &str) -> bool {
        self.devices.as_ref().is_some_and(|d| {
            d.lock()
                .map(|d| {
                    d.find(token, now_ms(), self.settings.device_lifetime)
                        .is_some()
                })
                .unwrap_or(false)
        })
    }
}

/// Wall-clock milliseconds since the epoch, for comparing with the
/// times the deck writes down.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

tokio::task_local! {
    /// The language the request asked for; set once, outermost.
    static LANG: Lang;
}

/// The language of the request being answered. Chinese outside one.
fn lang() -> Lang {
    LANG.try_with(|l| *l).unwrap_or_default()
}

/// One of two renderings, in the language of the request being answered.
fn t<T>(zh: T, en: T) -> T {
    lang().pick(zh, en)
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
        t(
            format!(
                "拒绝服务该 Host。本 deck 只应答 {}。\
                 一个指向 127.0.0.1 的外部域名正是 DNS rebinding 的形状；\
                 若你在用反向代理，请把它的域名加进 OQ_DECK_EXTRA_HOSTS。",
                deck.hosts.names().join(", ")
            ),
            format!(
                "This Host is refused. This deck answers only to {}. An outside name that \
                 points at 127.0.0.1 is exactly the shape of DNS rebinding; if you are behind \
                 a reverse proxy, add its name to OQ_DECK_EXTRA_HOSTS.",
                deck.hosts.names().join(", ")
            ),
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
            t(
                "该写入请求来自其他站点（Sec-Fetch-Site），跨站点的写入会被拒绝。",
                "This write came from another site (Sec-Fetch-Site); cross-site writes are refused.",
            ),
        ));
    }
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    if origin_permitted(origin, host, &deck.hosts, deck.settings.behind_tls) {
        return Ok(());
    }
    Err(Refusal::new(
        StatusCode::FORBIDDEN,
        t(
            "该写入请求的 Origin 不属于本 deck。跨站点的写入会被拒绝。",
            "This write's Origin is not this deck; cross-site writes are refused.",
        ),
    ))
}

/// The `Origin` check, for the routes a command line calls.
///
/// A browser sends `Origin` on every POST it makes, so its absence means
/// the caller is not one — and these two routes hand a caller nothing it
/// can spend: no cookie is read, and what comes back is a challenge it
/// would still have to sign with a key the operator holds. A caller that
/// *is* a page gets the same check as everywhere else, so a page cannot
/// quietly drive an enrolment either.
fn check_origin_if_a_browser(deck: &Deck, headers: &HeaderMap) -> Result<(), Refusal> {
    if headers.get(header::ORIGIN).is_none() {
        return Ok(());
    }
    check_origin(deck, headers)
}

fn check_session(deck: &Deck, headers: &HeaderMap) -> Result<(), Refusal> {
    let cookies = headers.get(header::COOKIE).and_then(|v| v.to_str().ok());
    let session = session::token_from_cookies(cookies);
    if let Some(token) = session.as_deref()
        && deck.sessions.touch(token).is_ok()
    {
        return Ok(());
    }
    // A device the operator enrolled is a credential in its own right.
    // It is what makes a deck restart — which drops every session,
    // because sessions live in memory — invisible to a browser they
    // chose to trust, and it is revocable where a session is not.
    if let Some(token) = session::device_from_cookies(cookies)
        && deck.enrolled(&token)
    {
        return Ok(());
    }
    Err(Refusal::new(
        StatusCode::UNAUTHORIZED,
        if session.is_some() {
            t(
                "会话已失效，请重新登录。",
                "The session has expired; sign in again.",
            )
        } else {
            t("请先登录。", "Sign in first.")
        },
    ))
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

/// The deck's own configuration is broken, not the request. `what` is
/// a finished sentence in the request's language.
fn misconfigured(what: &str) -> Refusal {
    Refusal::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        t(
            format!("{what}这是配置问题，不是密码错误；本次尝试不计入失败次数。"),
            format!(
                "{what} This is a configuration problem, not a wrong password; \
                 the attempt is not counted as a failure."
            ),
        ),
    )
}

fn locked_out(deck: &Deck, source: &str) -> Refusal {
    let minutes = deck
        .sessions
        .lockout_remaining(source)
        .map_or(0, |left| left.as_secs().div_ceil(60));
    Refusal::new(
        StatusCode::TOO_MANY_REQUESTS,
        t(
            format!("失败次数过多，请在 {minutes} 分钟后再试。"),
            format!("Too many failed attempts; try again in {minutes} min."),
        ),
    )
}

fn check_writes(deck: &Deck) -> Result<(), Refusal> {
    if deck.settings.allow_writes {
        return Ok(());
    }
    Err(Refusal::new(
        StatusCode::FORBIDDEN,
        t(
            "本 deck 处于只读模式；要修改任何东西，请先在设置中开启写入。",
            "This deck is read-only; to change anything, turn on write mode first.",
        ),
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

/// A write that changes what this console may do, rather than what the
/// host does.
///
/// Not behind `OQ_DECK_ALLOW_WRITES`: that flag says the console may act
/// on the host, and everything here makes it act *less*. A deck with
/// writes off that refused to let an enrolled browser be taken away
/// would be refusing the one action that is always safe, and the one an
/// operator reaches for when something is wrong.
fn guard_admin(deck: &Deck, headers: &HeaderMap) -> Result<(), Refusal> {
    check_host(deck, headers)?;
    check_origin(deck, headers)?;
    check_session(deck, headers)
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
    /// Whether the login form should offer to remember this browser.
    /// Told rather than guessed: a deck with no state directory cannot,
    /// and a box that silently does nothing is worse than no box.
    devices: bool,
    /// Whether this deck can be enrolled onto a new machine by proving
    /// an SSH key, which is the way in when there is no password on that
    /// machine yet. Told for the same reason as `devices`.
    enrol: bool,
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
        devices: deck.devices.is_some(),
        // Whether a key can enrol this browser. Both halves have to be
        // there: somewhere to keep the device, and keys to trust.
        enrol: deck.devices.is_some() && deck.settings.trusted_keys.is_some(),
    })
    .into_response()
}

/// The browsers this operator has enrolled.
///
/// Not behind `OQ_DECK_ALLOW_WRITES`: that flag is about acting on the
/// host, and this is the console's own administration. Reading the list
/// and taking an entry away both *reduce* what can reach the host, and a
/// deck that refused to let them be revoked while read-only would be
/// refusing the one action that is always safe.
async fn devices_list(State(deck): State<Deck>, headers: HeaderMap) -> Response {
    if let Err(refusal) = guard_read(&deck, &headers) {
        return refusal.into_response();
    }
    let Some(devices) = deck.devices.as_ref() else {
        return axum::Json(serde_json::json!({ "available": false, "devices": [] }))
            .into_response();
    };
    let list = devices
        .lock()
        .map(|d| {
            d.list()
                .iter()
                .map(|device| {
                    serde_json::json!({
                        "id": device.id,
                        "label": device.label,
                        "created_ms": device.created_ms,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    axum::Json(serde_json::json!({ "available": true, "devices": list })).into_response()
}

#[derive(Deserialize)]
struct RevokeBody {
    id: String,
}

/// Take one enrolled browser away.
async fn devices_revoke(
    State(deck): State<Deck>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<RevokeBody>,
) -> Response {
    if let Err(refusal) = guard_admin(&deck, &headers) {
        return refusal.into_response();
    }
    let Some(devices) = deck.devices.as_ref() else {
        return Refusal::not_found(t(
            "这台 deck 没有可写的状态目录，没有登记过设备。",
            "This deck has no writable state directory and has enrolled no devices.",
        ))
        .into_response();
    };
    match devices.lock().map(|mut d| d.revoke(&body.id)) {
        Ok(Ok(true)) => axum::Json(serde_json::json!({ "revoked": true })).into_response(),
        Ok(Ok(false)) => Refusal::not_found(t("没有这个设备。", "No such device.")).into_response(),
        Ok(Err(e)) => Refusal::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            t(format!("无法撤销：{e}"), format!("Could not revoke: {e}")),
        )
        .into_response(),
        Err(_) => Refusal::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            t("设备状态已损坏。", "the device state is poisoned"),
        )
        .into_response(),
    }
}

/// Enrol this browser, if the operator asked and the deck can keep it.
///
/// A deck with no state directory cannot, and the login form does not
/// offer the box — the console reports what it can do rather than
/// accepting a request it will quietly drop.
fn enrol(deck: &Deck, label: &str, now_ms: i64) -> Option<String> {
    let devices = deck.devices.as_ref()?;
    let token = auth::new_token().ok()?;
    let label = if label.trim().is_empty() {
        "a browser"
    } else {
        label.trim()
    };
    devices
        .lock()
        .ok()?
        .issue(label, &token, now_ms)
        .ok()
        .map(|_| token)
}

// -- enrolling by proving a key -------------------------------------------

#[derive(Serialize)]
struct Challenge {
    /// What to sign. A nonce, good once and briefly.
    challenge: String,
    /// The `-n` to sign it under, so the script and this end agree
    /// without the operator having to be told a magic string.
    namespace: &'static str,
}

/// Hand out something to sign.
///
/// Unauthenticated on purpose — this is how a machine with no session
/// gets one — and therefore bounded: a fixed number outstanding, two
/// minutes each, and a challenge is spent by being offered rather than
/// by being answered correctly.
async fn enrol_challenge(State(deck): State<Deck>, headers: HeaderMap) -> Response {
    if let Err(refusal) =
        check_host(&deck, &headers).and_then(|()| check_origin_if_a_browser(&deck, &headers))
    {
        return refusal.into_response();
    }
    if deck.settings.trusted_keys.is_none() {
        return Refusal::not_found(t(
            "本 deck 未配置 OQ_DECK_TRUSTED_KEYS，不能用 SSH 密钥登记。",
            "This deck has no OQ_DECK_TRUSTED_KEYS, so a key cannot enrol a browser.",
        ))
        .into_response();
    }
    match deck.enrolments.challenge(std::time::Instant::now()) {
        Ok(challenge) => axum::Json(Challenge {
            challenge,
            namespace: crate::enrol::NAMESPACE,
        })
        .into_response(),
        Err(e) => Refusal::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            t(
                format!("无法生成挑战：{e}"),
                format!("Could not make a challenge: {e}"),
            ),
        )
        .into_response(),
    }
}

#[derive(Deserialize)]
struct Enrol {
    challenge: String,
    /// The armored signature `ssh-keygen -Y sign` prints.
    signature: String,
    /// The principal in the `allowed_signers` file. Which key it is, not
    /// what it is allowed to do.
    identity: String,
    /// What the operator calls the machine, for the list they revoke
    /// from.
    #[serde(default)]
    label: String,
}

#[derive(Serialize)]
struct Claimed {
    /// What the browser redeems. Good once, for two minutes.
    claim: String,
    label: String,
}

/// Check the signature, and mint the claim code that replaces it.
///
/// The challenge is spent whatever the verdict, so a signature cannot be
/// tried twice against the same question.
async fn enrol_with_key(
    State(deck): State<Deck>,
    peer: Option<axum::Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    body: axum::Json<Enrol>,
) -> Response {
    if let Err(refusal) =
        check_host(&deck, &headers).and_then(|()| check_origin_if_a_browser(&deck, &headers))
    {
        return refusal.into_response();
    }
    // This route is unauthenticated and runs a subprocess per attempt, so
    // a failed signature counts the same as a failed password and closes
    // the door the same way. Without it, anyone who could reach the port
    // could make the deck fork `ssh-keygen` as fast as they could ask.
    let source = source_of(peer.as_ref());
    if deck.sessions.lockout_remaining(&source).is_some() {
        return locked_out(&deck, &source).into_response();
    }
    let Some(signers) = deck.settings.trusted_keys.clone() else {
        return Refusal::not_found(t(
            "本 deck 未配置 OQ_DECK_TRUSTED_KEYS，不能用 SSH 密钥登记。",
            "This deck has no OQ_DECK_TRUSTED_KEYS, so a key cannot enrol a browser.",
        ))
        .into_response();
    };
    // What the operator called the machine, or the principal they
    // proved — a name is what makes the revoke list usable, and the
    // identity is the fallback that is at least about this machine.
    let label = if body.label.trim().is_empty() {
        body.identity.trim().to_owned()
    } else {
        body.label.trim().to_owned()
    };
    let spent = deck
        .enrolments
        .spend(&body.challenge, &label, std::time::Instant::now());
    let Ok(claim) = spent else {
        return Refusal::new(
            StatusCode::UNAUTHORIZED,
            t(
                "挑战已过期或不存在；重新运行登记脚本即可。",
                "That challenge is not one this deck issued, or it has expired; run the enrolment script again.",
            ),
        )
        .into_response();
    };
    // Nothing is remembered from a rejected attempt but the refusal.
    // What the operator gets back is ssh-keygen's sentence, which is
    // the difference between "that key is not in the file" and "that
    // signature is not over that challenge".
    if let Err(why) =
        crate::enrol::verify(&signers, &body.identity, &body.challenge, &body.signature)
    {
        deck.sessions.record_failure(&source);
        return Refusal::new(StatusCode::UNAUTHORIZED, why).into_response();
    }
    axum::Json(Claimed { claim, label }).into_response()
}

#[derive(Deserialize)]
struct Claim {
    claim: String,
}

/// Take a claim code for a device credential.
///
/// The browser's half of the exchange, and the reason the two halves
/// are separate: a code that is only ever seen here is one that never
/// has to be typed into a terminal, and one that is spent by the first
/// browser that presents it.
async fn enrol_claim(
    State(deck): State<Deck>,
    headers: HeaderMap,
    body: axum::Json<Claim>,
) -> Response {
    // The code is the credential, so this is the one write that does not
    // want a session — asking for one is asking for the thing the caller
    // came here to get. The Origin check stays, because this half is the
    // browser's and a browser always sends one.
    if let Err(refusal) = check_host(&deck, &headers).and_then(|()| check_origin(&deck, &headers)) {
        return refusal.into_response();
    }
    if deck.devices.is_none() {
        return Refusal::not_found(t(
            "本 deck 没有状态目录，记不住浏览器。",
            "This deck has no state directory, so it cannot remember a browser.",
        ))
        .into_response();
    }
    let Some(label) = deck
        .enrolments
        .claim(&body.claim, std::time::Instant::now())
    else {
        return Refusal::new(
            StatusCode::UNAUTHORIZED,
            t(
                "这个登记码已经用过或已过期。",
                "That enrolment code has been used already, or it has expired.",
            ),
        )
        .into_response();
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
    let token = match enrol(&deck, &label, now_ms) {
        Some(token) => token,
        None => {
            return Refusal::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                t("无法登记这台浏览器。", "Could not enrol this browser."),
            )
            .into_response();
        }
    };
    let cookie = session::device_cookie(
        &token,
        deck.settings.behind_tls,
        deck.settings.device_lifetime,
    );
    let mut response =
        axum::Json(serde_json::json!({ "enrolled": true, "label": label })).into_response();
    if let Ok(value) = axum::http::HeaderValue::from_str(&cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

#[derive(Deserialize)]
struct Login {
    password: String,
    #[serde(default)]
    totp: String,
    /// Enrol this browser, so it is not asked for a password again.
    #[serde(default)]
    remember: bool,
    /// What the operator calls it. The list they revoke from is a list
    /// of names or it is a list of hex ids.
    #[serde(default)]
    device_label: String,
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
            t(
                "本 deck 尚未完成初始设置。",
                "This deck has not been set up yet.",
            ),
        )
        .into_response();
    };

    let rejected = Refusal::new(
        StatusCode::UNAUTHORIZED,
        t("密码或验证码不正确。", "The password or code is incorrect."),
    );

    match auth::verify_password(&body.password, stored) {
        Ok(()) => {}
        Err(auth::AuthError::Rejected) => {
            deck.sessions.record_failure(&source);
            return rejected.into_response();
        }
        // The stored hash, not the attempt. Reported as itself and not
        // counted: read as a wrong password it sent the operator after a
        // password that was never the problem, and locked them out.
        Err(_) => {
            return misconfigured(t(
                "OQ_DECK_PASSWORD_HASH 无法解析。",
                "OQ_DECK_PASSWORD_HASH cannot be parsed.",
            ))
            .into_response();
        }
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
                return misconfigured(t(
                    "OQ_DECK_TOTP_SECRET 无法解析。",
                    "OQ_DECK_TOTP_SECRET cannot be parsed.",
                ))
                .into_response();
            }
        }
    }

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));

    match deck.sessions.issue(&source) {
        Ok(token) => {
            let mut cookies = vec![session::set_cookie(
                &token,
                deck.settings.behind_tls,
                deck.settings.session_absolute,
            )];
            // After the session, so a failure here still leaves a
            // signed-in operator rather than a refusal they cannot
            // explain.
            if body.remember
                && let Some(device) = enrol(&deck, &body.device_label, now_ms)
            {
                cookies.push(session::device_cookie(
                    &device,
                    deck.settings.behind_tls,
                    deck.settings.device_lifetime,
                ));
            }
            let mut response =
                axum::Json(serde_json::json!({ "authenticated": true })).into_response();
            for value in cookies {
                if let Ok(v) = axum::http::HeaderValue::from_str(&value) {
                    response.headers_mut().append(header::SET_COOKIE, v);
                }
            }
            response
        }
        Err(error) => Refusal::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            t(
                format!("无法签发会话：{error}"),
                format!("Could not issue a session: {error}"),
            ),
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
    // Signing out ends the session, not the device: the device is a
    // thing the operator made on purpose, and taking it away is a
    // separate act with its own page. Anything else would make signing
    // out on a shared machine also un-enrol the machine.
    let clearing = session::clear_cookies();
    let mut response = axum::Json(serde_json::json!({ "authenticated": false })).into_response();
    for value in clearing {
        if let Ok(v) = axum::http::HeaderValue::from_str(&value) {
            response.headers_mut().append(header::SET_COOKIE, v);
        }
    }
    response
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
            return Refusal::new(
                StatusCode::CONFLICT,
                t("初始设置已经完成。", "Setup is already done."),
            )
            .into_response();
        };
        if !auth::secrets_match(expected, &body.token) {
            deck.sessions.record_failure(&source);
            return Refusal::new(
                StatusCode::UNAUTHORIZED,
                t("一次性令牌不正确。", "The one-time token is incorrect."),
            )
            .into_response();
        }
        // A password that is refused does not spend the token.
        if let Some(complaint) = auth::password_complaint(&body.password, lang()) {
            return Refusal::bad_request(complaint).into_response();
        }
        slot.take()
    };

    match (auth::hash_password(&body.password), auth::new_totp_secret()) {
        (Ok(password_hash), Ok(totp_secret)) => axum::Json(SetupDone {
            password_hash,
            totp_secret: totp_secret.clone(),
            next_steps: vec![
                t(
                    "把 OQ_DECK_PASSWORD_HASH 设为上面的 hash，重启 deck。",
                    "Set OQ_DECK_PASSWORD_HASH to the hash above and restart the deck.",
                )
                .to_owned(),
                t(
                    "把 TOTP secret 录入验证器应用，并设为 OQ_DECK_TOTP_SECRET；deck 对外可达时（非回环地址、反向代理或 OQ_DECK_EXTRA_HOSTS）它是必需的。",
                    "Add the TOTP secret to an authenticator app and set it as OQ_DECK_TOTP_SECRET; it is required whenever the deck is reachable from outside (a non-loopback address, a reverse proxy, or OQ_DECK_EXTRA_HOSTS).",
                )
                .to_owned(),
                t(
                    "两者都不要提交进 git，也不要写进任何日志。",
                    "Commit neither to git, and write neither to any log.",
                )
                .to_owned(),
            ],
        })
        .into_response(),
        (Err(error), _) | (_, Err(error)) => {
            // Nothing was issued, so the token is given back.
            *deck.setup_token.lock().unwrap_or_else(|e| e.into_inner()) = token;
            Refusal::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                t(
                    format!("初始设置失败：{error}"),
                    format!("Setup failed: {error}"),
                ),
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
        lang(),
    ))
    .into_response()
}

fn dir_of(configured: Option<&PathBuf>, variable: &str) -> Result<PathBuf, Refusal> {
    configured.cloned().ok_or_else(|| {
        Refusal::new(
            StatusCode::PRECONDITION_REQUIRED,
            t(
                format!("尚未配置目录；请设置 {variable}"),
                format!("No directory is configured; set {variable}"),
            ),
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
        t(
            format!("目录无法读取，所以无法判断其中有什么：{why}"),
            format!("The directory cannot be read, so what is in it is unknown: {why}"),
        ),
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
        return Refusal::bad_request(t(
            "需要 price_scale 和 qty_scale：归因金额按合约的价格与数量精度计算，\
             没有默认值可以替你选。",
            "price_scale and qty_scale are required: attribution is computed at the \
             instrument's price and quantity precision, and no default can choose them for you.",
        ))
        .into_response();
    };
    let inputs = attribution::Inputs {
        price_scale,
        qty_scale,
        funding: AttributionQuery::pair(query.venue_funding, query.model_funding),
        fees: AttributionQuery::pair(query.venue_fees, query.model_fees),
        lang: lang(),
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
        return Refusal::not_found(t(
            "尚未配置交易所最新记录（OQ_DECK_VENUE_RECORD）",
            "No latest venue record is configured (OQ_DECK_VENUE_RECORD)",
        ))
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
        return Refusal::not_found(t(
            "日志目录里还没有交易日志",
            "The journals directory has no journal yet",
        ))
        .into_response();
    };
    let text = match std::fs::read_to_string(&record_path) {
        Ok(t) => t,
        Err(e) => {
            return Refusal::not_found(t(
                format!("读不到交易所记录 {}：{e}", record_path.display()),
                format!(
                    "Cannot read the venue record {}: {e}",
                    record_path.display()
                ),
            ))
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
        return Refusal::not_found(t(
            "尚未配置主机代理（OQ_DECK_AGENT_SOCKET）",
            "No host agent is configured (OQ_DECK_AGENT_SOCKET)",
        ))
        .into_response();
    };
    let nonce = match auth::new_token() {
        Ok(n) => n,
        Err(e) => {
            return misconfigured(&t(
                format!("无法生成请求编号：{e}。"),
                format!("Could not make a request id: {e}."),
            ))
            .into_response();
        }
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
            // The agent writes a refusal in both languages — it cannot
            // know which one this reader asked for, and for a request it
            // could not even read there is nobody to ask.
            resp.why(lang()).map(str::to_string).unwrap_or_else(|| {
                t("主机代理拒绝了请求", "The host agent refused the request").into()
            }),
        )
        .into_response(),
        Err(e) => Refusal::new(
            StatusCode::BAD_GATEWAY,
            t(
                format!("主机代理无应答：{e}"),
                format!("The host agent did not answer: {e}"),
            ),
        )
        .into_response(),
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
        serde_json::from_str::<ops::AgentResponse>(&answer).map_err(|e| {
            t(
                format!("无法读取应答：{e}"),
                format!("cannot read the answer: {e}"),
            )
        })
    };
    tokio::time::timeout(std::time::Duration::from_secs(60), work)
        .await
        .map_err(|_| t("超时", "timed out").to_string())?
}

/// Who is asking, for the agent's audit trail: the deck's one operator,
/// and where from.
fn actor_of(
    peer: Option<&axum::Extension<ConnectInfo<SocketAddr>>>,
    headers: &HeaderMap,
) -> String {
    // Behind the proxy the socket peer is the proxy, so the address it
    // forwards is the only one there is — and a header is something the
    // caller wrote, not something this deck observed. It goes in marked
    // as a claim: the trail is read months later as evidence of who did
    // what, and an unmarked claim reads as a fact.
    let forwarded = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|v| !v.is_empty());
    match forwarded {
        Some(claimed) => format!("deck:operator@forwarded({claimed})"),
        None => format!("deck:operator@{}", source_of(peer)),
    }
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
        "session": {
            "idle_minutes": s.session_idle.as_secs() / 60,
            "absolute_hours": s.session_absolute.as_secs() / 3600,
        },
        // Reported rather than assumed: a deck with no state directory
        // cannot remember a browser, and the form must not offer it.
        "devices": deck.devices.is_some(),
        "device_days": s.device_lifetime.as_secs() / 86_400,
        "trusted_keys": path(&s.trusted_keys),
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
        return Refusal::bad_request(t("请填写原因", "A reason is required")).into_response();
    }
    let op = match body.action.as_str() {
        "halt" => ops::Op::Halt,
        "shutdown" => ops::Op::Shutdown,
        "resume" => ops::Op::Resume,
        "rollback" => ops::Op::Rollback,
        "unit" => match (body.unit, body.verb) {
            (Some(unit), Some(verb)) => ops::Op::Unit { unit, verb },
            _ => {
                return Refusal::bad_request(t("缺少单元或动作", "A unit and a verb are required"))
                    .into_response();
            }
        },
        "deploy" => match body.id {
            Some(id) => ops::Op::Deploy { id },
            None => {
                return Refusal::bad_request(t("缺少发布编号", "A release id is required"))
                    .into_response();
            }
        },
        "config_put" => match (body.name, body.content, body.base_sha) {
            (Some(name), Some(content), Some(base_sha)) => ops::Op::ConfigPut {
                name,
                content,
                base_sha,
            },
            _ => {
                return Refusal::bad_request(t(
                    "缺少文件名、内容或读取时的版本",
                    "A file name, content and the version it was read at are required",
                ))
                .into_response();
            }
        },
        "config_rollback" => match (body.name, body.backup, body.base_sha) {
            (Some(name), Some(backup), Some(base_sha)) => ops::Op::ConfigRollback {
                name,
                backup,
                base_sha,
            },
            _ => {
                return Refusal::bad_request(t(
                    "缺少文件名、备份或当前版本",
                    "A file name, a backup and the current version are required",
                ))
                .into_response();
            }
        },
        "strategy_create" => match (body.name, body.config) {
            (Some(name), Some(config)) => ops::Op::StrategyCreate { name, config },
            _ => {
                return Refusal::bad_request(t(
                    "缺少名称或配置文件",
                    "A name and a config file are required",
                ))
                .into_response();
            }
        },
        "strategy_backtest" => match (body.id, body.run, body.passed) {
            (Some(id), Some(run), Some(passed)) => ops::Op::StrategyBacktest { id, run, passed },
            _ => {
                return Refusal::bad_request(t(
                    "缺少实例、回测 run 或是否通过",
                    "An instance, a backtest run and a verdict are required",
                ))
                .into_response();
            }
        },
        "strategy_advance" => match body.id {
            Some(id) => ops::Op::StrategyAdvance { id },
            None => {
                return Refusal::bad_request(t("缺少实例", "An instance is required"))
                    .into_response();
            }
        },
        "alert_test" => ops::Op::AlertTest,
        "alert_silence" => match (body.key, body.minutes) {
            (Some(key), Some(minutes)) => ops::Op::AlertSilence { key, minutes },
            _ => {
                return Refusal::bad_request(t(
                    "缺少告警或时长",
                    "An alert and a duration are required",
                ))
                .into_response();
            }
        },
        other => {
            return Refusal::bad_request(t(
                format!("未知操作 {other}"),
                format!("Unknown action {other}"),
            ))
            .into_response();
        }
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
    devices: Option<Arc<std::sync::Mutex<oq_deck_core::devices::Devices>>>,
) -> Router {
    let hosts = Hosts::for_bind(settings.host, settings.port, &settings.extra_hosts);
    // Read before the settings are moved: the session store outlives the
    // reference to them.
    let (idle, absolute) = (settings.session_idle, settings.session_absolute);
    let deck = Deck {
        settings: Arc::new(settings),
        sessions: Arc::new(Sessions::with_lifetimes(idle, absolute)),
        hosts: Arc::new(hosts),
        setup_token: Arc::new(std::sync::Mutex::new(setup_token)),
        devices,
        enrolments: Arc::new(crate::enrol::Enrolments::default()),
    };

    let api = Router::new()
        .route("/health", get(health))
        .route("/session", get(whoami))
        .route("/session/login", post(login))
        .route("/session/logout", post(logout))
        .route("/session/challenge", post(enrol_challenge))
        .route("/session/enrol", post(enrol_with_key))
        .route("/session/claim", post(enrol_claim))
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
        .route("/devices", get(devices_list))
        .route("/devices/revoke", post(devices_revoke))
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
        // Outside even that: the refusal of a Host is a sentence too.
        .layer(axum::middleware::from_fn(language))
}

/// Answers in the language the request asks for (`Accept-Language`),
/// for everything the request reaches.
async fn language(request: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let asked = request
        .headers()
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .map_or_else(Lang::default, Lang::from_accept_language);
    LANG.scope(asked, next.run(request)).await
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
    Refusal::new(
        StatusCode::NOT_FOUND,
        t("没有这个 API 路径。", "No such API path."),
    )
    .into_response()
}
