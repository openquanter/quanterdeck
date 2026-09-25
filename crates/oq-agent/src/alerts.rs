//! Conditions worth telling a person about, watched from outside the
//! trader.
//!
//! Every thirty seconds the agent reads the trader's status, the units and
//! the host, and compares with the last look. A condition is announced
//! when it starts and again when it clears — edges, not levels, so a halt
//! that lasts a day is one message and not two thousand.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use crate::config::Config;
use crate::notify::{GREEN, Message, RED};
use crate::system;

/// A sentence for a person, in Chinese and in English.
///
/// The deck shows alerts in its reader's language, so the agent writes
/// both where it words them; the notification channel gets the Chinese.
pub use oq_deck_core::lang::Said;

/// Alerts currently raised, by key: since when, and the message.
#[derive(Debug, Default)]
pub struct Raised {
    pub active: BTreeMap<String, (i64, Said)>,
    /// Set when an operator stopped the trader on purpose, so its absence
    /// is not alarming; cleared when it is started again.
    pub trader_stopped_on_purpose: bool,
    /// Raised and cleared, newest last, the most recent few hundred.
    pub history: std::collections::VecDeque<(i64, String, Said, bool)>,
    /// Keys not to notify about until the given time.
    pub silenced: BTreeMap<String, i64>,
}

impl Raised {
    fn note(&mut self, at: i64, key: &str, msg: &Said, raised: bool) {
        self.history
            .push_back((at, key.to_string(), msg.clone(), raised));
        while self.history.len() > 300 {
            self.history.pop_front();
        }
    }

    fn quiet(&self, key: &str, now: i64) -> bool {
        self.silenced.get(key).is_some_and(|until| *until > now)
    }
}

/// What one look found: keys and messages of the conditions that hold.
#[must_use]
pub fn assess(
    status: Result<&Value, &Said>,
    units: &[(String, bool)],
    trader_unit: &str,
    host: &Value,
    stopped_on_purpose: bool,
    last_unreadable: Option<u64>,
) -> BTreeMap<String, Said> {
    let mut found = BTreeMap::new();
    let trader_up = units.iter().any(|(u, up)| u == trader_unit && *up);
    for (unit, up) in units {
        if !up && !(unit == trader_unit && stopped_on_purpose) {
            found.insert(
                format!("unit:{unit}"),
                Said::new(
                    format!("{unit} 没有在运行"),
                    format!("{unit} is not running"),
                ),
            );
        }
    }
    match status {
        Ok(s) => {
            if s["halted"] == true {
                let said = match s["halt_reason"].as_str() {
                    Some(why) => Said::new(
                        format!("交易进程已停机：{why}"),
                        format!("The trader has halted: {why}"),
                    ),
                    None => Said::new(
                        "交易进程已停机：原因未记录",
                        "The trader has halted; no reason was recorded",
                    ),
                };
                found.insert("halted".into(), said);
            }
            if let Some(j) = s["journal_lost"].as_str() {
                found.insert(
                    "journal".into(),
                    Said::new(
                        format!("交易日志无法写入，已停止开新单：{j}"),
                        format!("The journal cannot be written; no new orders are opened: {j}"),
                    ),
                );
            }
            if s["reconcile"]["agreed"] == false {
                found.insert(
                    "reconcile".into(),
                    Said::new(
                        format!(
                            "最近一次持仓核对与交易所不一致（累计 {} 次）",
                            s["reconcile"]["mismatches"]
                        ),
                        format!(
                            "The latest position check disagreed with the venue ({} so far this run)",
                            s["reconcile"]["mismatches"]
                        ),
                    ),
                );
            }
            let unreadable = s["feed"]["unreadable"].as_u64().unwrap_or(0);
            if last_unreadable.is_some_and(|before| unreadable > before) {
                found.insert(
                    "feed".into(),
                    Said::new(
                        format!("行情出现读不出的消息：累计 {unreadable}"),
                        format!(
                            "The feed sent messages that could not be read: {unreadable} so far"
                        ),
                    ),
                );
            }
        }
        Err(why) if trader_up => {
            // Why the port could not be reached is the agent's own
            // sentence, so it goes into the alert in the reader's
            // language rather than being carried across as one half.
            found.insert(
                "control".into(),
                Said::new(
                    format!("交易进程在运行，但控制口无应答：{}", why.zh),
                    format!(
                        "The trader is running but its control port does not answer: {}",
                        why.en
                    ),
                ),
            );
        }
        Err(_) => {}
    }
    if let Some(disks) = host["disks"].as_array() {
        for d in disks {
            if let (Some(size), Some(avail)) = (d["size"].as_u64(), d["avail"].as_u64())
                && size > 0
                && avail * 10 < size
            {
                found.insert(
                    format!("disk:{}", d["mount"].as_str().unwrap_or("?")),
                    Said::new(
                        format!("{} 剩余空间不足 10%", d["mount"].as_str().unwrap_or("?")),
                        format!(
                            "{} has less than 10% free",
                            d["mount"].as_str().unwrap_or("?")
                        ),
                    ),
                );
            }
        }
    }
    if host["clock_synced"] == false {
        found.insert(
            "clock".into(),
            Said::new(
                "系统时钟未与 NTP 同步",
                "The system clock is not synced with NTP",
            ),
        );
    }
    found
}

