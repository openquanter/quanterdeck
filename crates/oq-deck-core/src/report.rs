//! Reports: what the trading stack did over one period, kept as a file.
//!
//! The pages answer "what is happening"; a report answers "what happened
//! last day" a month later, when the black box has been read by nobody
//! and the page shows today. It is built from inputs fetched elsewhere —
//! the agent's black box window and a reconciliation — by a pure
//! function, stored as JSON, and rendered to one self-contained HTML
//! document on request, in the reader's language.
//!
//! The rule it is built by is the console's: a section that could not be
//! filled is listed in `unavailable` with the reason, and its figures are
//! absent — never zeros. A P&L of zero and a P&L nobody measured are
//! opposite facts.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::lang::{Lang, Said};
use crate::live::{CannotTell, Reconciliation, Verdict};

/// The version of the stored shape. Fields are added, never repurposed.
pub const FORMAT: u32 = 1;
/// The most points the equity curve keeps.
pub const EQUITY_POINTS: usize = 400;
/// The most events a report keeps; the most recent ones.
pub const MAX_EVENTS: usize = 500;
/// The most reconciliation differences a report repeats.
pub const MAX_DIFFERENCES: usize = 20;
/// How long reports are kept.
pub const KEEP_DAYS: i64 = 90;
/// How long after a period ends its scheduled report is due: the agent
/// samples every 30 s, so the period's last sample lands after its end.
pub const GRACE_MS: i64 = 2 * 60_000;

const HOUR_MS: i64 = 3_600_000;

/// One report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportData {
    pub format: u32,
    pub period_from_ms: i64,
    pub period_to_ms: i64,
    pub generated_at_ms: i64,
    /// Whether the schedule made it or an operator asked for it.
    pub trigger: Trigger,
    /// The trader over the period. `None` when it could not be read;
    /// `unavailable` says why.
    pub trader: Option<TraderSummary>,
    /// The newest journal against the newest venue reading, as of when
    /// the report was generated.
    pub reconciliation: Option<ReconSummary>,
    /// What changed during the period. `None` when the black box could
    /// not be read; empty when it was read and nothing happened.
    pub events: Option<Vec<Event>>,
    /// Events left out, earliest first, beyond [`MAX_EVENTS`].
    #[serde(default)]
    pub events_omitted: usize,
    pub host: Option<HostSummary>,
    /// Every section that could not be filled, and why.
    pub unavailable: Vec<Gap>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    Scheduled,
    Manual,
}

/// A part of the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Trader,
    Pnl,
    Reconciliation,
    Events,
    Host,
}

impl Section {
    fn name(self) -> Said {
        match self {
            Self::Trader => Said::new("交易进程", "Trader"),
            Self::Pnl => Said::new("盈亏", "P&L"),
            Self::Reconciliation => Said::new("对账", "Reconciliation"),
            Self::Events => Said::new("事件", "Events"),
            Self::Host => Said::new("主机", "Host"),
        }
    }
}

/// A section that could not be filled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gap {
    pub section: Section,
    pub reason: Said,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraderSummary {
    /// Trader samples seen in the period (after the agent's thinning).
    pub samples: usize,
    pub first_at_ms: i64,
    /// The sample the end state below is from.
    pub last_at_ms: i64,
    pub pnl: Option<PnlSummary>,
    /// At the last sample. `None` when the sample did not carry them.
    pub positions: Option<Vec<Position>>,
    pub resting: Option<u64>,
    pub halted: Option<bool>,
    pub halt_reason: Option<String>,
}

