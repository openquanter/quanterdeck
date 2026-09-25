//! Where everything is, read once from the environment the unit sets.
//!
//! Paths and names only. Secrets are files in `$CREDENTIALS_DIRECTORY`
//! (systemd `LoadCredential=`), read where they are used and never held in
//! the environment.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    /// The socket the deck connects to.
    pub socket: PathBuf,
    /// Users whose connections are accepted, by uid.
    pub peers: Vec<u32>,
    /// Units whose state is reported.
    pub units: Vec<String>,
    /// Units that may be started, stopped and restarted.
    pub manageable: Vec<String>,
    /// The trading process's unit.
    pub trader_unit: String,
    /// Where the trader's control socket is.
    pub control_dir: PathBuf,
    /// Log files that may be read.
    pub log_dir: PathBuf,
    /// The audit trail and deployment records.
    pub state_dir: PathBuf,
    /// Installed releases and the `current` / `previous` links.
    pub releases: PathBuf,
    /// Where signed releases are staged.
    pub incoming: PathBuf,
    /// Strategy config files the agent manages.
    pub config_dir: PathBuf,
    /// The trader's journals, run files and tick files.
    pub journals: PathBuf,
    /// `ssh-keygen -Y verify`'s allowed-signers file.
    pub signers: PathBuf,
    /// Credentials handed over by systemd.
    pub credentials: Option<PathBuf>,
    /// This host's name, in messages.
    pub host: String,
    /// Discord: guild, channel name, and the proxy that reaches it.
    pub discord_guild: Option<String>,
    pub discord_channel: String,
    pub proxy: Option<String>,
}

fn var(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn list(name: &str, default: &str) -> Vec<String> {
    var(name, default)
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// A local user's uid, from `/etc/passwd`: the peers are system users.
pub fn uid_of(user: &str) -> Result<u32, String> {
    let passwd = std::fs::read_to_string("/etc/passwd").map_err(|e| format!("/etc/passwd: {e}"))?;
    passwd
        .lines()
        .map(|l| l.split(':').collect::<Vec<_>>())
        .find(|f| f.first() == Some(&user))
        .and_then(|f| f.get(2).and_then(|u| u.parse().ok()))
        .ok_or_else(|| format!("{user:?} is not a user here"))
}

impl Config {
    /// # Errors
    /// A peer that does not resolve, or no socket location.
    pub fn from_env() -> Result<Self, String> {
        let runtime = std::env::var("RUNTIME_DIRECTORY")
            .ok()
            .and_then(|d| d.split(':').next().map(PathBuf::from));
        let socket = match std::env::var("OQ_AGENT_SOCKET") {
            Ok(p) => PathBuf::from(p),
            Err(_) => runtime
                .ok_or(
                    "no RUNTIME_DIRECTORY and no OQ_AGENT_SOCKET; refusing to pick a shared place",
                )?
                .join("agent.sock"),
        };
        let mut peers = Vec::new();
        for user in list("OQ_AGENT_PEERS", "oq-deck") {
            peers.push(uid_of(&user)?);
        }
        Ok(Self {
            socket,
            peers,
            units: list(
                "OQ_AGENT_UNITS",
                "oqp-live.service,oq-recon.service,oq-deck.service,oq-agent.service,caddy.service",
            ),
            manageable: list("OQ_AGENT_MANAGEABLE", "oqp-live.service,oq-recon.service"),
            trader_unit: var("OQ_AGENT_TRADER_UNIT", "oqp-live.service"),
            control_dir: PathBuf::from(var("OQ_AGENT_CONTROL_DIR", "/run/oq-live")),
            log_dir: PathBuf::from(var("OQ_AGENT_LOG_DIR", "/var/log/oq")),
            state_dir: PathBuf::from(var("OQ_AGENT_STATE", "/var/lib/oq-agent")),
            releases: PathBuf::from(var("OQ_AGENT_RELEASES", "/opt/oq/releases")),
            incoming: PathBuf::from(var("OQ_AGENT_INCOMING", "/var/lib/oq/incoming")),
            config_dir: PathBuf::from(var("OQ_AGENT_CONFIG_DIR", "/var/lib/oq/config")),
            journals: PathBuf::from(var("OQ_AGENT_JOURNALS", "/var/lib/oq/journals")),
            signers: PathBuf::from(var("OQ_AGENT_SIGNERS", "/etc/oq/allowed_signers")),
            credentials: std::env::var_os("CREDENTIALS_DIRECTORY").map(PathBuf::from),
            host: var("OQ_AGENT_HOST", "host"),
            discord_guild: std::env::var("OQ_AGENT_DISCORD_GUILD").ok(),
            discord_channel: var("OQ_AGENT_DISCORD_CHANNEL", "监控告警"),
            proxy: std::env::var("OQ_AGENT_PROXY")
                .ok()
                .filter(|p| !p.is_empty()),
        })
    }

    /// A credential's contents, trimmed; `None` if it was not given.
    #[must_use]
    pub fn credential(&self, name: &str) -> Option<String> {
        let dir = self.credentials.as_ref()?;
        std::fs::read_to_string(dir.join(name))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }
}