/// How often memory growth is judged.
const GROWTH_EVERY_MS: i64 = 10 * 60_000;

/// Run the watch forever on this thread.
pub fn watch(cfg: Config, raised: Arc<Mutex<Raised>>, notify: std::sync::mpsc::Sender<Message>) {
    let mut last_unreadable: Option<u64> = None;
    let mut recorder = crate::blackbox::Recorder::new(&cfg.state_dir);
    let (mut grown, mut grown_at): (Vec<(String, Said)>, i64) = (Vec::new(), 0);
    loop {
        // Whether each unit runs, from its cgroup: no process started.
        let units: Vec<(String, bool)> = cfg
            .manageable
            .iter()
            .map(|u| (u.clone(), crate::blackbox::Recorder::running(u)))
            .collect();
        let status = crate::control::ask(&cfg.control_dir, "status", "oq-agent watch", "");
        // The black box takes its sample from this same look: the trader
        // is asked once, and the host read once.
        let host = recorder
            .tick(system::now_ms(), &cfg.units, status.as_ref())
            .unwrap_or_else(|e| {
                eprintln!("oq-agent: black box not written: {e}");
                Value::Null
            });
        let on_purpose = raised
            .lock()
            .map(|r| r.trader_stopped_on_purpose)
            .unwrap_or(false);
        let found = assess(
            status.as_ref(),
            &units,
            &cfg.trader_unit,
            &host,
            on_purpose,
            last_unreadable,
        );
        let mut found = found;
        // Growth is judged over hours; asked every ten minutes, and the
        // last verdict stands in between.
        let now = system::now_ms();
        if now - grown_at >= GROWTH_EVERY_MS {
            grown = crate::blackbox::growing(&cfg.state_dir, &cfg.manageable, now);
            grown_at = now;
        }
        for (unit, msg) in &grown {
            found.insert(format!("mem:{unit}"), msg.clone());
        }
        if let Ok(s) = &status {
            last_unreadable = s["feed"]["unreadable"].as_u64();
        }
        let now = system::now_ms();
        if let Ok(mut r) = raised.lock() {
            r.silenced.retain(|_, until| *until > now);
            let cleared: Vec<String> = r
                .active
                .keys()
                .filter(|k| !found.contains_key(*k))
                .cloned()
                .collect();
            for key in cleared {
                if let Some((_, msg)) = r.active.remove(&key) {
                    r.note(now, &key, &msg, false);
                    recorder.event(now, "alert_cleared", &key, &msg);
                    if !r.quiet(&key, now) {
                        let _ = notify.send(Message {
                            title: "已恢复".into(),
                            body: msg.zh,
                            color: GREEN,
                        });
                    }
                }
            }
            for (key, msg) in found {
                if !r.active.contains_key(&key) {
                    r.note(now, &key, &msg, true);
                    recorder.event(now, "alert_raised", &key, &msg);
                    if !r.quiet(&key, now) {
                        let _ = notify.send(Message {
                            title: "告警".into(),
                            body: msg.zh.clone(),
                            color: RED,
                        });
                    }
                    r.active.insert(key, (now, msg));
                }
            }
        }
        std::thread::sleep(Duration::from_secs(30));
    }
}

