//! Configuration, and the guards that run before it is accepted.
//!
//! The defaults are the safe ones. Reaching a less safe configuration
//! takes a deliberate act with a name attached.

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Settings {
    pub host: IpAddr,
    pub port: u16,
    pub runs_dir: Option<PathBuf>,
    pub journals_dir: Option<PathBuf>,
    /// Tick files (`.oqtk`) to price fills against, for markouts.
    pub ticks_dir: Option<PathBuf>,
    /// The host agent's socket; with it, the operations pages.
    pub agent_socket: Option<PathBuf>,
    /// The venue reading `oq-recon --watch --latest` keeps.
    pub venue_record: Option<PathBuf>,
    /// Argon2id hash of the operator's password.
    ///
    /// `None` means setup has not been done. It does **not** mean the
    /// console is open: in that state it serves only the setup routes,
    /// and only to someone holding the one-time token it printed to the
    /// terminal it was started from.
    pub password_hash: Option<String>,
    /// Base32 TOTP secret. Optional on loopback, required off it.
    pub totp_secret: Option<String>,
    /// Extra names the deck will answer to, for a reverse proxy.
    pub extra_hosts: Vec<String>,
    /// Set when something in front terminates TLS, so the session cookie
    /// is marked `Secure`.
    pub behind_tls: bool,
    /// Off by default; every mutating route is refused while it is off.
    pub allow_writes: bool,
    /// How long a session survives without use.
    pub session_idle: Duration,
    /// Where the deck keeps what outlives a process: the browsers the
    /// operator has enrolled. `None` means it keeps nothing, and the
    /// console says so rather than offering a switch that does nothing.
    ///
    /// The deck is otherwise read-only about the host — it holds no
    /// venue key and writes nothing the trader uses — so this is the one
    /// directory it needs, and it is one systemd makes for it.
    pub state_dir: Option<PathBuf>,
    /// How long a browser the operator enrolled is trusted for.
    pub device_lifetime: Duration,
    /// How long a session survives at all, however active.
    ///
    /// Loosening this is how an operator trades a stolen laptop for not
    /// signing in twice a day, so it is a named setting rather than a
    /// constant that only a rebuild could move.
    pub session_absolute: Duration,
    /// An OpenSSH `allowed_signers` file: the keys whose signature will
    /// enrol a browser without a password.
    ///
    /// The same shape the host agent trusts a release with, and for the
    /// same reason — the private key never leaves the operator's
    /// machine, and what this file lists is what is trusted. `None`
    /// turns the route off, which the console reports rather than
    /// accepting a request it cannot check.
    pub trusted_keys: Option<PathBuf>,
    /// The GitHub repository whose newest release the deck compares
    /// what runs against, as `owner/name`.
    pub upstream_repo: String,
    /// This console's own GitHub repository, as `owner/name`: its newest
    /// release is compared with the version this deck was built as. Same
    /// schedule, switch and proxy as the framework check.
    pub self_repo: String,
    /// How often to ask, in hours. Zero turns the check off, and with it
    /// every request the deck would make to GitHub.
    pub upstream_every_hours: u64,
    /// An HTTP proxy for those requests, and only those. The ambient
    /// `HTTPS_PROXY` is deliberately not read: the one outbound path
    /// this deck has should be the one its settings name.
    pub upstream_proxy: Option<String>,
    /// Where scheduled reports are kept: `OQ_DECK_REPORTS_DIR`, else
    /// `reports` under the state directory. `None` turns reports off,
    /// and the console says so.
    pub reports_dir: Option<PathBuf>,
    /// The length of a report's period, in hours, and how often one is
    /// written. Zero turns reports off.
    pub report_every_hours: u64,
}

/// The longest gap between two upstream checks: a week. Longer is a
/// check that is effectively off while claiming to be on.
pub const UPSTREAM_MAX_HOURS: u64 = 168;

/// The longest report period: a week, which is also the longest window
/// the host agent's black box answers for in one request.
pub const REPORT_MAX_HOURS: u64 = 168;

impl Default for Settings {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 8899,
            runs_dir: None,
            journals_dir: None,
            ticks_dir: None,
            agent_socket: None,
            venue_record: None,
            password_hash: None,
            totp_secret: None,
            extra_hosts: Vec::new(),
            behind_tls: false,
            allow_writes: false,
            state_dir: None,
            device_lifetime: Duration::from_secs(90 * 24 * 3600),
            session_idle: Duration::from_secs(60 * 60),
            session_absolute: Duration::from_secs(12 * 60 * 60),
            trusted_keys: None,
            upstream_repo: "openquanter/openquanter".to_owned(),
            self_repo: "openquanter/quanterdeck".to_owned(),
            upstream_every_hours: 6,
            upstream_proxy: None,
            reports_dir: None,
            report_every_hours: 24,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(pub String);

impl core::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl core::error::Error for ConfigError {}

impl Settings {
    #[must_use]
    pub fn is_loopback(&self) -> bool {
        self.host.is_loopback()
    }

