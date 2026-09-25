//! The host's black box: what the system, each service and the trader
//! were doing, every thirty seconds, kept ninety days — so a review can
//! ask what things looked like at any moment and get an answer.
//!
//! # What is recorded
//!
//! * **host**: load, memory, swap, pressure (the kernel's own measure of
//!   time lost waiting for CPU, memory and IO), clock sync, and every five
//!   minutes the disks.
//! * **unit**, for each service including this agent: memory, its peak,
//!   CPU used, tasks, and whether it runs.
//! * **trader**: the trader's own status — halted and why, positions,
//!   resting orders, market-data counters, the last position check,
//!   whether its journal can write — from the same query the alert watch
//!   makes anyway.
//! * **event**: a service starting or stopping (with its exit status), the
//!   trader halting or resuming, its control port going quiet or coming
//!   back. Changes, recorded when they happen.
//!
//! The trader's decisions, fills and market observations are in its
//! journal, and its output in the systemd journal with timestamps; this
//! is the third part, the one neither of those has.
//!
//! # Cost
//!
//! A few file reads a sample: the cgroup files systemd keeps for each
//! unit and `/proc`. No process is started, except `systemctl` once when
//! a unit changes state, to learn why, and `df` every five minutes. The
//! trader is not asked anything it was not already asked. Lines are a few
//! hundred bytes; a day is a few megabytes.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

/// How often everything is sampled.
pub const EVERY: Duration = Duration::from_secs(30);
/// How long it is kept.
pub const KEEP_DAYS: i64 = 90;
/// How often the disks are measured, which starts `df`.
const DISK_EVERY_MS: i64 = 5 * 60_000;

fn dir(state: &Path) -> PathBuf {
    state.join("blackbox")
}

fn day(at_ms: i64) -> i64 {
    at_ms.div_euclid(86_400_000)
}

fn read_u64(path: &Path) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// One unit, from the cgroup systemd keeps for it. `None` when it has no
/// cgroup — it is not running.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Cgroup {
    memory: Option<u64>,
    peak: Option<u64>,
    cpu_usec: Option<u64>,
    tasks: Option<u64>,
}

fn cgroup(unit: &str) -> Option<Cgroup> {
    let d = Path::new("/sys/fs/cgroup/system.slice").join(unit);
    let tasks = read_u64(&d.join("pids.current"));
    if !d.is_dir() || tasks == Some(0) {
        return None;
    }
    let cpu_usec = std::fs::read_to_string(d.join("cpu.stat"))
        .ok()
        .and_then(|s| {
            s.lines().find_map(|l| {
                l.strip_prefix("usage_usec ")
                    .and_then(|v| v.trim().parse().ok())
            })
        });
    Some(Cgroup {
        memory: read_u64(&d.join("memory.current")),
        peak: read_u64(&d.join("memory.peak")),
        cpu_usec,
        tasks,
    })
}

fn pressure(kind: &str) -> Option<f64> {
    std::fs::read_to_string(format!("/proc/pressure/{kind}"))
        .ok()?
        .lines()
        .next()?
        .split_whitespace()
        .find_map(|f| f.strip_prefix("avg10=").and_then(|v| v.parse().ok()))
}

fn host(disks: &Value) -> Value {
    let loadavg = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    let load: Vec<f64> = loadavg
        .split_whitespace()
        .take(3)
        .filter_map(|v| v.parse().ok())
        .collect();
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let kb = |key: &str| {
        meminfo
            .lines()
            .find(|l| l.starts_with(key))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .map(|v| v * 1024)
    };
    json!({
        "load": load,
        "mem_total": kb("MemTotal:"), "mem_available": kb("MemAvailable:"),
        "swap_total": kb("SwapTotal:"), "swap_free": kb("SwapFree:"),
        "psi_cpu": pressure("cpu"), "psi_memory": pressure("memory"), "psi_io": pressure("io"),
        // systemd-timesyncd leaves this file while the clock is in sync.
        "clock_synced": Path::new("/run/systemd/timesync/synchronized").exists(),
        "disks": disks,
    })
}