/// The P&L over the period, summed across trader restarts.
///
/// The trader reports P&L since its own start, so each run's
/// contribution is its last figure minus its first in the period — or
/// minus nothing, when the run began inside the period. A field the
/// trader did not measure in any run is `None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PnlSummary {
    pub equity_first: Option<f64>,
    pub equity_last: Option<f64>,
    pub net: Option<f64>,
    pub realized: Option<f64>,
    pub fees: Option<f64>,
    pub funding: Option<f64>,
    /// Trader runs seen in the period; more than one means it restarted.
    pub runs: usize,
    /// `(at_ms, equity)`, at most [`EQUITY_POINTS`].
    pub equity: Vec<(i64, f64)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub side: String,
    pub amount: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconSummary {
    pub journal: String,
    pub verdict: Verdict,
    pub cannot_tell: Option<CannotTell>,
    /// The first [`MAX_DIFFERENCES`] differences.
    pub differences: Vec<String>,
    pub differences_total: usize,
    pub undecodable: u64,
    pub record_read_at_ms: i64,
    /// How old the venue reading was when the report was generated.
    pub record_age_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub at_ms: i64,
    /// The black box's own name for it, e.g. `alert_raised`.
    pub what: String,
    /// The unit or alert it is about.
    pub subject: Option<String>,
    pub detail: Option<Said>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostSummary {
    pub samples: usize,
    /// The highest one-minute load average.
    pub peak_load: Option<f64>,
    pub min_mem_available: Option<u64>,
    pub mem_total: Option<u64>,
    /// The least free space each disk had. `None` when no sample in the
    /// period carried a disk measurement.
    pub disks: Option<Vec<DiskLow>>,
    /// Samples taken while the clock was not synchronised.
    pub clock_unsynced_samples: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiskLow {
    pub mount: String,
    pub min_free_pct: f64,
}

/// What a report is built from, fetched by the caller.
#[derive(Debug)]
pub struct Inputs {
    pub from_ms: i64,
    pub to_ms: i64,
    pub generated_at_ms: i64,
    pub trigger: Trigger,
    /// The agent's black box window (`Op::Blackbox`), or why there is none.
    pub blackbox: Result<Value, Said>,
    /// The newest reconciliation, or why there is none.
    pub reconciliation: Result<Reconciliation, Said>,
}

/// Build a report from what was fetched. Pure: no clock, no files.
#[must_use]
pub fn build(inputs: Inputs) -> ReportData {
    let Inputs {
        from_ms,
        to_ms,
        generated_at_ms,
        trigger,
        blackbox,
        reconciliation,
    } = inputs;
    let mut unavailable = Vec::new();
    let mut gap = |section, reason| unavailable.push(Gap { section, reason });

    let in_period = |v: &Value| v["at"].as_i64().filter(|at| (from_ms..=to_ms).contains(at));

    let (trader, events, events_omitted, host) = match &blackbox {
        Err(why) => {
            for s in [Section::Trader, Section::Events, Section::Host] {
                gap(s, why.clone());
            }
            (None, None, 0, None)
        }
        Ok(window) => {
            let samples = |key: &str| -> Vec<(i64, &Value)> {
                window[key]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|s| in_period(s).map(|at| (at, &s[key])))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let traders = samples("trader");
            let trader = if traders.is_empty() {
                gap(
                    Section::Trader,
                    Said::new(
                        "黑匣子在这段时间里没有交易进程的样本：交易进程没有运行，或主机代理问不到它",
                        "The black box has no trader samples in this period: the trader was not \
                         running, or the host agent could not reach it",
                    ),
                );
                None
            } else {
                let t = trader_summary(&traders, from_ms);
                if t.pnl.is_none() {
                    gap(
                        Section::Pnl,
                        Said::new(
                            "交易进程的样本里没有盈亏（较早的版本不报告它）",
                            "The trader's samples carry no P&L (builds before it was reported)",
                        ),
                    );
                }
                Some(t)
            };
            let (events, omitted) = events_of(window, &in_period);
            let hosts = samples("host");
            let host = if hosts.is_empty() {
                gap(
                    Section::Host,
                    Said::new(
                        "黑匣子在这段时间里没有主机样本",
                        "The black box has no host samples in this period",
                    ),
                );
                None
            } else {
                Some(host_summary(&hosts))
            };
            (trader, Some(events), omitted, host)
        }
    };

    let reconciliation = match reconciliation {
        Ok(r) => Some(ReconSummary {
            differences_total: r.differences.len(),
            differences: r.differences.into_iter().take(MAX_DIFFERENCES).collect(),
            journal: r.journal,
            verdict: r.verdict,
            cannot_tell: r.cannot_tell,
            undecodable: r.undecodable,
            record_read_at_ms: r.venue.read_at_ms,
            record_age_ms: generated_at_ms - r.venue.read_at_ms,
        }),
        Err(why) => {
            gap(Section::Reconciliation, why);
            None
        }
    };

    ReportData {
        format: FORMAT,
        period_from_ms: from_ms,
        period_to_ms: to_ms,
        generated_at_ms,
        trigger,
        trader,
        reconciliation,
        events,
        events_omitted,
        host,
        unavailable,
    }
}

/// A number the trader wrote as a decimal string, or as a number.
fn num(v: &Value) -> Option<f64> {
    let n = match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }?;
    n.is_finite().then_some(n)
}

fn trader_summary(samples: &[(i64, &Value)], from_ms: i64) -> TraderSummary {
    let (first_at, _) = samples[0];
    let (last_at, last) = samples[samples.len() - 1];
    let positions = last["positions"].as_array().map(|ps| {
        ps.iter()
            .map(|p| {
                let text = |v: &Value| match v {
                    Value::String(s) => s.clone(),
                    Value::Null => String::new(),
                    other => other.to_string(),
                };
                Position {
                    side: text(&p["side"]),
                    amount: text(&p["amount"]),
                }
            })
            .collect()
    });
    TraderSummary {
        samples: samples.len(),
        first_at_ms: first_at,
        last_at_ms: last_at,
        pnl: pnl_summary(samples, from_ms),
        positions,
        resting: last["resting"].as_u64(),
        halted: last["halted"].as_bool(),
        halt_reason: last["halt_reason"].as_str().map(str::to_owned),
    }
}

fn pnl_summary(samples: &[(i64, &Value)], from_ms: i64) -> Option<PnlSummary> {
    let pnl: Vec<(i64, &Value)> = samples
        .iter()
        .filter(|(_, t)| t["pnl"].is_object())
        .map(|(at, t)| (*at, &t["pnl"]))
        .collect();
    if pnl.is_empty() {
        return None;
    }
    // Consecutive samples of one run share its start time.
    let mut runs: Vec<&[(i64, &Value)]> = Vec::new();
    let mut start = 0;
    for i in 1..=pnl.len() {
        if i == pnl.len() || pnl[i].1["since_ms"] != pnl[start].1["since_ms"] {
            runs.push(&pnl[start..i]);
            start = i;
        }
    }
    let delta = |field: &str| -> Option<f64> {
        let mut total = 0.0;
        for run in &runs {
            let last = num(&run[run.len() - 1].1[field])?;
            let began_inside = run[0].1["since_ms"].as_i64().is_some_and(|s| s >= from_ms);
            let base = if began_inside {
                0.0
            } else {
                num(&run[0].1[field])?
            };
            total += last - base;
        }
        Some(total)
    };
    let all: Vec<(i64, f64)> = pnl
        .iter()
        .filter_map(|(at, p)| num(&p["equity"]).map(|e| (*at, e)))
        .collect();
    let equity = thin(&all, EQUITY_POINTS);
    Some(PnlSummary {
        equity_first: num(&pnl[0].1["equity"]),
        equity_last: num(&pnl[pnl.len() - 1].1["equity"]),
        net: delta("net"),
        realized: delta("realized"),
        fees: delta("fees"),
        funding: delta("funding"),
        runs: runs.len(),
        equity,
    })
}

/// At most `n` points, evenly spaced, keeping the first and the last.
fn thin<T: Copy>(all: &[T], n: usize) -> Vec<T> {
    if all.len() <= n || n < 2 {
        return all.to_vec();
    }
    (0..n).map(|i| all[i * (all.len() - 1) / (n - 1)]).collect()
}

fn events_of(window: &Value, in_period: &impl Fn(&Value) -> Option<i64>) -> (Vec<Event>, usize) {
    const KEPT: [&str; 9] = [
        "alert_raised",
        "alert_cleared",
        "trader_halted",
        "trader_resumed",
        "unit_started",
        "unit_stopped",
        "control_lost",
        "control_back",
        "recording_started",
    ];
    let str_of = |v: &Value| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    let pair = |zh: &Value, en: &Value| {
        str_of(zh).map(|zh| {
            let en = str_of(en).unwrap_or_else(|| zh.clone());
            Said::new(zh, en)
        })
    };
    let mut all: Vec<Event> = window["events"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or_default()
        .iter()
        .filter_map(|e| {
            let at_ms = in_period(e)?;
            let what = e["what"].as_str()?;
            if !KEPT.contains(&what) {
                return None;
            }
            let detail = pair(&e["message"], &e["message_en"])
                .or_else(|| pair(&e["reason"], &e["reason_en"]))
                .or_else(|| {
                    let result = str_of(&e["result"])?;
                    let exit = str_of(&e["exit_status"]).unwrap_or_default();
                    Some(Said::same(format!("result={result} exit={exit}")))
                });
            Some(Event {
                at_ms,
                what: what.to_owned(),
                subject: str_of(&e["unit"]).or_else(|| str_of(&e["key"])),
                detail,
            })
        })
        .collect();
    all.sort_by_key(|e| e.at_ms);
    let omitted = all.len().saturating_sub(MAX_EVENTS);
    (all.split_off(omitted), omitted)
}

fn host_summary(samples: &[(i64, &Value)]) -> HostSummary {
    let mut peak_load: Option<f64> = None;
    let mut min_mem: Option<u64> = None;
    let mut mem_total = None;
    let mut disks: Option<Vec<DiskLow>> = None;
    let mut unsynced = 0;
    for (_, h) in samples {
        if let Some(l) = h["load"].get(0).and_then(Value::as_f64) {
            peak_load = Some(peak_load.map_or(l, |p| p.max(l)));
        }
        if let Some(m) = h["mem_available"].as_u64() {
            min_mem = Some(min_mem.map_or(m, |p| p.min(m)));
        }
        if let Some(t) = h["mem_total"].as_u64() {
            mem_total = Some(t);
        }
        if h["clock_synced"] == false {
            unsynced += 1;
        }
        for d in h["disks"].as_array().map(Vec::as_slice).unwrap_or_default() {
            let (Some(mount), Some(size), Some(avail)) = (
                d["mount"].as_str(),
                d["size"].as_u64().filter(|s| *s > 0),
                d["avail"].as_u64(),
            ) else {
                continue;
            };
            #[allow(clippy::cast_precision_loss)]
            let pct = avail as f64 / size as f64 * 100.0;
            let list = disks.get_or_insert_with(Vec::new);
            match list.iter_mut().find(|x| x.mount == mount) {
                Some(x) => x.min_free_pct = x.min_free_pct.min(pct),
                None => list.push(DiskLow {
                    mount: mount.to_owned(),
                    min_free_pct: pct,
                }),
            }
        }
    }
    HostSummary {
        samples: samples.len(),
        peak_load,
        min_mem_available: min_mem,
        mem_total,
        disks,
        clock_unsynced_samples: unsynced,
    }
}

// -- the schedule -----------------------------------------------------------

/// The newest period of `hours` that has ended, aligned to the epoch
/// (so 24 hours is a UTC day), as `(from_ms, to_ms)`.
///
/// "Ended" counts [`GRACE_MS`] past the boundary, so the period's last
/// samples have been written by the time it is reported.
#[must_use]
pub fn last_period(now_ms: i64, hours: u64) -> (i64, i64) {
    let len = period_ms(hours);
    let to = (now_ms - GRACE_MS).div_euclid(len) * len;
    (to - len, to)
}

/// When the next period's report falls due.
#[must_use]
pub fn next_due(now_ms: i64, hours: u64) -> i64 {
    let (_, to) = last_period(now_ms, hours);
    to + period_ms(hours) + GRACE_MS
}

/// The period the schedule should report now, if its report is missing.
#[must_use]
pub fn due(now_ms: i64, hours: u64, exists: impl Fn(i64, i64) -> bool) -> Option<(i64, i64)> {
    let (from, to) = last_period(now_ms, hours);
    (!exists(from, to)).then_some((from, to))
}

fn period_ms(hours: u64) -> i64 {
    i64::try_from(hours.max(1)).unwrap_or(168) * HOUR_MS
}

// -- storage ----------------------------------------------------------------

/// A report's id: its period, as two millisecond times.
#[must_use]
pub fn id_of(from_ms: i64, to_ms: i64) -> String {
    format!("{from_ms}-{to_ms}")
}

/// The period an id names, if it is one: digits, a dash, digits.
#[must_use]
pub fn parse_id(id: &str) -> Option<(i64, i64)> {
    let (a, b) = id.split_once('-')?;
    let digits = |s: &str| !s.is_empty() && s.len() <= 19 && s.bytes().all(|c| c.is_ascii_digit());
    if !digits(a) || !digits(b) {
        return None;
    }
    Some((a.parse().ok()?, b.parse().ok()?))
}

/// One report in a listing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    pub id: String,
    pub period_from_ms: i64,
    pub period_to_ms: i64,
    /// `None` when the file would not read; `error` says why.
    pub generated_at_ms: Option<i64>,
    pub trigger: Option<Trigger>,
    /// `None` when reconciliation was unavailable: not "agree".
    pub verdict: Option<Verdict>,
    pub net: Option<f64>,
    pub unavailable: Vec<Section>,
    pub error: Option<String>,
}

impl Entry {
    #[must_use]
    pub fn of(data: &ReportData) -> Self {
        Self {
            id: id_of(data.period_from_ms, data.period_to_ms),
            period_from_ms: data.period_from_ms,
            period_to_ms: data.period_to_ms,
            generated_at_ms: Some(data.generated_at_ms),
            trigger: Some(data.trigger),
            verdict: data.reconciliation.as_ref().map(|r| r.verdict),
            net: data
                .trader
                .as_ref()
                .and_then(|t| t.pnl.as_ref())
                .and_then(|p| p.net),
            unavailable: data.unavailable.iter().map(|g| g.section).collect(),
            error: None,
        }
    }
}

/// The reports in `dir`, newest period first. A file that will not
/// read is listed with its error, not left out.
///
/// # Errors
/// The directory cannot be read.
pub fn list(dir: &Path) -> Result<Vec<Entry>, String> {
    let mut out: Vec<Entry> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter(|e| crate::regular_with(e, "json"))
        .filter_map(|e| {
            let name = e.file_name().to_str()?.strip_suffix(".json")?.to_owned();
            let (from, to) = parse_id(&name)?;
            Some(match load(&e.path()) {
                Ok(data) => Entry::of(&data),
                Err(error) => Entry {
                    id: name,
                    period_from_ms: from,
                    period_to_ms: to,
                    generated_at_ms: None,
                    trigger: None,
                    verdict: None,
                    net: None,
                    unavailable: Vec::new(),
                    error: Some(error),
                },
            })
        })
        .collect();
    out.sort_by_key(|a| std::cmp::Reverse((a.period_to_ms, a.period_from_ms)));
    Ok(out)
}

/// Why a report could not be read.
#[derive(Debug, PartialEq, Eq)]
pub enum ReadError {
    /// No report by that id: matched against the listing, never joined.
    NotFound,
    Unreadable(String),
}

/// The file of the report `id`, found in `dir`'s listing.
#[must_use]
pub fn find(dir: &Path, id: &str) -> Option<PathBuf> {
    parse_id(id)?;
    crate::listed(dir, &format!("{id}.json"))
}

/// Whether a report for exactly this period exists.
#[must_use]
pub fn exists(dir: &Path, from_ms: i64, to_ms: i64) -> bool {
    find(dir, &id_of(from_ms, to_ms)).is_some()
}

/// The report `id`.
///
/// # Errors
/// There is none by that id, or it will not read.
pub fn read(dir: &Path, id: &str) -> Result<ReportData, ReadError> {
    let path = find(dir, id).ok_or(ReadError::NotFound)?;
    load(&path).map_err(ReadError::Unreadable)
}

fn load(path: &Path) -> Result<ReportData, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Write a report into `dir`, whole or not at all: to a temporary file
/// first, then renamed over the name. Readable by this user only — it
/// describes a live account. Returns its id.
///
/// # Errors
/// The directory or the file could not be written.
pub fn write(dir: &Path, data: &ReportData) -> Result<String, String> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| e.to_string())?;
    let id = id_of(data.period_from_ms, data.period_to_ms);
    // Built from two integers, so joining it is safe; nothing from a
    // request reaches this name.
    let tmp = dir.join(format!(".{id}.json.tmp"));
    let path = dir.join(format!("{id}.json"));
    let text = serde_json::to_string(data).map_err(|e| e.to_string())?;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    f.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    drop(f);
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(id)
}