    /// Whether anyone but the local operator can reach the deck.
    ///
    /// Off the loopback interface, obviously. But also behind a TLS
    /// proxy or with extra hosts allowed: those are how the security
    /// notes say to publish a deck bound to 127.0.0.1, and judged by the
    /// bind address alone that deck was "loopback" — its second factor
    /// optional while the internet reached it through the proxy.
    #[must_use]
    pub fn is_exposed(&self) -> bool {
        !self.is_loopback() || self.behind_tls || !self.extra_hosts.is_empty()
    }

    /// Whether the operator has finished setting the deck up.
    #[must_use]
    pub fn configured(&self) -> bool {
        self.password_hash.is_some()
    }

    /// Refuse a configuration that should not come up.
    ///
    /// Note what is *not* here: there is no combination that produces a
    /// console without a password. Listening on the loopback interface
    /// buys one thing only — the second factor becomes optional.
    ///
    /// # Errors
    /// The deck would listen off the loopback interface without both a
    /// password and a second factor.
    pub fn validate(&self) -> Result<(), ConfigError> {
        // A session that may idle longer than it may live is one of the
        // two settings not meaning what it says, and which one is not
        // recoverable from the outside.
        if self.session_absolute < self.session_idle {
            return Err(ConfigError(
                "the absolute session lifetime is shorter than the idle one; one of the two \
                 does not mean what it says"
                    .to_owned(),
            ));
        }
        // Checked whatever the address. A hash that does not parse turned
        // every login into "wrong password" and every attempt into a
        // counted failure, so the operator locked themselves out chasing
        // a password that was never the problem.
        if let Some(hash) = &self.password_hash
            && matches!(
                oq_deck_core::auth::verify_password("", hash),
                Err(oq_deck_core::auth::AuthError::Corrupt(_))
            )
        {
            return Err(ConfigError(
                "OQ_DECK_PASSWORD_HASH 无法解析为 Argon2 hash。请重新生成，不要手工编辑。"
                    .to_owned(),
            ));
        }
        // An empty or mistyped secret is not a second factor. Empty, every
        // code an empty key produces was accepted — codes anyone can
        // compute; mistyped, every login failed and counted.
        if let Some(secret) = &self.totp_secret {
            match oq_deck_core::auth::decode_totp_secret(secret) {
                Ok(key) if key.len() >= 10 => {}
                _ => {
                    return Err(ConfigError(
                        "OQ_DECK_TOTP_SECRET 不是有效的 base32 密钥（至少 80 位）。".to_owned(),
                    ));
                }
            }
        }
        self.validate_upstream()?;
        if self.report_every_hours > REPORT_MAX_HOURS {
            return Err(ConfigError(format!(
                "OQ_DECK_REPORT_HOURS is not a whole number between 0 and {REPORT_MAX_HOURS}: {}",
                self.report_every_hours
            )));
        }
        if !self.is_exposed() {
            return Ok(());
        }
        if self.password_hash.is_none() {
            return Err(ConfigError(format!(
                "拒绝以对外可达的方式运行（{}）：尚未设置密码。请先绑定 127.0.0.1 完成初始设置\
                 （启动后终端会打印一次性令牌），或设置 OQ_DECK_PASSWORD_HASH。\
                 一个能下单的控制台不会不带认证地暴露在网络上。",
                self.host
            )));
        }
        if self.totp_secret.is_none() {
            return Err(ConfigError(format!(
                "拒绝以对外可达的方式运行（{}）：尚未启用第二因素。请设置 OQ_DECK_TOTP_SECRET。\
                 经反向代理或 OQ_DECK_EXTRA_HOSTS 发布同样算对外可达。",
                self.host
            )));
        }
        Ok(())
    }

    /// The upstream check's four settings, each refused rather than
    /// bent into something it does not say.
    fn validate_upstream(&self) -> Result<(), ConfigError> {
        if !oq_deck_core::upstream::is_repo(&self.upstream_repo) {
            return Err(ConfigError(format!(
                "OQ_DECK_UPSTREAM_REPO is not owner/name: {}",
                self.upstream_repo
            )));
        }
        if !oq_deck_core::upstream::is_repo(&self.self_repo) {
            return Err(ConfigError(format!(
                "OQ_DECK_SELF_REPO is not owner/name: {}",
                self.self_repo
            )));
        }
        if self.upstream_every_hours > UPSTREAM_MAX_HOURS {
            return Err(ConfigError(format!(
                "OQ_DECK_UPSTREAM_CHECK_HOURS is not a whole number between 0 and \
                 {UPSTREAM_MAX_HOURS}: {}",
                self.upstream_every_hours
            )));
        }
        if let Some(proxy) = &self.upstream_proxy {
            // Not echoed: a proxy URL may carry a password.
            if !proxy.starts_with("http://") || ureq::Proxy::new(proxy).is_err() {
                return Err(ConfigError(
                    "OQ_DECK_UPSTREAM_PROXY is not an http:// proxy URL".to_owned(),
                ));
            }
        }
        Ok(())
    }