/// The fields of the trader's status worth keeping, not the whole answer.
fn trader(status: &Value) -> Value {
    json!({
        "halted": status["halted"], "halt_reason": status["halt_reason"],
        "journal_lost": status["journal_lost"], "resting": status["resting"],
        "ticks": status["ticks"], "positions": status["positions"],
        "last_tick": status["last_tick"], "feed": status["feed"],
        "reconcile": status["reconcile"], "counters": status["counters"],
        "pid": status["pid"],
    })
}

/// Keeps the previous look, to record changes as events.
#[derive(Debug, Default)]
pub struct Recorder {
    state: PathBuf,
    units: BTreeMap<String, bool>,
    halted: Option<bool>,
    control_ok: Option<bool>,
    disks: Value,
    disks_at: i64,
}

impl Recorder {
    #[must_use]
    pub fn new(state: &Path) -> Self {
        Self {
            state: state.to_path_buf(),
            disks: Value::Null,
            ..Self::default()
        }
    }

    /// Whether a unit runs, from its cgroup: no process started.
    #[must_use]
    pub fn running(unit: &str) -> bool {
        cgroup(unit).is_some()
    }

    /// One sample of everything, and the events since the last. Returns
    /// the host sample, for the alert watch to judge without looking again.
    ///
    /// # Errors
    /// The write failed.
    pub fn tick(
        &mut self,
        now_ms: i64,
        units: &[String],
        status: Result<&Value, &str>,
    ) -> Result<Value, String> {
        if now_ms - self.disks_at >= DISK_EVERY_MS {
            self.disks = json!(crate::system::host_health()["disks"]);
            self.disks_at = now_ms;
        }
        let host_now = host(&self.disks);
        let mut lines = vec![json!({"at": now_ms, "k": "host", "host": host_now})];
        if self.units.is_empty() && self.control_ok.is_none() {
            // The first look since the agent started: whatever happened
            // between the last sample and this one was not recorded, and
            // the review says so rather than drawing a line across it.
            lines.push(
                json!({"at": now_ms, "k": "event", "what": "recording_started",
                "version": env!("CARGO_PKG_VERSION")}),
            );
        }
        for unit in units {
            let cg = cgroup(unit);
            let active = cg.is_some();
            lines.push(json!({
                "at": now_ms, "k": "unit", "unit": unit, "active": active,
                "mem": cg.as_ref().and_then(|c| c.memory), "peak": cg.as_ref().and_then(|c| c.peak),
                "cpu_usec": cg.as_ref().and_then(|c| c.cpu_usec), "tasks": cg.as_ref().and_then(|c| c.tasks),
            }));
            if self.units.get(unit).is_some_and(|was| *was != active)
                || (!self.units.contains_key(unit) && !active)
            {
                // Why, asked only now: the one process this starts.
                let s = crate::system::unit_state_with(
                    unit,
                    "Result,ExecMainStatus,ActiveState,SubState",
                );
                lines.push(json!({
                    "at": now_ms, "k": "event",
                    "what": if active { "unit_started" } else { "unit_stopped" },
                    "unit": unit, "result": s["Result"], "exit_status": s["ExecMainStatus"],
                    "state": format!("{}/{}", s["ActiveState"].as_str().unwrap_or(""), s["SubState"].as_str().unwrap_or("")),
                }));
            }
            self.units.insert(unit.clone(), active);
        }
        match status {
            Ok(s) => {
                lines.push(json!({"at": now_ms, "k": "trader", "trader": trader(s)}));
                let halted = s["halted"] == true;
                if self.halted.is_some_and(|was| was != halted) {
                    lines.push(json!({"at": now_ms, "k": "event",
                        "what": if halted { "trader_halted" } else { "trader_resumed" },
                        "reason": s["halt_reason"]}));
                }
                self.halted = Some(halted);
                if self.control_ok == Some(false) {
                    lines.push(json!({"at": now_ms, "k": "event", "what": "control_back"}));
                }
                self.control_ok = Some(true);
            }
            Err(why) => {
                if self.control_ok != Some(false) {
                    lines.push(
                        json!({"at": now_ms, "k": "event", "what": "control_lost", "reason": why}),
                    );
                }
                self.control_ok = Some(false);
            }
        }
        self.write(now_ms, &lines)?;
        Ok(host_now)
    }

