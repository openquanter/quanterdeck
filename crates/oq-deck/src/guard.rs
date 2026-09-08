//! Header checks that run before anything else looks at a request.
//!
//! These are cheap and they are the two that a loopback service actually
//! needs. Neither depends on a session, because both defend against a
//! request that is *about* to be given one.

use std::collections::BTreeSet;
use std::net::IpAddr;

/// Hostnames this deck will answer to.
///
/// # Why a service on `127.0.0.1` needs an allowlist
///
/// A page the operator visits can point a hostname it controls at
/// `127.0.0.1` — DNS rebinding — and then talk to this console from
/// inside their browser, with the browser's own credentials, as a
/// same-origin request to `http://evil.example`. The socket cannot tell
/// the difference; the `Host` header can. Answering only to the names
/// the operator would actually type closes it.
#[derive(Debug, Clone)]
pub struct Hosts {
    allowed: BTreeSet<String>,
}

impl Hosts {
    /// The names that reach a deck listening on `host:port`.
    #[must_use]
    pub fn for_bind(host: IpAddr, port: u16, extra: &[String]) -> Self {
        let mut allowed = BTreeSet::new();
        for name in [host.to_string(), format!("[{host}]")] {
            allowed.insert(format!("{name}:{port}"));
            allowed.insert(name);
        }
        if host.is_loopback() {
            allowed.insert(format!("localhost:{port}"));
            allowed.insert("localhost".to_owned());
        }
        for name in extra {
            allowed.insert(name.to_lowercase());
        }
        Self { allowed }
    }

    #[must_use]
    pub fn permits(&self, header: Option<&str>) -> bool {
        // A request with no Host is HTTP/1.0 or handcrafted. Neither is
        // the operator's browser, and allowing it would be a hole shaped
        // exactly like the one this closes.
        header.is_some_and(|value| self.allowed.contains(&value.to_lowercase()))
    }

    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.allowed.iter().cloned().collect()
    }
}

/// Whether a mutating request came from this console's own pages.
///
/// `SameSite=Strict` on the session cookie already keeps it off
/// cross-site requests in every browser that honours it. This is the
/// second lock: it reads `Origin`, which a browser sets on writes and a
/// page cannot forge, and rejects anything that is not one of our own
/// names.
///
/// A missing `Origin` is refused rather than allowed. Same-origin `GET`
/// omits it, but this only runs on writes, where every browser sends it.
#[must_use]
pub fn origin_permitted(origin: Option<&str>, hosts: &Hosts) -> bool {
    let Some(origin) = origin else {
        return false;
    };
    // Compare authority to authority: `http://127.0.0.1:8899` against
    // the same `127.0.0.1:8899` the Host check uses.
    let authority = origin
        .split_once("://")
        .map_or(origin, |(_, rest)| rest)
        .trim_end_matches('/');
    hosts.permits(Some(authority))
}