/// Remove reports whose period ended more than `keep_days` before
/// `now_ms`. Returns how many went.
#[must_use]
pub fn prune(dir: &Path, now_ms: i64, keep_days: i64) -> usize {
    let cutoff = now_ms - keep_days * 24 * HOUR_MS;
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    rd.filter_map(Result::ok)
        .filter(|e| crate::regular_with(e, "json"))
        .filter(|e| {
            e.file_name()
                .to_str()
                .and_then(|n| n.strip_suffix(".json"))
                .and_then(parse_id)
                .is_some_and(|(_, to)| to < cutoff)
        })
        .filter(|e| std::fs::remove_file(e.path()).is_ok())
        .count()
}

// -- rendering --------------------------------------------------------------

/// Escape text for HTML content and attribute values.
#[must_use]
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Days since the epoch to a civil date (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    (y, m as u32, d as u32)
}

/// `2026-10-02 08:00 UTC`.
#[must_use]
pub fn utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (y, m, d) = civil(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        s / 3600,
        s % 3600 / 60
    )
}

fn number(n: f64) -> String {
    let s = if n.abs() >= 1000.0 {
        format!("{n:.2}")
    } else {
        let s = format!("{n:.6}");
        let s = s.trim_end_matches('0');
        s.trim_end_matches('.').to_owned()
    };
    if s == "-0" { "0".to_owned() } else { s }
}