    /// Record one event now: an alert raised or cleared.
    pub fn event(&self, now_ms: i64, what: &str, key: &str, message: &str) {
        let line =
            json!({"at": now_ms, "k": "event", "what": what, "key": key, "message": message});
        if let Err(e) = self.write(now_ms, &[line]) {
            eprintln!("oq-agent: black box event not written: {e}");
        }
    }

    fn write(&self, now_ms: i64, lines: &[Value]) -> Result<(), String> {
        let d = dir(&self.state);
        std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        let today = day(now_ms);
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(d.join(format!("{today}.jsonl")))
            .map_err(|e| e.to_string())?;
        let mut buf = String::new();
        for l in lines {
            buf.push_str(&l.to_string());
            buf.push('\n');
        }
        // One write per sample.
        f.write_all(buf.as_bytes()).map_err(|e| e.to_string())?;
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.filter_map(Result::ok) {
                let old = e
                    .file_name()
                    .to_string_lossy()
                    .strip_suffix(".jsonl")
                    .and_then(|n| n.parse::<i64>().ok())
                    .is_some_and(|n| n < today - KEEP_DAYS);
                if old {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        Ok(())
    }
}

/// Every line between two times, oldest first.
#[must_use]
pub fn read(state: &Path, from_ms: i64, to_ms: i64) -> Vec<Value> {
    let d = dir(state);
    let mut days: Vec<i64> = std::fs::read_dir(&d)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter_map(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .strip_suffix(".jsonl")
                        .and_then(|n| n.parse().ok())
                })
                .filter(|n| *n >= day(from_ms) && *n <= day(to_ms))
                .collect()
        })
        .unwrap_or_default();
    days.sort_unstable();
    let mut out = Vec::new();
    for n in days {
        let text = std::fs::read_to_string(d.join(format!("{n}.jsonl"))).unwrap_or_default();
        out.extend(
            text.lines()
                .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                .filter(|v| {
                    let at = v["at"].as_i64().unwrap_or(0);
                    at >= from_ms && at <= to_ms
                }),
        );
    }
    out
}

/// CPU between two samples of a unit, as a percentage of one core.
/// `None` across a restart, where the counter starts again.
fn cpu_pct(a: &Value, b: &Value) -> Option<f64> {
    let (ca, cb) = (a["cpu_usec"].as_u64()?, b["cpu_usec"].as_u64()?);
    let (ta, tb) = (a["at"].as_i64()?, b["at"].as_i64()?);
    #[allow(clippy::cast_precision_loss)]
    (cb >= ca && tb > ta).then(|| (cb - ca) as f64 / ((tb - ta) as f64 * 1000.0) * 100.0)
}

fn stats(mut values: Vec<f64>) -> Value {
    if values.is_empty() {
        return Value::Null;
    }
    values.sort_by(f64::total_cmp);
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let p95 = values[((values.len() - 1) as f64 * 0.95) as usize];
    #[allow(clippy::cast_precision_loss)]
    let avg = values.iter().sum::<f64>() / values.len() as f64;
    json!({"min": values[0], "max": values[values.len() - 1], "avg": avg, "p95": p95, "n": values.len()})
}

