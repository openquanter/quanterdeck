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

/// Alerts currently raised, by key: since when, and the message.
#[derive(Debug, Default)]
pub struct Raised {
    pub active: BTreeMap<String, (i64, String)>,
    /// Set when an operator stopped the trader on purpose, so its absence
    /// is not alarming; cleared when it is started again.
    pub trader_stopped_on_purpose: bool,
}

/// What one look found: keys and messages of the conditions that hold.
#[must_use]
pub fn assess(
    status: Result<&Value, &str>,
    units: &[(String, bool)],
    trader_unit: &str,
    host: &Value,
    stopped_on_purpose: bool,
    last_unreadable: Option<u64>,
) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    let trader_up = units.iter().any(|(u, up)| u == trader_unit && *up);
    for (unit, up) in units {
        if !up && !(unit == trader_unit && stopped_on_purpose) {
            found.insert(format!("unit:{unit}"), format!("{unit} 没有在运行"));
        }
    }
    match status {
        Ok(s) => {
            if s["halted"] == true {
                let why = s["halt_reason"].as_str().unwrap_or("原因未记录");
                found.insert("halted".into(), format!("交易进程已停机：{why}"));
            }
            if let Some(j) = s["journal_lost"].as_str() {
                found.insert(
                    "journal".into(),
                    format!("交易日志无法写入，已停止开新单：{j}"),
                );
            }
            if s["reconcile"]["agreed"] == false {
                found.insert(
                    "reconcile".into(),
                    format!(
                        "最近一次持仓核对与交易所不一致（累计 {} 次）",
                        s["reconcile"]["mismatches"]
                    ),
                );
            }
            let unreadable = s["feed"]["unreadable"].as_u64().unwrap_or(0);
            if last_unreadable.is_some_and(|before| unreadable > before) {
                found.insert(
                    "feed".into(),
                    format!("行情出现读不出的消息：累计 {unreadable}"),
                );
            }
        }
        Err(why) if trader_up => {
            found.insert(
                "control".into(),
                format!("交易进程在运行，但控制口无应答：{why}"),
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
                    format!("{} 剩余空间不足 10%", d["mount"].as_str().unwrap_or("?")),
                );
            }
        }
    }
    if host["clock_synced"] == false {
        found.insert("clock".into(), "系统时钟未与 NTP 同步".into());
    }
    found
}

/// Run the watch forever on this thread.
pub fn watch(cfg: Config, raised: Arc<Mutex<Raised>>, notify: std::sync::mpsc::Sender<Message>) {
    let mut last_unreadable: Option<u64> = None;
    loop {
        let units: Vec<(String, bool)> = cfg
            .manageable
            .iter()
            .map(|u| (u.clone(), system::is_active(u)))
            .collect();
        let status = crate::control::ask(&cfg.control_dir, "status", "oq-agent watch", "");
        let host = system::host_health();
        let on_purpose = raised
            .lock()
            .map(|r| r.trader_stopped_on_purpose)
            .unwrap_or(false);
        let found = assess(
            status.as_ref().map_err(String::as_str),
            &units,
            &cfg.trader_unit,
            &host,
            on_purpose,
            last_unreadable,
        );
        if let Ok(s) = &status {
            last_unreadable = s["feed"]["unreadable"].as_u64();
        }
        let now = system::now_ms();
        if let Ok(mut r) = raised.lock() {
            let cleared: Vec<String> = r
                .active
                .keys()
                .filter(|k| !found.contains_key(*k))
                .cloned()
                .collect();
            for key in cleared {
                if let Some((_, msg)) = r.active.remove(&key) {
                    let _ = notify.send(Message {
                        title: "已恢复".into(),
                        body: msg,
                        color: GREEN,
                    });
                }
            }
            for (key, msg) in found {
                if let std::collections::btree_map::Entry::Vacant(slot) = r.active.entry(key) {
                    let _ = notify.send(Message {
                        title: "告警".into(),
                        body: msg.clone(),
                        color: RED,
                    });
                    slot.insert((now, msg));
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
            .map(|(k, (since, msg))| json!({"key": k, "since_ms": since, "message": msg}))
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
        let units = vec![("oqp-live.service".to_string(), true)];
        let host =
            json!({"disks": [{"mount": "/", "size": 100, "avail": 50}], "clock_synced": true});
        assert!(
            assess(
                Ok(&healthy()),
                &units,
                "oqp-live.service",
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
            ("oqp-live.service".to_string(), true),
            ("oq-recon.service".to_string(), false),
        ];
        let mut s = healthy();
        s["halted"] = json!(true);
        s["halt_reason"] = json!("operator: test");
        s["reconcile"]["agreed"] = json!(false);
        s["feed"]["unreadable"] = json!(3);
        let host =
            json!({"disks": [{"mount": "/", "size": 100, "avail": 5}], "clock_synced": false});
        let found = assess(Ok(&s), &units, "oqp-live.service", &host, false, Some(1));
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
        assert!(found["halted"].contains("operator: test"));
    }

    #[test]
    fn a_trader_stopped_on_purpose_is_not_an_alarm_but_a_silent_port_is() {
        let down = vec![("oqp-live.service".to_string(), false)];
        let host = json!({});
        assert!(assess(Err("gone"), &down, "oqp-live.service", &host, true, None).is_empty());
        assert!(
            assess(Err("gone"), &down, "oqp-live.service", &host, false, None)
                .contains_key("unit:oqp-live.service")
        );
        let up = vec![("oqp-live.service".to_string(), true)];
        assert!(
            assess(Err("timeout"), &up, "oqp-live.service", &host, false, None)
                .contains_key("control")
        );
    }
}
