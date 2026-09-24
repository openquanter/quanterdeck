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
