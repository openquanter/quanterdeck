//! Configuration, and the guards that run before it is accepted.
//!
//! The defaults are the safe ones. Reaching an unsafe configuration takes
//! a deliberate act with a name attached, and `validate` refuses the
//! combinations that would put a trading console on a network with no
//! authentication.

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Settings {
    pub host: IpAddr,
    pub port: u16,
    pub runs_dir: Option<PathBuf>,
    pub password_hash: Option<String>,
    pub totp_secret: Option<String>,
    /// Off by default; every mutating route is refused while it is off.
    pub allow_writes: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 8899,
            runs_dir: None,
            password_hash: None,
            totp_secret: None,
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

    /// Refuse a configuration that should not come up.
    ///
    /// # Errors
    /// The deck would listen off the loopback interface without a
    /// password, or without a second factor.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.is_loopback() {
            return Ok(());
        }
        if self.password_hash.is_none() {
            return Err(ConfigError(format!(
                "refusing to listen on {} without a password. Either bind \
                 127.0.0.1, or set OQ_DECK_PASSWORD_HASH. A console that can \
                 place orders is not put on a network unauthenticated.",
                self.host
            )));
        }
        if self.totp_secret.is_none() {
            return Err(ConfigError(format!(
                "refusing to listen on {} without a second factor. Set \
                 OQ_DECK_TOTP_SECRET, or bind 127.0.0.1.",
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
        settings.password_hash = std::env::var("OQ_DECK_PASSWORD_HASH").ok();
        settings.totp_secret = std::env::var("OQ_DECK_TOTP_SECRET").ok();
        settings.allow_writes = std::env::var("OQ_DECK_ALLOW_WRITES").as_deref() == Ok("1");

        settings.validate()?;
        Ok(settings)
    }
}