/// A window, for review: every event, and each series thinned to about
/// `points` samples with CPU worked out from consecutive samples.
#[must_use]
pub fn window(state: &Path, from_ms: i64, to_ms: i64, points: usize) -> Value {
    let lines = read(state, from_ms, to_ms);
    let mut units: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    let (mut hosts, mut traders, mut events) = (Vec::new(), Vec::new(), Vec::new());
    for l in &lines {
        match l["k"].as_str() {
            Some("unit") => units
                .entry(l["unit"].as_str().unwrap_or("").to_string())
                .or_default()
                .push(l),
            Some("host") => hosts.push(l),
            Some("trader") => traders.push(l),
            Some("event") => events.push(l.clone()),
            _ => {}
        }
    }
    let thin = |n: usize| (n / points.max(1)).max(1);
    let unit_views: BTreeMap<String, Value> = units
        .iter()
        .map(|(u, s)| {
            #[allow(clippy::cast_precision_loss)]
            let mem: Vec<f64> = s.iter().filter_map(|v| v["mem"].as_u64().map(|m| m as f64)).collect();
            let cpu: Vec<f64> = s.windows(2).filter_map(|w| cpu_pct(w[0], w[1])).collect();
            let curve: Vec<Value> = s
                .windows(2)
                .step_by(thin(s.len()))
                .map(|w| json!({"at": w[1]["at"], "mem": w[1]["mem"], "cpu": cpu_pct(w[0], w[1]), "tasks": w[1]["tasks"], "active": w[1]["active"]}))
                .collect();
            let down = s.iter().filter(|v| v["active"] != true).count();
            (u.clone(), json!({
                "samples": s.len(), "first_ms": s.first().map(|v| v["at"].clone()),
                "memory": stats(mem), "cpu_percent": stats(cpu),
                "samples_down": down, "curve": curve,
            }))
        })
        .collect();
    json!({
        "from_ms": from_ms, "to_ms": to_ms, "every_s": EVERY.as_secs(),
        "units": unit_views,
        "host": hosts.iter().step_by(thin(hosts.len())).map(|v| json!({"at": v["at"], "host": v["host"]})).collect::<Vec<_>>(),
        "trader": traders.iter().step_by(thin(traders.len())).map(|v| json!({"at": v["at"], "trader": v["trader"]})).collect::<Vec<_>>(),
        "events": events,
    })
}

/// What everything looked like at `at_ms`: the last sample of each kind
/// at or before it, within ten minutes.
#[must_use]
pub fn at(state: &Path, at_ms: i64) -> Value {
    let lines = read(state, at_ms - 10 * 60_000, at_ms);
    let last = |k: &str, unit: Option<&str>| {
        lines
            .iter()
            .rev()
            .find(|v| v["k"] == k && unit.is_none_or(|u| v["unit"] == u))
            .cloned()
    };
    let units: BTreeMap<String, Value> = lines
        .iter()
        .filter(|v| v["k"] == "unit")
        .filter_map(|v| v["unit"].as_str().map(str::to_string))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter_map(|u| last("unit", Some(&u)).map(|v| (u, v)))
        .collect();
    json!({"at_ms": at_ms, "host": last("host", None), "trader": last("trader", None), "units": units})
}

/// Summary over the last `hours` for the operations page.
#[must_use]
pub fn resources(state: &Path, now_ms: i64, hours: i64, units: &[String]) -> Value {
    let w = window(state, now_ms - hours * 3_600_000, now_ms, 240);
    json!(units.iter().map(|u| {
        let v = &w["units"][u];
        let changes = v["curve"].as_array().map_or(0, |c| c.windows(2).filter(|p| p[0]["active"] == true && p[1]["active"] != true).count());
        json!({
            "unit": u, "since_ms": w["from_ms"], "samples": v["samples"].as_u64().unwrap_or(0),
            "first_ms": v["first_ms"], "memory": v["memory"], "cpu_percent": v["cpu_percent"],
            "restarts": changes, "process_changes": changes,
            "samples_down": v["samples_down"].as_u64().unwrap_or(0),
            "curve": v["curve"],
        })
    }).collect::<Vec<_>>())
}

