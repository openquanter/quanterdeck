//! Messages to a person: a Discord channel and/or a Telegram chat, each
//! through a bot.
//!
//! Discord is the same bot and channel the 1.x monitors post to. Sending
//! is best effort and off the request path: an alert that cannot be
//! delivered is printed, and never delays or fails what raised it. Every
//! configured channel gets every message, and one that fails does not
//! hold up the others.
//!
//! Only Discord is read back. The audit trail's and the journal's anchors
//! (`Discord::recent`) depend on reading what this host posted earlier
//! from somewhere this host cannot rewrite; a Telegram bot cannot list a
//! chat's history (`getUpdates` only returns updates addressed to the
//! bot, for at most a day, and never the bot's own messages), so Telegram
//! is a delivery channel only and never an off-machine anchor.

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
#[derive(Clone)]
pub struct Discord {
    token: String,
    guild: String,
    channel_name: String,
    proxy: Option<String>,
}

impl core::fmt::Debug for Discord {
    /// Hand-written because a derived one prints the bot token, and a
    /// token that reaches a log is a token that has to be rotated. The
    /// rest is here so that a `{:?}` in a test tells you which channel
    /// it was talking about.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Discord")
            .field("guild", &self.guild)
            .field("channel_name", &self.channel_name)
            .field("proxy", &self.proxy)
            .field("token", &"<redacted>")
            .finish()
    }
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

    /// The descriptions of the channel's most recent messages.
    ///
    /// The other half of mirroring the audit trail off this host. What
    /// was posted cannot be unposted by anyone who reaches this machine
    /// afterwards, so reading it back is what tells a trail that was cut
    /// short from one that was always that short — the local chain
    /// cannot, because what is left of it verifies.
    ///
    /// # Errors
    /// The channel could not be found, or Discord refused the read.
    pub fn recent(&self, limit: u32) -> Result<Vec<String>, String> {
        let agent = self.agent()?;
        let id = self.channel_id(&agent)?;
        let mut resp = agent
            .get(&format!(
                "{API}/channels/{id}/messages?limit={}",
                limit.clamp(1, 100)
            ))
            .header("Authorization", &format!("Bot {}", self.token))
            .call()
            .map_err(|e| e.to_string())?;
        let text = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| e.to_string())?;
        let messages: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        Ok(messages
            .as_array()
            .map(|ms| {
                ms.iter()
                    .filter_map(|m| m["embeds"][0]["description"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default())
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

/// One place messages are delivered to.
///
/// A trait so that delivering to several channels — and one failing
/// without stopping the rest — can be tested without a network.
pub trait Sink: Send {
    /// The channel's kind, as the console names it: `discord`, `telegram`.
    fn name(&self) -> &'static str;
    /// Deliver one message.
    ///
    /// # Errors
    /// The channel refused or could not be reached. The text never carries
    /// a credential: it is printed.
    fn send(&self, m: &Message, host: &str) -> Result<(), String>;
}

impl Sink for Discord {
    fn name(&self) -> &'static str {
        "discord"
    }
    fn send(&self, m: &Message, host: &str) -> Result<(), String> {
        Discord::send(self, m, host)
    }
}

/// A Telegram chat, through a bot.
///
/// Delivery only: see the module documentation for why it is never read
/// back as an anchor.
#[derive(Clone)]
pub struct Telegram {
    token: String,
    chat: String,
    proxy: Option<String>,
}

impl core::fmt::Debug for Telegram {
    /// Hand-written for the same reason as `Discord`'s: the bot token is
    /// the whole credential, and here it is also part of every request URL.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Telegram")
            .field("chat", &self.chat)
            .field("proxy", &self.proxy)
            .field("token", &"<redacted>")
            .finish()
    }
}

const TELEGRAM_API: &str = "https://api.telegram.org";
/// Telegram's limit on a message's text, in UTF-16 code units.
const TELEGRAM_MAX: usize = 4096;

/// The UTF-16 length Telegram counts a text by.
fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// The longest prefix of `s` that is at most `max` UTF-16 code units,
/// cut on a char boundary.
fn take_utf16(s: &str, max: usize) -> &str {
    let mut used = 0;
    for (i, c) in s.char_indices() {
        used += c.len_utf16();
        if used > max {
            return &s[..i];
        }
    }
    s
}

/// A message as Telegram plain text: the title, the body, and a footer
/// naming the host, within Telegram's length limit. The footer is kept
/// whole; what does not fit is cut from the end of the body.
#[must_use]
pub fn telegram_text(m: &Message, host: &str) -> String {
    let footer = format!("\n\noq-agent · {host}");
    let head = if m.body.is_empty() {
        m.title.clone()
    } else {
        format!("{}\n\n{}", m.title, m.body)
    };
    let budget = TELEGRAM_MAX.saturating_sub(utf16_len(&footer));
    let head = if utf16_len(&head) > budget {
        // One unit for the ellipsis that says it was cut.
        format!("{}…", take_utf16(&head, budget.saturating_sub(1)))
    } else {
        head
    };
    take_utf16(&format!("{head}{footer}"), TELEGRAM_MAX).to_string()
}

/// The `sendMessage` request for one message: URL and JSON body.
///
/// Pure, so what is sent can be tested without sending it. The URL holds
/// the bot token; it goes to the HTTP client and nowhere else.
#[must_use]
pub fn telegram_request(token: &str, chat: &str, m: &Message, host: &str) -> (String, Value) {
    (
        format!("{TELEGRAM_API}/bot{token}/sendMessage"),
        json!({
            "chat_id": chat,
            "text": telegram_text(m, host),
            "disable_web_page_preview": true,
        }),
    )
}

/// An error's text with every form of the token taken out.
///
/// The token is in the request URL, and an HTTP client's error can quote
/// the URL — as written or percent-encoded. The part after the colon is
/// the secret, so it is removed on its own too, in case the bot id was
/// rewritten around it.
#[must_use]
pub fn scrub(text: &str, token: &str) -> String {
    if token.is_empty() {
        return text.to_string();
    }
    let mut out = text.replace(token, "<redacted>");
    out = out.replace(&token.replace(':', "%3A"), "<redacted>");
    out = out.replace(&token.replace(':', "%3a"), "<redacted>");
    if let Some((_, secret)) = token.split_once(':')
        && !secret.is_empty()
    {
        out = out.replace(secret, "<redacted>");
    }
    out
}

impl Telegram {
    #[must_use]
    pub fn new(token: String, chat: String, proxy: Option<String>) -> Self {
        Self { token, chat, proxy }
    }

    /// The channel from its two halves, if both are there.
    ///
    /// The second value is a warning when exactly one is: a half-made
    /// configuration is a mistake to say out loud, not a channel that is
    /// quietly off. It names what is missing, never the value present.
    #[must_use]
    pub fn from_parts(
        token: Option<String>,
        chat: Option<String>,
        proxy: Option<String>,
    ) -> (Option<Self>, Option<&'static str>) {
        match (token, chat) {
            (Some(t), Some(c)) => (Some(Self::new(t, c, proxy)), None),
            (Some(_), None) => (
                None,
                Some(
                    "a TELEGRAM_BOT_TOKEN credential is present but OQ_AGENT_TELEGRAM_CHAT is \
                     not set; Telegram alerts are off",
                ),
            ),
            (None, Some(_)) => (
                None,
                Some(
                    "OQ_AGENT_TELEGRAM_CHAT is set but there is no TELEGRAM_BOT_TOKEN \
                     credential; Telegram alerts are off",
                ),
            ),
            (None, None) => (None, None),
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

    /// Post one message.
    ///
    /// # Errors
    /// Telegram could not be reached or refused the post. The text is
    /// token-free.
    pub fn send(&self, m: &Message, host: &str) -> Result<(), String> {
        let hide = |e: String| format!("telegram: {}", scrub(&e, &self.token));
        let agent = self.agent().map_err(hide)?;
        let (url, body) = telegram_request(&self.token, &self.chat, m, host);
        let body = serde_json::to_string(&body).map_err(|e| hide(e.to_string()))?;
        let mut resp = agent
            .post(&url)
            .header("Content-Type", "application/json")
            .send(body)
            .map_err(|e| hide(e.to_string()))?;
        if resp.status().is_success() {
            return Ok(());
        }
        // Telegram explains a refusal in `description` ("chat not found",
        // "bot was blocked by the user"), which is what tells an operator
        // which half of the configuration is wrong.
        let status = resp.status();
        let why = resp
            .body_mut()
            .read_to_string()
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v["description"].as_str().map(str::to_string))
            .unwrap_or_default();
        Err(hide(
            format!("answered {status} {why}").trim_end().to_string(),
        ))
    }
}

impl Sink for Telegram {
    fn name(&self) -> &'static str {
        "telegram"
    }
    fn send(&self, m: &Message, host: &str) -> Result<(), String> {
        Telegram::send(self, m, host)
    }
}

/// Deliver one message to every sink; the ones that failed, by name, with
/// why. Each sink is tried whatever the ones before it did.
pub fn deliver(sinks: &[Box<dyn Sink>], m: &Message, host: &str) -> Vec<(&'static str, String)> {
    let mut failed = Vec::new();
    for s in sinks {
        if let Err(e) = s.send(m, host) {
            failed.push((s.name(), e));
        }
    }
    failed
}

/// A queue drained by one thread, so senders never wait on the network.
///
/// Every message goes to every sink, in order; one slow or failing
/// channel delays the others by at most its timeout and never costs them
/// their copy. With no sink at all, messages are printed.
#[must_use]
pub fn start(sinks: Vec<Box<dyn Sink>>, host: String) -> Sender<Message> {
    let (tx, rx): (Sender<Message>, Receiver<Message>) = channel();
    std::thread::spawn(move || {
        for m in rx {
            if sinks.is_empty() {
                eprintln!("notify (no channel configured): {} — {}", m.title, m.body);
                continue;
            }
            for (name, e) in deliver(&sinks, &m, &host) {
                eprintln!("notify: {name} could not post {:?}: {e}", m.title);
            }
        }
    });
    tx
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // Shaped like a bot token, and not one.
    const TOKEN: &str = "123456789:AAFakeSecretPartForTestsOnly_xyz";

    fn msg(title: &str, body: &str) -> Message {
        Message {
            title: title.into(),
            body: body.into(),
            color: RED,
        }
    }

    #[test]
    fn the_request_goes_to_send_message_with_a_plain_text_body() {
        let (url, body) = telegram_request(TOKEN, "-100123", &msg("停机", "原因"), "h1");
        assert_eq!(
            url,
            format!("https://api.telegram.org/bot{TOKEN}/sendMessage")
        );
        assert_eq!(body["chat_id"], "-100123");
        assert_eq!(body["disable_web_page_preview"], true);
        assert_eq!(body["text"], "停机\n\n原因\n\noq-agent · h1");
        assert!(body.get("parse_mode").is_none(), "plain text, no markup");
    }

    #[test]
    fn an_empty_body_does_not_leave_a_gap() {
        assert_eq!(telegram_text(&msg("t", ""), "h"), "t\n\noq-agent · h");
    }

    #[test]
    fn a_long_message_is_cut_to_the_limit_and_keeps_its_footer() {
        // Multi-byte and astral (two UTF-16 units) characters, so a cut by
        // bytes or by chars would both be wrong.
        let body = "告😀".repeat(3000);
        let text = telegram_text(&msg("title", &body), "host-a");
        assert!(utf16_len(&text) <= TELEGRAM_MAX, "{}", utf16_len(&text));
        assert!(utf16_len(&text) >= TELEGRAM_MAX - 2);
        assert!(text.ends_with("…\n\noq-agent · host-a"));
        assert!(text.starts_with("title\n\n告😀"));
    }

    #[test]
    fn a_message_at_the_limit_is_not_cut() {
        let footer = utf16_len("\n\noq-agent · h");
        let body = "a".repeat(TELEGRAM_MAX - footer - 3);
        let text = telegram_text(&msg("t", &body), "h");
        assert_eq!(utf16_len(&text), TELEGRAM_MAX);
        assert!(!text.contains('…'));
    }

    #[test]
    fn debug_never_prints_the_token() {
        let t = Telegram::new(TOKEN.into(), "-100123".into(), None);
        let shown = format!("{t:?}");
        assert!(!shown.contains("AAFakeSecret"), "{shown}");
        assert!(shown.contains("<redacted>"));
        assert!(shown.contains("-100123"));
    }

    #[test]
    fn errors_are_scrubbed_of_the_token_in_every_form() {
        let raw = format!(
            "io: connect https://api.telegram.org/bot{TOKEN}/sendMessage; also bot{}/x; and {}",
            TOKEN.replace(':', "%3A"),
            TOKEN.split_once(':').unwrap().1
        );
        let clean = scrub(&raw, TOKEN);
        assert!(!clean.contains("AAFakeSecret"), "{clean}");
        assert!(clean.contains("api.telegram.org/bot<redacted>/sendMessage"));
    }

    #[test]
    fn a_failed_send_reports_without_the_token() {
        // A proxy that cannot parse fails before any network is touched.
        let t = Telegram::new(TOKEN.into(), "1".into(), Some("::not a proxy::".into()));
        let e = t.send(&msg("t", "b"), "h").unwrap_err();
        assert!(e.starts_with("telegram: "), "{e}");
        assert!(!e.contains("AAFakeSecret"), "{e}");
    }

    #[test]
    fn half_a_telegram_configuration_is_off_and_says_which_half() {
        let (t, w) = Telegram::from_parts(Some(TOKEN.into()), None, None);
        assert!(t.is_none());
        let w = w.unwrap();
        assert!(w.contains("OQ_AGENT_TELEGRAM_CHAT") && !w.contains("AAFakeSecret"));
        let (t, w) = Telegram::from_parts(None, Some("4242".into()), None);
        assert!(t.is_none());
        let w = w.unwrap();
        assert!(w.contains("TELEGRAM_BOT_TOKEN") && !w.contains("4242"));
        assert!(matches!(
            Telegram::from_parts(None, None, None),
            (None, None)
        ));
        assert!(matches!(
            Telegram::from_parts(Some(TOKEN.into()), Some("4242".into()), None),
            (Some(_), None)
        ));
    }

    struct Fake {
        name: &'static str,
        fail: bool,
        got: Arc<Mutex<Vec<String>>>,
    }

    impl Sink for Fake {
        fn name(&self) -> &'static str {
            self.name
        }
        fn send(&self, m: &Message, host: &str) -> Result<(), String> {
            self.got.lock().unwrap().push(format!("{}@{host}", m.title));
            if self.fail {
                Err("down".into())
            } else {
                Ok(())
            }
        }
    }

    fn fake(name: &'static str, fail: bool, got: &Arc<Mutex<Vec<String>>>) -> Box<dyn Sink> {
        Box::new(Fake {
            name,
            fail,
            got: Arc::clone(got),
        })
    }

    #[test]
    fn one_channel_failing_does_not_stop_the_other() {
        let a = Arc::new(Mutex::new(Vec::new()));
        let b = Arc::new(Mutex::new(Vec::new()));
        let sinks = vec![fake("discord", true, &a), fake("telegram", false, &b)];
        let failed = deliver(&sinks, &msg("x", "y"), "h");
        assert_eq!(failed, vec![("discord", "down".to_string())]);
        assert_eq!(*a.lock().unwrap(), vec!["x@h"]);
        assert_eq!(*b.lock().unwrap(), vec!["x@h"]);

        // The other way round: the first succeeding does not stand in for
        // the second.
        let sinks = vec![fake("discord", false, &a), fake("telegram", true, &b)];
        assert_eq!(
            deliver(&sinks, &msg("z", ""), "h"),
            vec![("telegram", "down".to_string())]
        );
        assert_eq!(*a.lock().unwrap(), vec!["x@h", "z@h"]);
        assert_eq!(*b.lock().unwrap(), vec!["x@h", "z@h"]);
    }

    #[test]
    fn the_queue_delivers_every_message_to_every_sink() {
        let a = Arc::new(Mutex::new(Vec::new()));
        let b = Arc::new(Mutex::new(Vec::new()));
        let tx = start(
            vec![fake("discord", true, &a), fake("telegram", false, &b)],
            "h".into(),
        );
        tx.send(msg("one", "")).unwrap();
        tx.send(msg("two", "")).unwrap();
        drop(tx);
        // Wait for the drain thread by polling its effect, bounded so a
        // hang fails the test instead of stalling it.
        for _ in 0..400 {
            if b.lock().unwrap().len() == 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(*a.lock().unwrap(), vec!["one@h", "two@h"]);
        assert_eq!(*b.lock().unwrap(), vec!["one@h", "two@h"]);
    }
}
