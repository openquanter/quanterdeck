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
    /// How long a session survives at all, however active.
    ///
    /// Loosening this is how an operator trades a stolen laptop for not
    /// signing in twice a day, so it is a named setting rather than a
    /// constant that only a rebuild could move.
    pub session_absolute: Duration,
}

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
            session_idle: Duration::from_secs(60 * 60),
            session_absolute: Duration::from_secs(12 * 60 * 60),
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
        settings.behind_tls = std::env::var("OQ_DECK_BEHIND_TLS").as_deref() == Ok("1");
        settings.allow_writes = std::env::var("OQ_DECK_ALLOW_WRITES").as_deref() == Ok("1");
        // Both bounded, and the idle one below the absolute one: a
        // session that can idle longer than it can live would be a
        // setting that reads as though it does something.
        if let Some(minutes) = count("OQ_DECK_SESSION_IDLE_MINUTES", 1440)? {
            settings.session_idle = Duration::from_secs(minutes * 60);
        }
        if let Some(hours) = count("OQ_DECK_SESSION_HOURS", 720)? {
            settings.session_absolute = Duration::from_secs(hours * 3600);
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
