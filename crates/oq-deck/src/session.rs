//! Sessions, and the checks every request passes before it is one.
//!
//! # The threats this is written against
//!
//! Not a hostile internet — the deck listens on the loopback interface
//! by default and expects to stay there. The realistic ones are closer:
//!
//! | Threat | What stops it |
//! |---|---|
//! | Another local process or user opening the port | A session cookie, always required |
//! | A page in the operator's browser reaching `127.0.0.1` by DNS rebinding | The `Host` allowlist |
//! | That page reading the answer, or riding the session | `SameSite=Strict`, and an `Origin` check on writes |
//! | A token or cookie read out of a log | Neither is ever logged |
//! | Guessing the password | Argon2id, plus a lockout that counts attempts |
//! | A stolen cookie living forever | Idle and absolute expiry, both enforced server-side |
//!
//! Sessions live in memory. A restart logs the operator out, which for a
//! console watching a trading process is the right trade: the
//! alternative is a token on disk that outlives the reason it was
//! issued.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use oq_deck_core::auth;

/// How long a session survives without use.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(60 * 60);
/// How long a session survives at all, however active.
pub const ABSOLUTE_TIMEOUT: Duration = Duration::from_secs(12 * 60 * 60);
/// Failed attempts from one source before the door closes to it.
pub const MAX_FAILURES: u32 = 5;
/// Failed attempts from every source together before the door closes to
/// all of them: the bound on guessing that per-source counting alone
/// would not give, since sources are cheap to vary.
pub const MAX_FAILURES_OVERALL: u32 = 50;
/// How long it stays closed.
pub const LOCKOUT: Duration = Duration::from_secs(15 * 60);

pub const COOKIE_NAME: &str = "oq_deck_session";

struct Session {
    created: Instant,
    last_seen: Instant,
}

/// Everything about who is allowed in.
pub struct Sessions {
    inner: Mutex<Inner>,
}

struct Inner {
    live: HashMap<String, Session>,
    /// Failures and lockout per source address.
    ///
    /// Per source because one global counter let anything that could
    /// reach the port — another local process, anyone on the internet
    /// once a reverse proxy was in front — lock the operator out with
    /// five bad requests every fifteen minutes.
    sources: HashMap<String, Attempts>,
    /// All sources together, the backstop on total guessing.
    overall: Attempts,
}

#[derive(Default)]
struct Attempts {
    failures: u32,
    locked_until: Option<Instant>,
}

impl Attempts {
    fn remaining(&self, now: Instant) -> Option<Duration> {
        self.locked_until
            .and_then(|until| until.checked_duration_since(now))
    }

    fn fail(&mut self, threshold: u32, now: Instant) {
        self.failures += 1;
        if self.failures >= threshold {
            self.locked_until = Some(now + LOCKOUT);
            self.failures = 0;
        }
    }
}

/// Why a request is not authenticated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denied {
    /// No session, or one that has expired.
    NoSession,
}

impl Default for Sessions {
    fn default() -> Self {
        Self::new()
    }
}

impl Sessions {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                live: HashMap::new(),
                sources: HashMap::new(),
                overall: Attempts::default(),
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // A poisoned lock means a previous request panicked while
        // holding it. The session table is not left inconsistent by
        // that, and refusing every login afterwards would turn one
        // panic into an outage.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Whether the door is closed to `source`, and for how much longer:
    /// the longer of its own lockout and the overall one.
    #[must_use]
    pub fn lockout_remaining(&self, source: &str) -> Option<Duration> {
        let inner = self.lock();
        let now = Instant::now();
        let own = inner.sources.get(source).and_then(|a| a.remaining(now));
        let all = inner.overall.remaining(now);
        own.max(all)
    }

    /// Record a failed attempt from `source`, closing the door to it at
    /// its threshold and to everyone at the overall one.
    pub fn record_failure(&self, source: &str) {
        let now = Instant::now();
        let mut inner = self.lock();
        inner
            .sources
            .entry(source.to_owned())
            .or_default()
            .fail(MAX_FAILURES, now);
        inner.overall.fail(MAX_FAILURES_OVERALL, now);
    }