fn signed(n: f64) -> String {
    let s = number(n);
    if n > 0.0 && s != "0" {
        format!("+{s}")
    } else {
        s
    }
}

fn bytes(b: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let f = b as f64;
    if b >= 1 << 30 {
        format!("{:.1} GiB", f / f64::from(1u32 << 30))
    } else {
        format!("{:.0} MiB", f / f64::from(1u32 << 20))
    }
}

fn duration(ms: i64) -> String {
    let m = ms.max(0) / 60_000;
    if m < 60 {
        format!("{m} min")
    } else if m < 48 * 60 {
        format!("{:.1} h", m as f64 / 60.0)
    } else {
        format!("{:.1} d", m as f64 / 1440.0)
    }
}

fn event_name(what: &str) -> Said {
    match what {
        "alert_raised" => Said::new("告警触发", "Alert raised"),
        "alert_cleared" => Said::new("告警解除", "Alert cleared"),
        "trader_halted" => Said::new("交易进程停机", "Trader halted"),
        "trader_resumed" => Said::new("交易进程恢复", "Trader resumed"),
        "unit_started" => Said::new("服务启动", "Service started"),
        "unit_stopped" => Said::new("服务停止", "Service stopped"),
        "control_lost" => Said::new("控制端口无应答", "Control port lost"),
        "control_back" => Said::new("控制端口恢复", "Control port back"),
        "recording_started" => Said::new("黑匣子开始记录", "Black box started recording"),
        other => Said::same(other),
    }
}

fn cannot_tell_reason(why: Option<CannotTell>) -> Said {
    match why {
        Some(CannotTell::Undecodable) => Said::new(
            "journal 里有解不开的帧，重建出来的账可能只是碰巧对上。",
            "Frames in the journal did not decode, so what was rebuilt may agree by luck.",
        ),
        Some(CannotTell::NoAdoption) => Said::new(
            "journal 里没有接管记录：读出来是空仓，既可能是真空仓，也可能是持着没人写下来的仓位。",
            "No adoption record: a flat reconstruction means flat, or means a position nobody \
             wrote down.",
        ),
        Some(CannotTell::ReadingPredatesTheRun) => Said::new(
            "交易所读数是本轮启动之前取的，描述的是上一轮，不能拿来比。",
            "The venue reading was taken before the current run started, so it describes the \
             run before it and cannot be compared.",
        ),
        None => Said::new(
            "没有差异，但 journal 不完整，不能算一致。",
            "No differences, but the journal is incomplete, so this does not count as agreement.",
        ),
    }
}

/// The equity curve as an inline SVG, or a sentence when there is not
/// enough to draw.
fn chart(points: &[(i64, f64)], lang: Lang) -> String {
    if points.len() < 2 {
        return format!(
            "<p class=\"muted\">{}</p>",
            escape(lang.pick(
                "权益样本不足，画不出曲线。",
                "Too few equity samples to draw a curve."
            ))
        );
    }
    let (w, h, pad_l, pad_r, pad_t, pad_b) = (800.0, 240.0, 70.0, 10.0, 10.0, 28.0);
    let (t0, t1) = (points[0].0, points[points.len() - 1].0);
    let lo = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let hi = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let span_y = if hi > lo { hi - lo } else { 1.0 };
    #[allow(clippy::cast_precision_loss)]
    let span_t = (t1 - t0).max(1) as f64;
    let mut path = String::new();
    for (i, (t, v)) in points.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let x = pad_l + (*t - t0) as f64 / span_t * (w - pad_l - pad_r);
        let y = pad_t + (hi - v) / span_y * (h - pad_t - pad_b);
        let _ = write!(path, "{}{x:.1},{y:.1}", if i == 0 { "M" } else { " L" });
    }
    let title = escape(lang.pick("权益曲线", "Equity curve"));
    format!(
        "<svg class=\"chart\" viewBox=\"0 0 {w} {h}\" role=\"img\" aria-label=\"{title}\">\
         <line class=\"axis\" x1=\"{pad_l}\" y1=\"{pad_t}\" x2=\"{pad_l}\" y2=\"{yb}\"/>\
         <line class=\"axis\" x1=\"{pad_l}\" y1=\"{yb}\" x2=\"{xr}\" y2=\"{yb}\"/>\
         <text x=\"{tx}\" y=\"{ty_hi}\" text-anchor=\"end\">{hi_s}</text>\
         <text x=\"{tx}\" y=\"{ty_lo}\" text-anchor=\"end\">{lo_s}</text>\
         <text x=\"{pad_l}\" y=\"{h_text}\">{t0_s}</text>\
         <text x=\"{xr}\" y=\"{h_text}\" text-anchor=\"end\">{t1_s}</text>\
         <path class=\"line\" d=\"{path}\"/></svg>",
        yb = h - pad_b,
        xr = w - pad_r,
        tx = pad_l - 6.0,
        ty_hi = pad_t + 10.0,
        ty_lo = h - pad_b,
        hi_s = escape(&number(hi)),
        lo_s = escape(&number(lo)),
        h_text = h - 6.0,
        t0_s = escape(&utc(t0)),
        t1_s = escape(&utc(t1)),
    )
}

