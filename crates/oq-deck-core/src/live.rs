//! What a live process believed, and whether the venue agrees.
//!
//! Two readings of the same account, in the same vocabulary:
//!
//! * `oq-belief` reconstructs the position and resting orders from a
//!   process's own journal — what it *thought* it held.
//! * `oq-recon --record` writes what the **venue** says, as a text
//!   record with a declared format.
//!
//! Neither is checked against the other by anything that a person then
//! looks at, which is the gap this module closes. It talks to no venue
//! and starts no process: it reads a journal file and a record file, and
//! asks `oq_gateway::record::Record::differences` for the answer.
//!
//! Nothing here decides what a difference *means*. A position that moved
//! and a process that misread a position that did not move produce the
//! same line here; separating them is the operator's job, and pretending
//! otherwise would be the console inventing a judgement the framework
//! deliberately does not make.

use std::fs;
use std::path::{Path, PathBuf};

use oq_gateway::record::Record;
use oq_live::belief::Belief;
use serde::Serialize;

/// What a process believed it held.
#[derive(Debug, Clone, Serialize)]
pub struct BeliefView {
    pub symbol: Option<String>,
    pub position_lots: i64,
    pub entry_ticks: i64,
    pub resting: Vec<String>,
    pub price_scale: u8,
    pub qty_scale: u8,
    /// The run took over a position it did not open.
    pub adopted: bool,
    /// The account holds both legs. A net figure hides one of them, so
    /// the interface must not present a hedged account as a single
    /// number.
    pub hedged: bool,
    /// Frames the reader could not decode.
    ///
    /// Surfaced rather than swallowed: a belief reconstructed from a
    /// journal with undecodable frames is a belief with holes, and the
    /// number of holes is the only warning there is.
    pub undecodable: u64,
    /// Each leg as `(name, signed lots, entry ticks)`. What a hedged
    /// account must be shown as, instead of `position_lots`.
    pub legs: Vec<(String, i64, i64)>,
}

impl From<&Belief> for BeliefView {
    fn from(belief: &Belief) -> Self {
        Self {
            symbol: belief.symbol.clone(),
            position_lots: belief.position_lots,
            entry_ticks: belief.entry_ticks,
            resting: belief.resting.clone(),
            price_scale: belief.price_scale,
            qty_scale: belief.qty_scale,
            adopted: belief.adopted,
            hedged: belief.hedged,
            undecodable: belief.undecodable,
            legs: belief.legs.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct JournalSummary {
    pub id: String,
    pub path: String,
    pub belief: BeliefView,
}

/// A journal in a listing, whether or not it read.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Entry {
    Read(Box<JournalSummary>),
    Unreadable {
        id: String,
        path: String,
        error: String,
    },
}

fn read_belief(path: &Path) -> Result<Belief, String> {
    Belief::from_journal(path).map_err(|e| e.to_string())
}

fn id_of(path: &Path) -> String {
    path.file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
}

/// Every journal in a directory.
///
/// A missing directory is an empty listing, not an error: a deck pointed
/// at a journal directory nothing has written to yet is a normal state.
///
/// # Errors
/// Any other failure to read the directory, which is not an empty one.
pub fn list(dir: &Path) -> Result<Vec<Entry>, String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };

    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "oqj"))
        .collect();
    paths.sort();

    Ok(paths
        .iter()
        .map(|path| {
            let id = id_of(path);
            match read_belief(path) {
                Ok(belief) => Entry::Read(Box::new(JournalSummary {
                    id,
                    path: path.display().to_string(),
                    belief: BeliefView::from(&belief),
                })),
                Err(error) => Entry::Unreadable {
                    id,
                    path: path.display().to_string(),
                    error,
                },
            }
        })
        .collect())
}

/// Resolve a journal id inside `dir`, refusing anything else.
///
/// The id arrives from a URL, so it is matched against the directory
/// rather than joined onto it.
#[must_use]
pub fn resolve(dir: &Path, id: &str) -> Option<PathBuf> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        return None;
    }
    crate::listed(dir, &format!("{id}.oqj"))
}

/// One journal's belief.
///
/// # Errors
/// The journal is missing or will not read.
pub fn belief(dir: &Path, id: &str) -> Result<BeliefView, String> {
    let path = resolve(dir, id).ok_or_else(|| format!("no journal named {id}"))?;
    Ok(BeliefView::from(&read_belief(&path)?))
}

/// One account, as one side reported it.
#[derive(Debug, Clone, Serialize)]
pub struct RecordView {
    pub symbol: String,
    pub read_at_ms: i64,
    /// Each leg as `(name, position, entry price)`. Two legs means the
    /// account is hedged.
    pub legs: Vec<(String, f64, f64)>,
    pub orders: Vec<String>,
}

impl From<&Record> for RecordView {
    fn from(record: &Record) -> Self {
        Self {
            symbol: record.symbol.clone(),
            read_at_ms: record.read_at_ms,
            legs: record.legs.clone(),
            orders: record.orders.clone(),
        }
    }
}