/// The raised alerts as JSON.
#[must_use]
pub fn list(raised: &Raised) -> Value {
    json!(
        raised
            .active
            .iter()
            .map(
                |(k, (since, msg))| json!({"key": k, "since_ms": since, "message": msg.zh,
                "message_en": msg.en, "silenced_until_ms": raised.silenced.get(k)})
            )
            .collect::<Vec<_>>()
    )
}

/// What has been raised and cleared lately, newest first.
#[must_use]
pub fn history(raised: &Raised) -> Value {
    json!(
        raised
            .history
            .iter()
            .rev()
            .map(|(at, k, msg, up)| {
                json!({"at_ms": at, "key": k, "message": msg.zh, "message_en": msg.en, "raised": up})
            })
            .collect::<Vec<_>>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy() -> Value {
        json!({"halted": false, "journal_lost": null,
               "reconcile": {"agreed": true, "mismatches": 0},
               "feed": {"unreadable": 0}})
    }

    #[test]
    fn a_healthy_process_raises_nothing() {
        let units = vec![("trader.service".to_string(), true)];
        let host =
            json!({"disks": [{"mount": "/", "size": 100, "avail": 50}], "clock_synced": true});
        assert!(
            assess(
                Ok(&healthy()),
                &units,
                "trader.service",
                &host,
                false,
                Some(0)
            )
            .is_empty()
        );
    }

    #[test]
    fn each_condition_is_named() {
        let units = vec![
            ("trader.service".to_string(), true),
            ("oq-recon.service".to_string(), false),
        ];
        let mut s = healthy();
        s["halted"] = json!(true);
        s["halt_reason"] = json!("operator: test");
        s["reconcile"]["agreed"] = json!(false);
        s["feed"]["unreadable"] = json!(3);
        let host =
            json!({"disks": [{"mount": "/", "size": 100, "avail": 5}], "clock_synced": false});
        let found = assess(Ok(&s), &units, "trader.service", &host, false, Some(1));
        let keys: Vec<&str> = found.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "clock",
                "disk:/",
                "feed",
                "halted",
                "reconcile",
                "unit:oq-recon.service"
            ]
        );
        assert!(found["halted"].zh.contains("operator: test"));
        assert!(found["halted"].en.contains("operator: test"));
    }

    #[test]
    fn a_trader_stopped_on_purpose_is_not_an_alarm_but_a_silent_port_is() {
        let down = vec![("trader.service".to_string(), false)];
        let host = json!({});
        let gone = Said::new("端口不在了", "the socket is gone");
        assert!(assess(Err(&gone), &down, "trader.service", &host, true, None).is_empty());
        assert!(
            assess(Err(&gone), &down, "trader.service", &host, false, None)
                .contains_key("unit:trader.service")
        );
        let up = vec![("trader.service".to_string(), true)];
        let found = assess(Err(&gone), &up, "trader.service", &host, false, None);
        // The agent's reason for not reaching the port is the agent's
        // sentence, so each rendering carries its own.
        assert!(found["control"].zh.contains("端口不在了"), "{found:?}");
        assert!(
            found["control"].en.contains("the socket is gone"),
            "{found:?}"
        );
        assert!(!found["control"].en.contains("端口"), "{found:?}");
    }
}