const STYLE: &str = "
:root{--bg:#fff;--fg:#1f2328;--muted:#656d76;--line:#d0d7de;--card:#f6f8fa;
--good:#1a7f37;--bad:#cf222e;--warn:#9a6700;--accent:#0969da}
@media (prefers-color-scheme:dark){:root{--bg:#0d1117;--fg:#e6edf3;--muted:#8d96a0;
--line:#30363d;--card:#161b22;--good:#3fb950;--bad:#f85149;--warn:#d29922;--accent:#4493f8}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);
font:14px/1.5 -apple-system,BlinkMacSystemFont,'Segoe UI','PingFang SC','Microsoft YaHei',sans-serif}
main{max-width:920px;margin:0 auto;padding:24px 16px 48px}
h1{font-size:22px;margin:0 0 4px}h2{font-size:16px;margin:28px 0 8px}
.muted{color:var(--muted)}
.card{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:12px 16px}
.gaps{border-color:var(--warn)}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(170px,1fr));gap:8px}
.stat .k{color:var(--muted);font-size:12px}.stat .v{font-size:18px;font-variant-numeric:tabular-nums}
.na{color:var(--warn);font-style:italic}
.badge{display:inline-block;padding:2px 10px;border-radius:999px;font-weight:600;border:1px solid}
.agree{color:var(--good)}.disagree{color:var(--bad)}.cannot{color:var(--warn)}
table{width:100%;border-collapse:collapse}
th,td{text-align:left;padding:4px 8px;border-bottom:1px solid var(--line);vertical-align:top}
th{color:var(--muted);font-weight:500;font-size:12px}
td.n{font-variant-numeric:tabular-nums;white-space:nowrap}
.chart{width:100%;height:auto}
.chart .axis{stroke:var(--line)}.chart .line{stroke:var(--accent);stroke-width:1.5;fill:none}
.chart text{fill:var(--muted);font-size:11px}
code{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px}
";

