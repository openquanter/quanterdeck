//! Messages to a person: a Discord channel, through a bot.
//!
//! The same bot and channel the 1.x monitors post to. Sending is best
//! effort and off the request path: an alert that cannot be delivered is
//! printed, and never delays or fails what raised it.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use serde_json::{Value, json};

/// A message waiting to be sent.
#[derive(Debug, Clone)]
pub struct Message {
    pub title: String,
    pub body: String,
    /// Red for a problem, green for a recovery, blue for an action.
    pub color: u32,
}

pub const RED: u32 = 0xe7_4c_3c;
pub const GREEN: u32 = 0x2e_cc_71;
pub const BLUE: u32 = 0x34_98_db;

/// Where messages are sent.
#[derive(Debug, Clone)]
pub struct Discord {
    token: String,
    guild: String,
    channel_name: String,
    proxy: Option<String>,
}

const API: &str = "https://discord.com/api/v10";

impl Discord {
    #[must_use]
    pub fn new(token: String, guild: String, channel_name: String, proxy: Option<String>) -> Self {
        Self {
            token,
            guild,
            channel_name,
            proxy,
        }
    }

    fn agent(&self) -> Result<ureq::Agent, String> {
        let mut cfg = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .http_status_as_error(false)
            .https_only(true);
        if let Some(p) = &self.proxy {
            cfg = cfg.proxy(Some(ureq::Proxy::new(p).map_err(|e| e.to_string())?));
        }
        Ok(cfg.build().into())
    }

    fn channel_id(&self, agent: &ureq::Agent) -> Result<String, String> {
        let mut resp = agent
            .get(&format!("{API}/guilds/{}/channels", self.guild))
            .header("Authorization", &format!("Bot {}", self.token))
            .call()
            .map_err(|e| e.to_string())?;
        let text = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| e.to_string())?;
        let channels: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        channels
            .as_array()
            .and_then(|cs| {
                cs.iter().find(|c| {
                    c["type"] == 0 && c["name"].as_str() == Some(self.channel_name.as_str())
                })
            })
            .and_then(|c| c["id"].as_str().map(str::to_string))
            .ok_or_else(|| format!("no text channel named {} in the guild", self.channel_name))
    }

    /// Post one message.
    ///
    /// # Errors
    /// The channel could not be found or Discord refused the post.
    pub fn send(&self, m: &Message, host: &str) -> Result<(), String> {
        let agent = self.agent()?;
        let id = self.channel_id(&agent)?;
        let body = json!({"embeds": [{
            "title": truncate(&m.title, 250),
            "description": truncate(&m.body, 4000),
            "color": m.color,
            "footer": {"text": format!("oq-agent · {host}")},
        }]});
        let resp = agent
            .post(&format!("{API}/channels/{id}/messages"))
            .header("Authorization", &format!("Bot {}", self.token))
            .header("Content-Type", "application/json")
            .send(serde_json::to_string(&body).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("discord answered {}", resp.status()))
        }
    }
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// A queue drained by one thread, so senders never wait on the network.
#[must_use]
pub fn start(discord: Option<Discord>, host: String) -> Sender<Message> {
    let (tx, rx): (Sender<Message>, Receiver<Message>) = channel();
    std::thread::spawn(move || {
        for m in rx {
            match &discord {
                Some(d) => {
                    if let Err(e) = d.send(&m, &host) {
                        eprintln!("notify: could not post {:?}: {e}", m.title);
                    }
                }
                None => eprintln!("notify (no channel configured): {} — {}", m.title, m.body),
            }
        }
    });
    tx
}
