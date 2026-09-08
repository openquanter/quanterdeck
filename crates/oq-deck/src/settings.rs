//! Configuration, and the guards that run before it is accepted.
//!
//! The defaults are the safe ones. Reaching a less safe configuration
//! takes a deliberate act with a name attached.

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Settings {
    pub host: IpAddr,
    pub port: u16,
    pub runs_dir: Option<PathBuf>,
    pub journals_dir: Option<PathBuf>,
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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 8899,
            runs_dir: None,
            journals_dir: None,
            password_hash: None,
            totp_secret: None,
            extra_hosts: Vec::new(),
            behind_tls: false,
            allow_writes: false,
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
        if self.is_loopback() {
            return Ok(());
        }
        if self.password_hash.is_none() {
            return Err(ConfigError(format!(
                "拒绝在 {} 上监听：尚未设置密码。请先绑定 127.0.0.1 完成初始设置\
                 （启动后终端会打印一次性令牌），或设置 OQ_DECK_PASSWORD_HASH。\
                 一个能下单的控制台不会不带认证地暴露在网络上。",
                self.host
            )));
        }
        if self.totp_secret.is_none() {
            return Err(ConfigError(format!(
                "拒绝在 {} 上监听：尚未启用第二因素。请设置 OQ_DECK_TOTP_SECRET，\
                 或改为绑定 127.0.0.1。",
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
        settings.password_hash = std::env::var("OQ_DECK_PASSWORD_HASH").ok();
        settings.totp_secret = std::env::var("OQ_DECK_TOTP_SECRET").ok();
        settings.behind_tls = std::env::var("OQ_DECK_BEHIND_TLS").as_deref() == Ok("1");
        settings.allow_writes = std::env::var("OQ_DECK_ALLOW_WRITES").as_deref() == Ok("1");
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