/// One self-contained HTML document: inline CSS, an inline SVG, no
/// script and nothing fetched. Every string from the report is escaped.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn render_html(data: &ReportData, lang: Lang) -> String {
    let t = |zh: &'static str, en: &'static str| lang.pick(zh, en);
    let said = |s: &Said| escape(s.in_lang(lang));
    let na = || format!("<span class=\"na\">{}</span>", t("不可得", "unavailable"));
    let not_measured = || format!("<span class=\"na\">{}</span>", t("未测得", "not measured"));
    let stat = |k: &str, v: String| {
        format!(
            "<div class=\"stat card\"><div class=\"k\">{}</div><div class=\"v\">{v}</div></div>",
            escape(k)
        )
    };
    let gap_of = |s: Section| data.unavailable.iter().find(|g| g.section == s);
    let unavailable_line = |s: Section| {
        format!(
            "<p class=\"na\">{}</p>",
            gap_of(s).map_or_else(|| escape(t("不可得", "Unavailable")), |g| said(&g.reason))
        )
    };

    let mut o = String::with_capacity(16 * 1024);
    let title = format!(
        "{} {} – {}",
        t("交易报告", "Trading report"),
        utc(data.period_from_ms),
        utc(data.period_to_ms)
    );
    let _ = write!(
        o,
        "<!doctype html><html lang=\"{}\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <meta name=\"color-scheme\" content=\"light dark\">\
         <title>{}</title><style>{STYLE}</style></head><body><main>",
        t("zh-CN", "en"),
        escape(&title)
    );
    let _ = write!(
        o,
        "<h1>{}</h1><p class=\"muted\">{} {} – {} · {} {} · {}</p>",
        escape(t("交易报告", "Trading report")),
        escape(t("时段", "Period")),
        escape(&utc(data.period_from_ms)),
        escape(&utc(data.period_to_ms)),
        escape(t("生成于", "generated")),
        escape(&utc(data.generated_at_ms)),
        escape(match data.trigger {
            Trigger::Scheduled => t("按计划生成", "scheduled"),
            Trigger::Manual => t("手动生成", "on request"),
        })
    );

    if !data.unavailable.is_empty() {
        let _ = write!(
            o,
            "<div class=\"card gaps\"><strong>{}</strong><ul>",
            escape(t(
                "以下部分无法填写，报告里没有它们的数字——那不是零：",
                "These sections could not be filled; their figures are absent, not zero:"
            ))
        );
        for g in &data.unavailable {
            let _ = write!(
                o,
                "<li><strong>{}</strong>: {}</li>",
                said(&g.section.name()),
                said(&g.reason)
            );
        }
        o.push_str("</ul></div>");
    }

    // P&L
    let _ = write!(o, "<h2>{}</h2>", escape(t("盈亏", "P&L")));
    match data.trader.as_ref().and_then(|tr| tr.pnl.as_ref()) {
        Some(p) => {
            let v = |x: Option<f64>| x.map_or_else(not_measured, |x| escape(&signed(x)));
            let plain = |x: Option<f64>| x.map_or_else(not_measured, |x| escape(&number(x)));
            o.push_str("<div class=\"grid\">");
            o.push_str(&stat(t("净盈亏", "Net P&L"), v(p.net)));
            o.push_str(&stat(t("已实现", "Realized"), v(p.realized)));
            o.push_str(&stat(t("手续费", "Fees"), v(p.fees)));
            o.push_str(&stat(t("资金费", "Funding"), v(p.funding)));
            o.push_str(&stat(t("期初权益", "Equity, first"), plain(p.equity_first)));
            o.push_str(&stat(t("期末权益", "Equity, last"), plain(p.equity_last)));
            o.push_str("</div>");
            if p.runs > 1 {
                let _ = write!(
                    o,
                    "<p class=\"muted\">{}</p>",
                    escape(&lang.pick(
                        format!("这段时间里交易进程运行了 {} 次（中间有重启）；盈亏按每次运行分别计算后相加。", p.runs),
                        format!("The trader ran {} times in this period (it restarted); P&L is summed run by run.", p.runs),
                    ))
                );
            }
            let _ = write!(o, "<h2>{}</h2>", escape(t("权益曲线", "Equity")));
            o.push_str(&chart(&p.equity, lang));
        }
        None => o.push_str(&unavailable_line(if data.trader.is_some() {
            Section::Pnl
        } else {
            Section::Trader
        })),
    }

    // Reconciliation
    let _ = write!(o, "<h2>{}</h2>", escape(t("对账", "Reconciliation")));
    match &data.reconciliation {
        Some(r) => {
            let (class, word) = match r.verdict {
                Verdict::Agree => ("agree", t("一致", "Agree")),
                Verdict::Disagree => ("disagree", t("不一致", "Disagree")),
                Verdict::CannotTell => ("cannot", t("无法判断", "Cannot tell")),
            };
            let _ = write!(
                o,
                "<div class=\"card\"><span class=\"badge {class}\">{}</span> \
                 <span class=\"muted\">{} <code>{}</code> · {} {}（{} {}）</span>",
                escape(word),
                escape(t("journal", "journal")),
                escape(&r.journal),
                escape(t("交易所读数取于", "venue reading taken")),
                escape(&utc(r.record_read_at_ms)),
                escape(t("生成报告时已过", "age at generation")),
                escape(&duration(r.record_age_ms)),
            );
            if r.verdict == Verdict::CannotTell {
                let _ = write!(o, "<p>{}</p>", said(&cannot_tell_reason(r.cannot_tell)));
            }
            if r.undecodable > 0 {
                let _ = write!(
                    o,
                    "<p class=\"na\">{}</p>",
                    escape(&lang.pick(
                        format!("journal 里有 {} 条记录解不开。", r.undecodable),
                        format!(
                            "{} records in the journal could not be decoded.",
                            r.undecodable
                        ),
                    ))
                );
            }
            if !r.differences.is_empty() {
                o.push_str("<ul>");
                for d in &r.differences {
                    let _ = write!(o, "<li><code>{}</code></li>", escape(d));
                }
                o.push_str("</ul>");
                if r.differences_total > r.differences.len() {
                    let _ = write!(
                        o,
                        "<p class=\"muted\">{}</p>",
                        escape(&lang.pick(
                            format!(
                                "另有 {} 条未列出。",
                                r.differences_total - r.differences.len()
                            ),
                            format!(
                                "{} more not listed.",
                                r.differences_total - r.differences.len()
                            ),
                        ))
                    );
                }
            }
            let _ = write!(
                o,
                "<p class=\"muted\">{}</p></div>",
                escape(t(
                    "对账是生成报告时的状态，不是整段时间的。",
                    "Reconciliation is as of when the report was generated, not over the period."
                ))
            );
        }
        None => o.push_str(&unavailable_line(Section::Reconciliation)),
    }

    // End state
    let _ = write!(o, "<h2>{}</h2>", escape(t("期末状态", "State at the end")));
    match &data.trader {
        Some(tr) => {
            let _ = write!(
                o,
                "<p class=\"muted\">{} {}</p><div class=\"grid\">",
                escape(t("最后一个样本：", "Last sample:")),
                escape(&utc(tr.last_at_ms))
            );
            let halted = match tr.halted {
                Some(true) => format!(
                    "<span class=\"disagree\">{}</span>{}",
                    escape(t("已停机", "Halted")),
                    tr.halt_reason
                        .as_deref()
                        .map(|r| format!(" <span class=\"muted\">{}</span>", escape(r)))
                        .unwrap_or_default()
                ),
                Some(false) => escape(t("运行中", "Running")),
                None => na(),
            };
            o.push_str(&stat(t("停机", "Halt"), halted));
            o.push_str(&stat(
                t("挂单数", "Resting orders"),
                tr.resting.map_or_else(na, |r| r.to_string()),
            ));
            let positions = match &tr.positions {
                None => na(),
                Some(ps) if ps.is_empty() => escape(t("空仓", "Flat")),
                Some(ps) => ps
                    .iter()
                    .map(|p| format!("{} {}", escape(&p.side), escape(&p.amount)))
                    .collect::<Vec<_>>()
                    .join("<br>"),
            };
            o.push_str(&stat(t("持仓", "Positions"), positions));
            o.push_str("</div>");
        }
        None => o.push_str(&unavailable_line(Section::Trader)),
    }

    // Events
    let _ = write!(o, "<h2>{}</h2>", escape(t("事件", "Events")));
    match &data.events {
        None => o.push_str(&unavailable_line(Section::Events)),
        Some(ev) if ev.is_empty() => {
            let _ = write!(
                o,
                "<p class=\"muted\">{}</p>",
                escape(t(
                    "这段时间里没有告警、停机或服务启停。",
                    "No alerts, halts or service starts and stops in this period."
                ))
            );
        }
        Some(ev) => {
            if data.events_omitted > 0 {
                let _ = write!(
                    o,
                    "<p class=\"muted\">{}</p>",
                    escape(&lang.pick(
                        format!("更早的 {} 条事件未列出。", data.events_omitted),
                        format!("{} earlier events are not listed.", data.events_omitted),
                    ))
                );
            }
            let _ = write!(
                o,
                "<table><thead><tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr></thead><tbody>",
                escape(t("时间", "Time")),
                escape(t("事件", "Event")),
                escape(t("对象", "Subject")),
                escape(t("说明", "Detail")),
            );
            for e in ev {
                let _ = write!(
                    o,
                    "<tr><td class=\"n\">{}</td><td>{}</td><td><code>{}</code></td><td>{}</td></tr>",
                    escape(&utc(e.at_ms)),
                    said(&event_name(&e.what)),
                    escape(e.subject.as_deref().unwrap_or("")),
                    e.detail.as_ref().map(said).unwrap_or_default(),
                );
            }
            o.push_str("</tbody></table>");
        }
    }

    // Host
    let _ = write!(o, "<h2>{}</h2>", escape(t("主机", "Host")));
    match &data.host {
        None => o.push_str(&unavailable_line(Section::Host)),
        Some(h) => {
            o.push_str("<div class=\"grid\">");
            o.push_str(&stat(
                t("最高负载（1 分钟）", "Peak load (1 min)"),
                h.peak_load.map_or_else(na, |l| format!("{l:.2}")),
            ));
            o.push_str(&stat(
                t("最少可用内存", "Least memory available"),
                h.min_mem_available.map_or_else(na, |m| {
                    h.mem_total.map_or_else(
                        || escape(&bytes(m)),
                        |total| format!("{} / {}", escape(&bytes(m)), escape(&bytes(total))),
                    )
                }),
            ));
            o.push_str(&stat(
                t("时钟未同步的样本", "Samples with the clock unsynced"),
                h.clock_unsynced_samples.to_string(),
            ));
            o.push_str("</div>");
            match &h.disks {
                None => o.push_str(&format!(
                    "<p class=\"na\">{}</p>",
                    escape(t(
                        "这段时间里没有磁盘测量。",
                        "No disk measurement in this period."
                    ))
                )),
                Some(disks) => {
                    let _ = write!(
                        o,
                        "<table><thead><tr><th>{}</th><th>{}</th></tr></thead><tbody>",
                        escape(t("挂载点", "Mount")),
                        escape(t("最少剩余空间", "Least free space")),
                    );
                    for d in disks {
                        let _ = write!(
                            o,
                            "<tr><td><code>{}</code></td><td class=\"n\">{:.1}%</td></tr>",
                            escape(&d.mount),
                            d.min_free_pct
                        );
                    }
                    o.push_str("</tbody></table>");
                }
            }
        }
    }

    let _ = write!(
        o,
        "<p class=\"muted\" style=\"margin-top:32px\">quanterdeck {} · {}</p></main></body></html>",
        escape(env!("CARGO_PKG_VERSION")),
        escape(t(
            "数字缺失处是没有测到，不是零。",
            "Where a figure is missing it was not measured; it is not zero."
        ))
    );
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::RecordView;
    use serde_json::json;

    const DAY: i64 = 86_400_000;
    const FROM: i64 = 1_790_000_000_000 / DAY * DAY;
    const TO: i64 = FROM + DAY;

    fn recon(verdict: Verdict) -> Reconciliation {
        let view = |at| RecordView {
            symbol: "BTCUSDT".into(),
            read_at_ms: at,
            legs: vec![],
            orders: vec![],
        };
        Reconciliation {
            journal: "trader-1".into(),
            believed: view(TO - 60_000),
            venue: view(TO - 60_000),
            differences: if verdict == Verdict::Disagree {
                vec!["position 1 != 2".into()]
            } else {
                vec![]
            },
            verdict,
            cannot_tell: (verdict == Verdict::CannotTell).then_some(CannotTell::NoAdoption),
            agrees: verdict == Verdict::Agree,
            undecodable: 0,
            hedged: false,
        }
    }

    fn trader(at: i64, since: i64, net: &str, fees: Option<&str>, equity: &str) -> Value {
        json!({"at": at, "trader": {"halted": false, "resting": 4,
            "positions": [{"side": "LONG", "amount": "0.002"}],
            "pnl": {"since_ms": since, "realized": net, "fees": fees, "funding": "0",
                    "net": net, "equity": equity}}})
    }

    fn window() -> Value {
        json!({
            "from_ms": FROM, "to_ms": TO,
            "trader": [
                // A run that began before the period: only its change counts.
                trader(FROM + 1000, FROM - DAY, "10", Some("1"), "1000"),
                trader(FROM + 3_600_000, FROM - DAY, "12", Some("1.5"), "1002"),
                // It restarted: the new run's figures count whole.
                trader(FROM + 7_200_000, FROM + 7_000_000, "0.5", Some("0.1"), "1002.5"),
                trader(TO - 30_000, FROM + 7_000_000, "3", Some("0.2"), "1005"),
                // Outside the period: ignored.
                trader(TO + 30_000, FROM + 7_000_000, "99", Some("9"), "9999"),
            ],
            "host": [
                {"at": FROM + 1000, "host": {"load": [0.5, 0.4, 0.3], "mem_available": 4u64 << 30,
                 "mem_total": 16u64 << 30, "clock_synced": true,
                 "disks": [{"mount": "/", "size": 100, "avail": 40, "used": 60}]}},
                {"at": FROM + 2000, "host": {"load": [2.5, 0.4, 0.3], "mem_available": 3u64 << 30,
                 "mem_total": 16u64 << 30, "clock_synced": false,
                 "disks": [{"mount": "/", "size": 100, "avail": 30, "used": 70}]}},
            ],
            "events": [
                {"at": FROM + 5000, "k": "event", "what": "alert_raised", "key": "disk",
                 "message": "磁盘快满了", "message_en": "Disk nearly full"},
                {"at": FROM + 6000, "k": "event", "what": "trader_halted", "reason": "operator: test"},
                {"at": FROM + 7000, "k": "event", "what": "something_else"},
            ],
        })
    }

    fn inputs(blackbox: Result<Value, Said>, recon: Result<Reconciliation, Said>) -> Inputs {
        Inputs {
            from_ms: FROM,
            to_ms: TO,
            generated_at_ms: TO + 120_000,
            trigger: Trigger::Scheduled,
            blackbox,
            reconciliation: recon,
        }
    }

    #[test]
    fn pnl_is_summed_run_by_run_across_a_restart() {
        let r = build(inputs(Ok(window()), Ok(recon(Verdict::Agree))));
        assert!(r.unavailable.is_empty(), "{:?}", r.unavailable);
        let t = r.trader.expect("trader");
        let p = t.pnl.expect("pnl");
        // (12 - 10) + 3 = 5; fees (1.5 - 1) + 0.2 = 0.7.
        assert_eq!(p.net, Some(5.0));
        assert!((p.fees.expect("fees") - 0.7).abs() < 1e-9);
        assert_eq!(p.funding, Some(0.0), "measured zero stays zero");
        assert_eq!(p.runs, 2);
        assert_eq!(p.equity_first, Some(1000.0));
        assert_eq!(p.equity_last, Some(1005.0));
        assert_eq!(p.equity.len(), 4);
        assert_eq!(t.resting, Some(4));
        assert_eq!(t.halted, Some(false));
        let h = r.host.expect("host");
        assert_eq!(h.peak_load, Some(2.5));
        assert_eq!(h.min_mem_available, Some(3 << 30));
        assert_eq!(h.clock_unsynced_samples, 1);
        assert_eq!(h.disks.expect("disks")[0].min_free_pct, 30.0);
        let ev = r.events.expect("events");
        assert_eq!(ev.len(), 2, "only the kinds a report is about");
        assert_eq!(ev[0].subject.as_deref(), Some("disk"));
        assert_eq!(
            ev[0].detail.as_ref().map(|d| d.en.as_str()),
            Some("Disk nearly full")
        );
        assert_eq!(r.reconciliation.expect("recon").record_age_ms, 180_000);
    }

    #[test]
    fn unmeasured_fees_are_absent_not_zero() {
        let w =
            json!({"trader": [trader(FROM + 1, FROM, "1", None, "10")], "host": [], "events": []});
        let r = build(inputs(Ok(w), Ok(recon(Verdict::Agree))));
        let p = r.trader.expect("trader").pnl.expect("pnl");
        assert_eq!(p.fees, None);
        assert_eq!(p.net, Some(1.0));
        // No host samples is a gap with a reason; no events is an empty
        // list, which is a fact, not a gap.
        assert_eq!(r.events, Some(vec![]));
        assert!(r.host.is_none());
        assert_eq!(r.unavailable[0].section, Section::Host);
    }

    #[test]
    fn without_an_agent_every_section_it_feeds_is_unavailable_with_the_reason() {
        let why = Said::new("没有主机代理", "No host agent");
        let r = build(inputs(
            Err(why.clone()),
            Err(Said::new("没有记录", "No record")),
        ));
        assert!(r.trader.is_none() && r.events.is_none() && r.host.is_none());
        assert!(r.reconciliation.is_none());
        let sections: Vec<Section> = r.unavailable.iter().map(|g| g.section).collect();
        assert_eq!(
            sections,
            [
                Section::Trader,
                Section::Events,
                Section::Host,
                Section::Reconciliation
            ]
        );
        assert_eq!(r.unavailable[0].reason, why);
        let entry = Entry::of(&r);
        assert_eq!(entry.verdict, None, "unavailable is not agreement");
        assert_eq!(entry.net, None);
        for lang in [Lang::Zh, Lang::En] {
            let html = render_html(&r, lang);
            assert!(html.contains(&escape(why.in_lang(lang))), "{html}");
            assert!(!html.contains("Agree") && !html.contains(">一致<"));
        }
    }

    #[test]
    fn samples_without_pnl_are_a_pnl_gap() {
        let w =
            json!({"trader": [{"at": FROM + 1, "trader": {"halted": true, "halt_reason": "x"}}]});
        let r = build(inputs(Ok(w), Ok(recon(Verdict::Agree))));
        assert!(r.trader.as_ref().expect("trader").pnl.is_none());
        assert!(r.unavailable.iter().any(|g| g.section == Section::Pnl));
    }

    #[test]
    fn cannot_tell_is_never_drawn_as_agreement() {
        let r = build(inputs(Ok(window()), Ok(recon(Verdict::CannotTell))));
        let en = render_html(&r, Lang::En);
        assert!(en.contains("Cannot tell") && en.contains("No adoption record"));
        assert!(!en.contains(">Agree<"));
        let zh = render_html(&r, Lang::Zh);
        assert!(zh.contains("无法判断") && zh.contains("lang=\"zh-CN\""));
        assert!(zh.contains("净盈亏") && en.contains("Net P&amp;L"));
        let d = render_html(
            &build(inputs(Ok(window()), Ok(recon(Verdict::Disagree)))),
            Lang::En,
        );
        assert!(d.contains("Disagree") && d.contains("position 1 != 2"));
    }

    #[test]
    fn every_string_from_the_inputs_is_escaped() {
        let hostile = "<script>alert('x')</script>\"&";
        let mut w = window();
        w["events"][0]["message_en"] = json!(hostile);
        w["events"][0]["key"] = json!(hostile);
        w["trader"][3]["trader"]["halted"] = json!(true);
        w["trader"][3]["trader"]["halt_reason"] = json!(hostile);
        w["trader"][3]["trader"]["positions"] = json!([{"side": hostile, "amount": hostile}]);
        w["host"][0]["host"]["disks"][0]["mount"] = json!(hostile);
        let mut rec = recon(Verdict::Disagree);
        rec.journal = hostile.into();
        rec.differences = vec![hostile.into()];
        let r = build(inputs(Ok(w), Ok(rec)));
        for lang in [Lang::Zh, Lang::En] {
            let html = render_html(&r, lang);
            assert!(!html.contains("<script"), "{html}");
            assert!(!html.contains(hostile));
            assert!(html.contains("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;&quot;&amp;"));
            assert!(
                !html.contains("src=") && !html.contains("href="),
                "nothing fetched"
            );
        }
    }

    #[test]
    fn the_chart_is_inline_svg_and_long_series_are_thinned() {
        let all: Vec<(i64, f64)> = (0..5000).map(|i| (i64::from(i), f64::from(i))).collect();
        let t = thin(&all, EQUITY_POINTS);
        assert_eq!(t.len(), EQUITY_POINTS);
        assert_eq!(t[0], all[0]);
        assert_eq!(t[EQUITY_POINTS - 1], all[4999]);
        let svg = chart(&t, Lang::En);
        assert!(svg.starts_with("<svg") && svg.contains("<path"));
        assert!(chart(&t[..1], Lang::En).contains("Too few"));
    }

    #[test]
    fn the_schedule_reports_the_last_whole_period_once() {
        let now = TO + 5 * 60_000;
        assert_eq!(last_period(now, 24), (FROM, TO));
        // Inside the grace the previous period is still the newest one.
        assert_eq!(last_period(TO + 60_000, 24), (FROM - DAY, FROM));
        assert_eq!(due(now, 24, |_, _| false), Some((FROM, TO)));
        assert_eq!(due(now, 24, |f, t| (f, t) == (FROM, TO)), None);
        assert_eq!(next_due(now, 24), TO + DAY + GRACE_MS);
        let (f, t) = last_period(now, 6);
        assert_eq!(t - f, 6 * 3_600_000);
        assert_eq!(t % (6 * 3_600_000), 0);
    }

    #[test]
    fn ids_are_two_numbers_and_nothing_else() {
        assert_eq!(parse_id(&id_of(1, 2)), Some((1, 2)));
        for bad in [
            "",
            "1",
            "1-",
            "-1",
            "a-1",
            "1-2-3",
            "../1-2",
            "1-2.json",
            "1-2/..",
            "/etc/passwd",
        ] {
            assert_eq!(parse_id(bad), None, "{bad}");
        }
    }

    #[test]
    fn written_reports_list_read_back_and_age_out() {
        let d = tempfile::tempdir().expect("dir");
        let dir = d.path().join("reports");
        let r = build(inputs(Ok(window()), Ok(recon(Verdict::Agree))));
        let id = write(&dir, &r).expect("written");
        assert!(exists(&dir, FROM, TO));
        assert_eq!(read(&dir, &id).expect("reads"), r);
        assert_eq!(read(&dir, "1-2"), Err(ReadError::NotFound));
        assert_eq!(read(&dir, "../reports"), Err(ReadError::NotFound));
        std::fs::write(dir.join("5-6.json"), "not json").expect("write");
        let listed = list(&dir).expect("lists");
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, id, "newest first");
        assert_eq!(listed[0].verdict, Some(Verdict::Agree));
        assert!(
            listed[1].error.is_some(),
            "a broken file is listed with its error"
        );
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join(format!("{id}.json")))
            .expect("stat")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(
            prune(&dir, TO + 89 * DAY, KEEP_DAYS),
            1,
            "only the ancient one"
        );
        assert_eq!(prune(&dir, TO + 91 * DAY, KEEP_DAYS), 1);
        assert!(list(&dir).expect("lists").is_empty());
    }

    #[test]
    fn dates_are_utc_civil_dates() {
        assert_eq!(utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(utc(1_790_000_000_000), "2026-09-21 14:13 UTC");
        assert_eq!(number(1.230_000), "1.23");
        assert_eq!(signed(-0.5), "-0.5");
        assert_eq!(signed(2.0), "+2");
    }
}
