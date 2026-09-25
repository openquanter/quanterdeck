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

use oq_deck_core::lang::Said;
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
        "pid": status["pid"], "pnl": status["pnl"],
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
        status: Result<&Value, &Said>,
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
                    // The reason is the agent's own sentence, so the
                    // review reads it in the language it is being read in.
                    lines.push(json!({"at": now_ms, "k": "event", "what": "control_lost",
                        "reason": why.zh, "reason_en": why.en}));
                }
                self.control_ok = Some(false);
            }
        }
        self.write(now_ms, &lines)?;
        Ok(host_now)
    }

    /// Record one event now: an alert raised or cleared.
    pub fn event(&self, now_ms: i64, what: &str, key: &str, message: &crate::alerts::Said) {
        let line = json!({"at": now_ms, "k": "event", "what": what, "key": key,
            "message": message.zh, "message_en": message.en});
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

/// Every line between two times, oldest first, handed to `f` one at a
/// time. Nothing holds more than the line in hand: a week of samples is
/// tens of megabytes of text, and the host this runs on is trading.
///
/// With `needle`, a line not containing it is skipped before it is
/// parsed — most of the cost of a scan is parsing lines nobody asked for.
fn scan(state: &Path, from_ms: i64, to_ms: i64, needle: Option<&str>, mut f: impl FnMut(Value)) {
    use std::io::BufRead;
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
    for n in days {
        let Ok(file) = std::fs::File::open(d.join(format!("{n}.jsonl"))) else {
            continue;
        };
        for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
            if needle.is_some_and(|n| !line.contains(n)) {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let at = v["at"].as_i64().unwrap_or(0);
            if at >= from_ms && at <= to_ms {
                f(v);
            }
        }
    }
}

/// Every line between two times, oldest first. For short spans only.
#[must_use]
pub fn read(state: &Path, from_ms: i64, to_ms: i64) -> Vec<Value> {
    let mut out = Vec::new();
    scan(state, from_ms, to_ms, None, |v| out.push(v));
    out
}

/// One sample of a unit, without the JSON around it.
#[derive(Debug, Clone, Copy)]
struct UnitSample {
    at: i64,
    mem: Option<u64>,
    cpu_usec: Option<u64>,
    tasks: Option<u64>,
    active: bool,
}

impl UnitSample {
    fn of(v: &Value) -> Option<Self> {
        Some(Self {
            at: v["at"].as_i64()?,
            mem: v["mem"].as_u64(),
            cpu_usec: v["cpu_usec"].as_u64(),
            tasks: v["tasks"].as_u64(),
            active: v["active"] == true,
        })
    }
}

/// CPU between two samples of a unit, as a percentage of one core.
/// `None` across a restart, where the counter starts again.
fn cpu_pct(a: &UnitSample, b: &UnitSample) -> Option<f64> {
    let (ca, cb) = (a.cpu_usec?, b.cpu_usec?);
    #[allow(clippy::cast_precision_loss)]
    (cb >= ca && b.at > a.at).then(|| (cb - ca) as f64 / ((b.at - a.at) as f64 * 1000.0) * 100.0)
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
///
/// Read in one pass. Host and trader samples are thinned as they are
/// read, at the stride the recording interval implies, so a week costs
/// about as much memory as an hour; unit samples are kept whole but
/// compact, because their statistics need every one.
#[must_use]
pub fn window(state: &Path, from_ms: i64, to_ms: i64, points: usize) -> Value {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let expected = ((to_ms - from_ms).max(0) as u64 / (EVERY.as_millis() as u64).max(1)) as usize;
    let stride = (expected / points.max(1)).max(1);
    let mut units: BTreeMap<String, Vec<UnitSample>> = BTreeMap::new();
    let (mut hosts, mut traders, mut events) = (Vec::new(), Vec::new(), Vec::new());
    let (mut host_n, mut trader_n) = (0usize, 0usize);
    scan(state, from_ms, to_ms, None, |l| match l["k"].as_str() {
        Some("unit") => {
            if let (Some(u), Some(sample)) = (l["unit"].as_str(), UnitSample::of(&l)) {
                units.entry(u.to_string()).or_default().push(sample);
            }
        }
        Some("host") => {
            if host_n % stride == 0 {
                hosts.push(json!({"at": l["at"], "host": l["host"]}));
            }
            host_n += 1;
        }
        Some("trader") => {
            if trader_n % stride == 0 {
                traders.push(json!({"at": l["at"], "trader": l["trader"]}));
            }
            trader_n += 1;
        }
        Some("event") => events.push(l),
        _ => {}
    });
    let unit_views: BTreeMap<String, Value> = units
        .iter()
        .map(|(u, s)| {
            #[allow(clippy::cast_precision_loss)]
            let mem: Vec<f64> = s.iter().filter_map(|v| v.mem.map(|m| m as f64)).collect();
            let cpu: Vec<f64> = s.windows(2).filter_map(|w| cpu_pct(&w[0], &w[1])).collect();
            let curve: Vec<Value> = s
                .windows(2)
                .step_by((s.len() / points.max(1)).max(1))
                .map(|w| json!({"at": w[1].at, "mem": w[1].mem, "cpu": cpu_pct(&w[0], &w[1]), "tasks": w[1].tasks, "active": w[1].active}))
                .collect();
            let down = s.iter().filter(|v| !v.active).count();
            (u.clone(), json!({
                "samples": s.len(), "first_ms": s.first().map(|v| v.at),
                "memory": stats(mem), "cpu_percent": stats(cpu),
                "samples_down": down, "curve": curve,
            }))
        })
        .collect();
    json!({
        "from_ms": from_ms, "to_ms": to_ms, "every_s": EVERY.as_secs(),
        "units": unit_views, "host": hosts, "trader": traders, "events": events,
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

/// Which units' memory has kept climbing: the last hour's average at
/// least twice the first hour's in the last day, and 32 MiB more, without
/// a stop in between. A spike does not trip it; a leak does.
///
/// One pass over the day for every unit, reading only unit lines. It is
/// asked every few minutes, not every sample: a leak measured in hours
/// does not need a verdict every 30 seconds, and a day of samples is not
/// free to read.
#[must_use]
pub fn growing(state: &Path, units: &[String], now_ms: i64) -> Vec<(String, crate::alerts::Said)> {
    let mut samples: BTreeMap<&str, Vec<UnitSample>> =
        units.iter().map(|u| (u.as_str(), Vec::new())).collect();
    scan(
        state,
        now_ms - 86_400_000,
        now_ms,
        Some(r#""k":"unit""#),
        |v| {
            if let (Some(u), Some(sample)) = (v["unit"].as_str(), UnitSample::of(&v))
                && let Some(list) = samples.get_mut(u)
            {
                list.push(sample);
            }
        },
    );
    samples
        .into_iter()
        .filter_map(|(unit, s)| grown(unit, &s, now_ms).map(|m| (unit.to_string(), m)))
        .collect()
}

fn grown(unit: &str, s: &[UnitSample], now_ms: i64) -> Option<crate::alerts::Said> {
    // Only since it last started: a restart resets memory.
    let start = s.iter().rposition(|v| !v.active).map_or(0, |i| i + 1);
    let run = &s[start..];
    let (first, last) = (run.first()?.at, run.last()?.at);
    if last - first < 6 * 3_600_000 {
        return None;
    }
    let avg = |from: i64, to: i64| {
        let v: Vec<u64> = run
            .iter()
            .filter(|x| x.at >= from && x.at < to)
            .filter_map(|x| x.mem)
            .collect();
        #[allow(clippy::cast_precision_loss)]
        (!v.is_empty()).then(|| v.iter().sum::<u64>() as f64 / v.len() as f64)
    };
    let early = avg(first, first + 3_600_000)?;
    let late = avg(now_ms - 3_600_000, now_ms + 1)?;
    #[allow(clippy::cast_precision_loss)]
    (late >= early * 2.0 && late - early >= 32.0 * 1_048_576.0).then(|| {
        let (from, to, hours) = (
            early / 1_048_576.0,
            late / 1_048_576.0,
            (last - first) as f64 / 3.6e6,
        );
        crate::alerts::Said::new(
            format!(
                "{unit} 的内存在持续上涨：{from:.1} MiB → {to:.1} MiB（{hours:.0} 小时内，期间没有重启）"
            ),
            format!(
                "{unit}'s memory keeps rising: {from:.1} MiB → {to:.1} MiB over {hours:.0} h, with no restart"
            ),
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
        assert!(growing(d.path(), &["u".to_string()], base + 8 * hour).is_empty());
        let d = tempfile::tempdir().expect("dir");
        let leak: Vec<Value> = (0..=8)
            .map(|h| unit_line(base + h * hour, (10 + 10 * h as u64) << 20, 0, true))
            .collect();
        write(d.path(), &leak);
        let found = growing(d.path(), &["u".to_string()], base + 8 * hour);
        let said = &found.first().expect("growth").1;
        assert!(said.zh.contains("持续上涨"), "{}", said.zh);
        assert!(said.en.contains("keeps rising"), "{}", said.en);
        let d = tempfile::tempdir().expect("dir");
        let mut restarted = leak.clone();
        restarted[7] = unit_line(base + 7 * hour, 0, 0, false);
        write(d.path(), &restarted);
        assert!(
            growing(d.path(), &["u".to_string()], base + 8 * hour).is_empty(),
            "a restart starts it again"
        );
    }

    /// A week recorded at the real rate and shape, read back the ways the
    /// console reads it. Run by hand to see the cost:
    /// `cargo test --release -p oq-agent week_of_samples -- --ignored --nocapture`.
    #[test]
    #[ignore = "a benchmark, not a check"]
    fn week_of_samples() {
        let d = tempfile::tempdir().expect("dir");
        let base: i64 = 1_790_000_000_000;
        let units = [
            "trader.service",
            "oq-recon.service",
            "oq-deck.service",
            "oq-agent.service",
            "caddy.service",
        ];
        let dirp = dir(d.path());
        std::fs::create_dir_all(&dirp).expect("dir");
        let mut files: BTreeMap<i64, std::io::BufWriter<std::fs::File>> = BTreeMap::new();
        let every = 30_000;
        let n = 7 * 86_400_000 / every;
        for k in 0..n {
            let at = base + k * every;
            let f = files.entry(day(at)).or_insert_with(|| {
                std::io::BufWriter::new(
                    std::fs::File::create(dirp.join(format!("{}.jsonl", day(at)))).expect("file"),
                )
            });
            use std::io::Write;
            let mut line = |v: Value| writeln!(f, "{v}").expect("write");
            line(
                json!({"at": at, "k": "host", "host": {"clock_synced": true, "disks": [{"avail": 418_632_335_360_u64, "mount": "/", "size": 501_809_635_328_u64, "used": 57_611_497_472_u64}], "load": [0.03, 0.02, 0.0], "mem_available": 14_926_151_680_u64, "mem_total": 16_542_121_984_u64, "psi_cpu": 0.0, "psi_io": 0.0, "psi_memory": 0.0, "swap_free": 4_292_681_728_u64, "swap_total": 4_294_963_200_u64}}),
            );
            for u in units {
                line(
                    json!({"active": true, "at": at, "cpu_usec": k * 1000, "k": "unit", "mem": 5_603_328 + (k % 100) * 1000, "peak": 5_873_664, "tasks": 1, "unit": u}),
                );
            }
            line(
                json!({"at": at, "k": "trader", "trader": {"counters": {"disconnects": 0, "fills": 0, "foreign_orders": 0, "sent": 0, "unbookable_reports": 0}, "feed": {"depth": k, "out_of_order": k, "quiet": 1, "resyncs": 0, "snapshots": 3, "trades": k, "unreadable": 0}, "halt_reason": null, "halted": false, "journal_lost": null, "last_tick": {"exch_ns": at * 1_000_000, "last": 8_443_710, "local_ns": at * 1_000_000}, "pid": 1553, "positions": [{"amount": "0.002", "side": "LONG"}, {"amount": "-0.008", "side": "SHORT"}], "reconcile": {"agreed": true, "at_ns": at * 1_000_000, "mismatches": 0, "unread": 0}, "resting": 14, "ticks": k * 60, "pnl": {"equity": "5006.5", "fees": "0", "funding": "0", "net": "1.4", "realized": "1.4", "since_ms": base}}}),
            );
        }
        drop(files);
        let bytes: u64 = std::fs::read_dir(&dirp)
            .expect("dir")
            .filter_map(Result::ok)
            .map(|e| e.metadata().map_or(0, |m| m.len()))
            .sum();
        let end = base + 7 * 86_400_000;
        let t = std::time::Instant::now();
        let w = window(d.path(), base, end, 400);
        let week = t.elapsed();
        let t = std::time::Instant::now();
        let g = growing(d.path(), &units.map(String::from), end);
        let grow = t.elapsed();
        println!(
            "week: {} MiB on disk; window(7d) {:?}, {} host points; growing(24h, 5 units) {:?}, {} found",
            bytes >> 20,
            week,
            w["host"].as_array().map_or(0, Vec::len),
            grow,
            g.len()
        );
    }
}