/// The two readings, and where they disagree.
#[derive(Debug, Clone, Serialize)]
pub struct Reconciliation {
    pub journal: String,
    /// What the process believed, rendered in the venue's vocabulary so
    /// the two are comparable at all.
    pub believed: RecordView,
    /// What the venue said.
    pub venue: RecordView,
    /// One line per disagreement, from the framework's own comparison.
    pub differences: Vec<String>,
    /// Whether the two can be said to agree, disagree, or neither.
    pub verdict: Verdict,
    /// True only when `verdict` is `Agree`. Kept for readers of the old
    /// field; it no longer reads "no differences found" as agreement.
    pub agrees: bool,
    /// Repeated here because it qualifies everything above: a belief
    /// rebuilt from a journal with undecodable frames may agree by luck.
    pub undecodable: u64,
    /// Present when the account holds both legs.
    pub hedged: bool,
}

/// What a reconciliation concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Nothing differs, and the belief is whole.
    Agree,
    /// Something differs.
    Disagree,
    /// Nothing differs, but the belief has holes — undecodable frames, or
    /// no record of the position the process took over — so "no
    /// difference" may be luck. Not agreement; `agrees` was true here, and
    /// it is the case this console exists to not render as agreement.
    CannotTell,
}

/// Compare a process's belief against a venue record.
///
/// `venue_record` is the text `oq-recon --record` wrote. Parsing it is
/// the framework's job, not this one's.
///
/// # Errors
/// The journal is missing or will not read, or the record will not parse.
pub fn reconcile(dir: &Path, id: &str, venue_record: &str) -> Result<Reconciliation, String> {
    let path = resolve(dir, id).ok_or_else(|| format!("no journal named {id}"))?;
    let belief = read_belief(&path)?;
    let venue =
        Record::parse(venue_record).map_err(|e| format!("the venue record will not parse: {e}"))?;

    // Rendered at the venue record's own timestamp. The comparison is
    // between two accounts, not two clocks, and giving the belief a
    // different read time would put a spurious line in the output.
    let believed = belief.to_record(venue.read_at_ms);
    let differences = believed.differences(&venue);

    let verdict = if !differences.is_empty() {
        Verdict::Disagree
    } else if belief.undecodable > 0 || !belief.adopted {
        Verdict::CannotTell
    } else {
        Verdict::Agree
    };

    Ok(Reconciliation {
        journal: id.to_owned(),
        believed: RecordView::from(&believed),
        venue: RecordView::from(&venue),
        agrees: verdict == Verdict::Agree,
        verdict,
        differences,
        undecodable: belief.undecodable,
        hedged: belief.hedged,
    })
}

/// One journal record, as the interface shows it.
///
/// `kind` names the variant; `at` is the process's own timestamp, in
/// nanoseconds, where the record has one. Every other field is carried
/// as the record holds it — prices and quantities in integer ticks and
/// lots, the venue's own strings where the venue sent strings.
#[derive(Debug, Clone, Serialize)]
pub struct JournalRecord {
    pub seq: u64,
    pub kind: &'static str,
    pub at: Option<i64>,
    pub fields: serde_json::Value,
}

/// A page of records, newest first.
#[derive(Debug, Clone, Serialize)]
pub struct RecordsPage {
    pub journal: String,
    /// Records of the kinds asked for in the whole journal.
    pub total: usize,
    pub records: Vec<JournalRecord>,
    /// Pass as `before` for the next, older page; `None` at the start.
    pub next_before: Option<u64>,
    pub price_scale: u8,
    pub qty_scale: u8,
    /// Frames that did not decode, anywhere in the journal. A replay with
    /// holes is shown as one.
    pub undecodable: u64,
}