/// Whether a unit's memory has kept climbing: the last hour's average at
/// least twice its first hour in the last day, and 32 MiB more, without a
/// stop in between. A spike does not trip it; a leak does.
#[must_use]
pub fn growing(state: &Path, unit: &str, now_ms: i64) -> Option<String> {
    let lines = read(state, now_ms - 86_400_000, now_ms);
    let s: Vec<&Value> = lines
        .iter()
        .filter(|v| v["k"] == "unit" && v["unit"] == unit)
        .collect();
    // Only since it last started: a restart resets memory.
    let start = s
        .iter()
        .rposition(|v| v["active"] != true)
        .map_or(0, |i| i + 1);
    let run = &s[start..];
    let (first, last) = (run.first()?["at"].as_i64()?, run.last()?["at"].as_i64()?);
    if last - first < 6 * 3_600_000 {
        return None;
    }
    let avg = |from: i64, to: i64| {
        let v: Vec<u64> = run
            .iter()
            .filter(|x| x["at"].as_i64().is_some_and(|a| a >= from && a < to))
            .filter_map(|x| x["mem"].as_u64())
            .collect();
        #[allow(clippy::cast_precision_loss)]
        (!v.is_empty()).then(|| v.iter().sum::<u64>() as f64 / v.len() as f64)
    };
    let early = avg(first, first + 3_600_000)?;
    let late = avg(now_ms - 3_600_000, now_ms + 1)?;
    #[allow(clippy::cast_precision_loss)]
    (late >= early * 2.0 && late - early >= 32.0 * 1_048_576.0).then(|| {
        format!(
            "{unit} 的内存在持续上涨：{:.1} MiB → {:.1} MiB（{:.0} 小时内，期间没有重启）",
            early / 1_048_576.0,
            late / 1_048_576.0,
            (last - first) as f64 / 3.6e6
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_line(at: i64, mem: u64, cpu: u64, active: bool) -> Value {
        json!({"at": at, "k": "unit", "unit": "u", "active": active, "mem": mem, "cpu_usec": cpu, "tasks": 1})
    }

    fn write(state: &Path, lines: &[Value]) {
        let r = Recorder::new(state);
        r.write(lines[0]["at"].as_i64().expect("at"), lines)
            .expect("write");
    }

    #[test]
    fn a_window_has_extremes_cpu_and_every_event_and_a_moment_can_be_looked_at() {
        let d = tempfile::tempdir().expect("dir");
        let base = 1_790_000_000_000;
        write(
            d.path(),
            &[
                unit_line(base, 10 << 20, 0, true),
                json!({"at": base, "k": "trader", "trader": {"halted": false, "resting": 13}}),
                unit_line(base + 30_000, 12 << 20, 300_000, true),
                json!({"at": base + 30_000, "k": "event", "what": "trader_halted", "reason": "operator: test"}),
                unit_line(base + 60_000, 8 << 20, 0, true),
            ],
        );
        let w = window(d.path(), base - 1, base + 60_000, 100);
        assert_eq!(w["units"]["u"]["memory"]["min"], f64::from(8u32 << 20));
        assert_eq!(w["units"]["u"]["memory"]["max"], f64::from(12u32 << 20));
        // 0.3 s of CPU in 30 s is 1% of a core; the counter going back is a
        // restart and gives no value.
        assert_eq!(w["units"]["u"]["cpu_percent"]["n"], 1);
        assert_eq!(w["events"][0]["what"], "trader_halted");
        let moment = at(d.path(), base + 45_000);
        assert_eq!(moment["trader"]["trader"]["resting"], 13);
        assert_eq!(moment["units"]["u"]["mem"], 12 << 20);
    }

    #[test]
    fn steady_memory_is_not_growth_a_leak_is_and_a_restart_resets_it() {
        let hour = 3_600_000;
        let base = 1_790_000_000_000;
        let d = tempfile::tempdir().expect("dir");
        write(
            d.path(),
            &(0..=8)
                .map(|h| unit_line(base + h * hour, 10 << 20, 0, true))
                .collect::<Vec<_>>(),
        );
        assert!(growing(d.path(), "u", base + 8 * hour).is_none());
        let d = tempfile::tempdir().expect("dir");
        let leak: Vec<Value> = (0..=8)
            .map(|h| unit_line(base + h * hour, (10 + 10 * h as u64) << 20, 0, true))
            .collect();
        write(d.path(), &leak);
        assert!(
            growing(d.path(), "u", base + 8 * hour)
                .expect("growth")
                .contains("持续上涨")
        );
        let d = tempfile::tempdir().expect("dir");
        let mut restarted = leak.clone();
        restarted[7] = unit_line(base + 7 * hour, 0, 0, false);
        write(d.path(), &restarted);
        assert!(
            growing(d.path(), "u", base + 8 * hour).is_none(),
            "a restart starts it again"
        );
    }
}