    /// Read the environment, then validate.
    ///
    /// # Errors
    /// The environment does not parse, or `validate` refuses it.
    pub fn from_env() -> Result<Self, ConfigError> {
        let mut settings = Self::default();

        if let Ok(host) = std::env::var("OQ_DECK_HOST") {
            settings.host = host
                .parse()
                .map_err(|_| ConfigError(format!("OQ_DECK_HOST is not an address: {host}")))?;
        }
        if let Ok(port) = std::env::var("OQ_DECK_PORT") {
            settings.port = port
                .parse()
                .map_err(|_| ConfigError(format!("OQ_DECK_PORT is not a port: {port}")))?;
        }
        settings.runs_dir = std::env::var("OQ_DECK_RUNS_DIR").ok().map(PathBuf::from);
        settings.journals_dir = std::env::var("OQ_DECK_JOURNALS_DIR")
            .ok()
            .map(PathBuf::from);
        settings.ticks_dir = std::env::var("OQ_DECK_TICKS_DIR").ok().map(PathBuf::from);
        settings.agent_socket = std::env::var("OQ_DECK_AGENT_SOCKET")
            .ok()
            .map(PathBuf::from);
        settings.venue_record = std::env::var("OQ_DECK_VENUE_RECORD")
            .ok()
            .map(PathBuf::from);
        // An empty variable is an unset one: `OQ_DECK_TOTP_SECRET=` in an
        // environment file read as a second factor with an empty key.
        let set = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        // A count, or absent. Bounded here rather than clamped later: a
        // setting silently rounded to something else is a setting that
        // does not mean what it says.
        let count = |name: &str, max: u64| -> Result<Option<u64>, ConfigError> {
            let Some(text) = set(name) else {
                return Ok(None);
            };
            match text.trim().parse::<u64>() {
                Ok(n) if (1..=max).contains(&n) => Ok(Some(n)),
                _ => Err(ConfigError(format!(
                    "{name} is not a whole number between 1 and {max}: {text}"
                ))),
            }
        };
        settings.password_hash = set("OQ_DECK_PASSWORD_HASH");
        settings.totp_secret = set("OQ_DECK_TOTP_SECRET");
        // `STATE_DIRECTORY` is what systemd sets for `StateDirectory=`,
        // so a unit that asks for one gets this without also naming a
        // variable. The explicit one wins when both are there.
        settings.state_dir = set("OQ_DECK_STATE_DIR")
            .or_else(|| set("STATE_DIRECTORY"))
            .map(PathBuf::from);
        settings.trusted_keys = set("OQ_DECK_TRUSTED_KEYS").map(PathBuf::from);
        settings.behind_tls = std::env::var("OQ_DECK_BEHIND_TLS").as_deref() == Ok("1");
        settings.allow_writes = std::env::var("OQ_DECK_ALLOW_WRITES").as_deref() == Ok("1");
        // Both bounded, and the idle one below the absolute one: a
        // session that can idle longer than it can live would be a
        // setting that reads as though it does something.
        if let Some(minutes) = count("OQ_DECK_SESSION_IDLE_MINUTES", 1440)? {
            settings.session_idle = Duration::from_secs(minutes * 60);
        }
        if let Some(days) = count("OQ_DECK_DEVICE_DAYS", 365)? {
            settings.device_lifetime = Duration::from_secs(days * 24 * 3600);
        }
        if let Some(hours) = count("OQ_DECK_SESSION_HOURS", 720)? {
            settings.session_absolute = Duration::from_secs(hours * 3600);
        }

        if let Some(repo) = set("OQ_DECK_UPSTREAM_REPO") {
            settings.upstream_repo = repo.trim().to_owned();
        }
        if let Some(repo) = set("OQ_DECK_SELF_REPO") {
            settings.self_repo = repo.trim().to_owned();
        }
        // Zero is a value here, not an absence: it is how the check is
        // turned off, so `count` (which starts at one) does not fit.
        if let Some(text) = set("OQ_DECK_UPSTREAM_CHECK_HOURS") {
            settings.upstream_every_hours = text.trim().parse::<u64>().map_err(|_| {
                ConfigError(format!(
                    "OQ_DECK_UPSTREAM_CHECK_HOURS is not a whole number between 0 and \
                     {UPSTREAM_MAX_HOURS}: {text}"
                ))
            })?;
        }
        settings.upstream_proxy = set("OQ_DECK_UPSTREAM_PROXY").map(|p| p.trim().to_owned());

        // Reports live beside the deck's other state unless told
        // otherwise; with neither, there is nowhere to keep them.
        settings.reports_dir = set("OQ_DECK_REPORTS_DIR")
            .map(PathBuf::from)
            .or_else(|| settings.state_dir.as_ref().map(|d| d.join("reports")));
        if let Some(text) = set("OQ_DECK_REPORT_HOURS") {
            settings.report_every_hours = text.trim().parse::<u64>().map_err(|_| {
                ConfigError(format!(
                    "OQ_DECK_REPORT_HOURS is not a whole number between 0 and \
                     {REPORT_MAX_HOURS}: {text}"
                ))
            })?;
        }

        settings.extra_hosts = std::env::var("OQ_DECK_EXTRA_HOSTS")
            .ok()
            .map(|value| {
                value
                    .split(',')
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        settings.validate()?;
        Ok(settings)
    }
}