fn view(seq: u64, record: oq_live::record::Record) -> JournalRecord {
    use oq_live::record::Record as R;
    use serde_json::json;
    let side = |s: oq_types::Side| match s {
        oq_types::Side::Buy => "buy",
        oq_types::Side::Sell => "sell",
    };
    let (kind, at, fields) = match record {
        R::SessionStart {
            prefix,
            symbol,
            price_scale,
            qty_scale,
        } => (
            "session_start",
            None,
            json!({"prefix": prefix, "symbol": symbol, "price_scale": price_scale, "qty_scale": qty_scale}),
        ),
        R::Tick {
            at,
            seen,
            last,
            bid,
            ask,
            volume,
        } => (
            "tick",
            Some(at.0),
            json!({"seen": seen.0, "last": last.0, "bid": bid.0, "ask": ask.0, "volume": volume.0}),
        ),
        R::Submitted {
            at,
            client_id,
            side: s,
            limit_price,
            qty,
            reduce_only,
            leg,
        } => (
            "submitted",
            Some(at.0),
            json!({"client_id": client_id, "side": side(s), "limit_price": limit_price.0,
                   "qty": qty.0, "reduce_only": reduce_only, "leg": leg}),
        ),
        R::Outcome {
            at,
            client_id,
            tag,
            detail,
        } => (
            "outcome",
            Some(at.0),
            json!({"client_id": client_id, "tag": format!("{tag:?}").to_lowercase(), "detail": detail}),
        ),
        R::Cancelled { at, client_id } => {
            ("cancelled", Some(at.0), json!({"client_id": client_id}))
        }
        R::Fill {
            at,
            client_id,
            trade_id,
            qty,
            price,
            order,
            side,
        } => (
            "fill",
            Some(at.0),
            json!({"client_id": client_id, "trade_id": trade_id, "qty": qty, "price": price,
                   "order": order, "side": side}),
        ),
        R::Refused { at, breach } => ("refused", Some(at.0), json!({"breach": breach})),
        R::Reconciled { at, legs } => (
            "reconciled",
            Some(at.0),
            json!({"legs": legs.into_iter().map(|(sym, side, lots, entry)|
                json!({"symbol": sym, "side": side, "lots": lots, "entry": entry})).collect::<Vec<_>>()}),
        ),
        R::Waiting { at, entries } => (
            "waiting",
            Some(at.0),
            json!(
                entries
                    .into_iter()
                    .collect::<std::collections::BTreeMap<_, _>>()
            ),
        ),
        R::Operator {
            at,
            command,
            reason,
            origin,
            outcome,
        } => (
            "operator",
            Some(at.0),
            json!({"command": command, "reason": reason, "origin": origin, "outcome": outcome}),
        ),
        R::Funding {
            at,
            settled_ms,
            rate,
            mark,
            venue,
            model,
            verified,
        } => (
            "funding",
            Some(at.0),
            json!({
                "settled_ms": settled_ms, "rate": rate, "mark": mark,
                "venue": cash_text(venue), "model": cash_text(model), "verified": verified,
            }),
        ),
    };
    JournalRecord {
        seq,
        kind,
        at,
        fields,
    }
}

/// Most records one page returns.
pub const MAX_PAGE: usize = 1000;

/// A journal's records of the given kinds (all, when `kinds` is empty),
/// newest first, `limit` at a time, older than `before` when given.
///
/// # Errors
/// The journal is missing or will not read.
pub fn records(
    dir: &Path,
    id: &str,
    kinds: &[String],
    limit: usize,
    before: Option<u64>,
) -> Result<RecordsPage, String> {
    records_between(dir, id, kinds, limit, before, None, None)
}

/// [`records`], only those whose time falls between `from_ns` and
/// `to_ns` when given — the minutes around a moment under review.
/// Records without a time (the session start) are kept.
///
/// # Errors
/// The journal is missing or will not read.
pub fn records_between(
    dir: &Path,
    id: &str,
    kinds: &[String],
    limit: usize,
    before: Option<u64>,
    from_ns: Option<i64>,
    to_ns: Option<i64>,
) -> Result<RecordsPage, String> {
    let path = resolve(dir, id).ok_or_else(|| format!("no journal named {id}"))?;
    let replay = oq_journal::Reader::open(&path)
        .and_then(|r| r.replay())
        .map_err(|e| e.to_string())?;
    let mut undecodable = 0;
    let (mut price_scale, mut qty_scale) = (0, 0);
    let mut all = Vec::new();
    for frame in replay.since(0) {
        match oq_live::record::Record::decode(frame.kind, &frame.payload) {
            Some(record) => {
                if let oq_live::record::Record::SessionStart {
                    price_scale: p,
                    qty_scale: q,
                    ..
                } = &record
                {
                    (price_scale, qty_scale) = (*p, *q);
                }
                let v = view(frame.seq, record);
                let in_time = v.at.is_none_or(|at| {
                    from_ns.is_none_or(|f| at >= f) && to_ns.is_none_or(|t| at <= t)
                });
                if in_time && (kinds.is_empty() || kinds.iter().any(|k| k == v.kind)) {
                    all.push(v);
                }
            }
            None => undecodable += 1,
        }
    }
    let total = all.len();
    let limit = limit.clamp(1, MAX_PAGE);
    let records: Vec<JournalRecord> = all
        .into_iter()
        .rev()
        .filter(|r| before.is_none_or(|b| r.seq < b))
        .take(limit)
        .collect();
    let next_before = (records.len() == limit)
        .then(|| records.last().map(|r| r.seq))
        .flatten();
    Ok(RecordsPage {
        journal: id.to_string(),
        total,
        records,
        next_before,
        price_scale,
        qty_scale,
        undecodable,
    })
}

/// Cash to its last place, as the venue writes it: a funding line is
/// compared to the venue's own figure, and a float would not match it.
fn cash_text(c: oq_types::Cash) -> String {
    let sign = if c.0 < 0 { "-" } else { "" };
    let v = c.0.unsigned_abs();
    format!("{sign}{}.{:08}", v / 100_000_000, v % 100_000_000)
}