    /// Issue a session, clearing the failure count.
    ///
    /// # Errors
    /// The system CSPRNG was unavailable.
    pub fn issue(&self, source: &str) -> Result<String, auth::AuthError> {
        let token = auth::new_token()?;
        let now = Instant::now();
        let mut inner = self.lock();
        inner.sources.remove(source);
        inner.live.insert(
            token.clone(),
            Session {
                created: now,
                last_seen: now,
            },
        );
        Ok(token)
    }

    /// Check a token and, if it is good, extend it.
    ///
    /// Expiry is enforced here rather than by a sweeper, so a session
    /// cannot be resurrected by a request arriving between sweeps.
    pub fn touch(&self, token: &str) -> Result<(), Denied> {
        let now = Instant::now();
        let mut inner = self.lock();
        // No lockout check. A session is proof the holder already passed
        // the door; closing it on them because someone else is guessing
        // turned the lockout into a way to evict the operator — five bad
        // requests and every live session read as locked out too.

        // Look the token up in constant time with respect to its
        // contents: a HashMap probe is not, so the comparison that
        // decides the answer is done explicitly.
        let found = inner
            .live
            .keys()
            .find(|candidate| auth::secrets_match(candidate, token))
            .cloned();
        let Some(key) = found else {
            return Err(Denied::NoSession);
        };

        let Some(session) = inner.live.get_mut(&key) else {
            return Err(Denied::NoSession);
        };
        if now.duration_since(session.created) > ABSOLUTE_TIMEOUT
            || now.duration_since(session.last_seen) > IDLE_TIMEOUT
        {
            inner.live.remove(&key);
            return Err(Denied::NoSession);
        }
        session.last_seen = now;
        Ok(())
    }

    /// End one session.
    pub fn revoke(&self, token: &str) {
        let mut inner = self.lock();
        let found = inner
            .live
            .keys()
            .find(|candidate| auth::secrets_match(candidate, token))
            .cloned();
        if let Some(key) = found {
            inner.live.remove(&key);
        }
    }

    /// End every session. Used when the password changes.
    pub fn revoke_all(&self) {
        self.lock().live.clear();
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.lock().live.len()
    }
}

/// Pull the session token out of a cookie header.
#[must_use]
pub fn token_from_cookies(header: Option<&str>) -> Option<String> {
    header?.split(';').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name.trim() == COOKIE_NAME).then(|| value.trim().to_owned())
    })
}

/// The `Set-Cookie` value for a new session.
///
/// `HttpOnly` keeps it away from script, `SameSite=Strict` keeps it off
/// cross-site requests, and `Secure` is added only behind TLS because a
/// browser silently drops a `Secure` cookie on plain `http://localhost`,
/// which would leave the operator staring at a login page that does
/// nothing.
#[must_use]
pub fn set_cookie(token: &str, secure: bool) -> String {
    let mut cookie = format!(
        "{COOKIE_NAME}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
        ABSOLUTE_TIMEOUT.as_secs()
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// The `Set-Cookie` value that clears a session.
#[must_use]
pub fn clear_cookie() -> String {
    format!("{COOKIE_NAME}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0")
}

#[cfg(test)]
mod per_source {
    use super::{MAX_FAILURES, MAX_FAILURES_OVERALL, Sessions};

    /// One source's failures close the door to it, not to another.
    #[test]
    fn a_lockout_is_per_source() {
        let s = Sessions::new();
        for _ in 0..MAX_FAILURES {
            s.record_failure("203.0.113.9");
        }
        assert!(s.lockout_remaining("203.0.113.9").is_some());
        assert!(s.lockout_remaining("198.51.100.4").is_none());
    }

    /// Varying the source does not buy unlimited guesses.
    #[test]
    fn many_sources_together_close_the_door_to_all() {
        let s = Sessions::new();
        for i in 0..MAX_FAILURES_OVERALL {
            s.record_failure(&format!("10.0.0.{}", i % 250));
        }
        assert!(s.lockout_remaining("192.0.2.1").is_some());
    }

    /// A lockout never touches a session already issued.
    #[test]
    fn a_session_outlives_a_lockout() {
        let s = Sessions::new();
        let token = s.issue("127.0.0.1").expect("issued");
        for _ in 0..MAX_FAILURES {
            s.record_failure("127.0.0.1");
        }
        assert!(s.touch(&token).is_ok());
    }
}
