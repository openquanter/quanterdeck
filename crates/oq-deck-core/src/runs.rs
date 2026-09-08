//! The read side of a run, as the console needs it.
//!
//! Nothing here parses a run file. `oq_parity::wire` owns that format and
//! owns the judgement about what a malformed one means — a truncated file
//! is rejected rather than half-read, and a version this build does not
//! know is rejected rather than partially understood. Re-implementing any
//! of that here would produce a console that disagreed with the tool that
//! wrote the file, which is worse than a console that cannot read it.
//!
//! What this module adds is the two things a directory of runs needs and
//! a single-file parser does not: a listing that survives one bad file,
//! and a shape that serialises.

use std::fs;
use std::path::{Path, PathBuf};

use oq_parity::diff::{ParityReport, compare};
use oq_parity::manifest::{BaselineStatus, RunManifest};
use oq_parity::record::RunOutput;
use oq_parity::wire::Run;
use serde::Serialize;

/// A run's identity, flattened for transport.
#[derive(Debug, Clone, Serialize)]
pub struct Identity {
    pub code_commit: String,
    pub data_hash: String,
    pub config_hash: String,
    /// Not part of identity, which is why it is named separately here
    /// rather than sitting among the three hashes as a fourth field.
    pub label: String,
}

impl From<&RunManifest> for Identity {
    fn from(manifest: &RunManifest) -> Self {
        Self {
            code_commit: manifest.code_commit.clone(),
            data_hash: manifest.data_hash.clone(),
            config_hash: manifest.config_hash.clone(),
            label: manifest.label.clone(),
        }
    }
}

/// One run in a listing.
#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub id: String,
    pub path: String,
    pub identity: Identity,
    pub pnl: f64,
    pub fills: usize,
}

/// A file in the runs directory, whether or not it read.
///
/// A file that will not parse is a finding, not an omission. Dropping it
/// from the listing would leave an operator looking at a directory of
/// twelve files and a page showing eleven, with nothing to say which one
/// went missing or why.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Entry {
    Read(RunSummary),
    Unreadable {
        id: String,
        path: String,
        error: String,
    },
}

/// One fill, flattened for transport.
#[derive(Debug, Clone, Serialize)]
pub struct FillRow {
    pub ts: i64,
    pub symbol: String,
    pub side: String,
    pub price_ticks: i64,
    pub qty_lots: i64,
    /// `None` and `Some("")` are different things in the run format and
    /// stay different here: a fill with no tag versus one tagged with the
    /// empty string.
    pub tag: Option<String>,
}

/// A run with its fills.
#[derive(Debug, Clone, Serialize)]
pub struct RunDetail {
    #[serde(flatten)]
    pub summary: RunSummary,
    pub fills: Vec<FillRow>,
}

fn summarise(id: &str, path: &Path, run: &Run) -> RunSummary {
    RunSummary {
        id: id.to_owned(),
        path: path.display().to_string(),
        identity: Identity::from(&run.manifest),
        pnl: run.output.pnl,
        fills: run.output.fills.len(),
    }
}

fn read_run(path: &Path) -> Result<Run, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    Run::parse(&text).map_err(|e| e.to_string())
}

/// Every run file in a directory, newest name last.
///
/// A missing directory is an empty listing, not an error: a deck pointed
/// at a runs directory that does not exist yet is a deck nobody has run
/// anything with, and that is a normal state on the first day.
#[must_use]
pub fn list(dir: &Path) -> Vec<Entry> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "run"))
        .collect();
    paths.sort();

    paths
        .iter()
        .map(|path| {
            let id = path
                .file_stem()
                .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
            match read_run(path) {
                Ok(run) => Entry::Read(summarise(&id, path, &run)),
                Err(error) => Entry::Unreadable {
                    id,
                    path: path.display().to_string(),
                    error,
                },
            }
        })
        .collect()
}

/// Resolve a run id to a path inside `dir`, refusing anything else.
///
/// The id arrives from a URL. Treating it as a path fragment would let
/// `../../etc/passwd` out of the directory, so it is matched against the
/// listing instead of joined onto it.
#[must_use]
pub fn resolve(dir: &Path, id: &str) -> Option<PathBuf> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        return None;
    }
    let candidate = dir.join(format!("{id}.run"));
    candidate.is_file().then_some(candidate)
}

/// One run, with its fills.
///
/// # Errors
/// The file is missing, unreadable, or not a run file this build knows.
pub fn detail(dir: &Path, id: &str) -> Result<RunDetail, String> {
    let path = resolve(dir, id).ok_or_else(|| format!("no run named {id}"))?;
    let run = read_run(&path)?;
    let summary = summarise(id, &path, &run);
    let fills = run
        .output
        .fills
        .iter()
        .map(|fill| FillRow {
            ts: fill.ts.0,
            symbol: fill.symbol.clone(),
            side: format!("{:?}", fill.side).to_lowercase(),
            price_ticks: fill.price.0,
            qty_lots: fill.qty.0,
            tag: fill.tag.clone(),
        })
        .collect();
    Ok(RunDetail { summary, fills })
}

/// How a baseline status renders for a reader.
#[derive(Debug, Clone, Serialize)]
pub struct Verdict {
    /// `comparable` | `code_changed` | `invalidated`
    pub status: String,
    /// Whether anything about behaviour may be concluded from the diff.
    pub conclusive: bool,
    /// For an invalidated baseline, what moved and what to do about it.
    pub changed: Vec<String>,
}

impl From<&BaselineStatus> for Verdict {
    fn from(status: &BaselineStatus) -> Self {
        let conclusive = status.permits_behavioral_conclusions();
        match status {
            BaselineStatus::Comparable => Self {
                status: "comparable".into(),
                conclusive,
                changed: Vec::new(),
            },
            BaselineStatus::CodeChanged => Self {
                status: "code_changed".into(),
                conclusive,
                changed: Vec::new(),
            },
            BaselineStatus::Invalidated { changed } => Self {
                status: "invalidated".into(),
                conclusive,
                changed: changed.iter().map(|e| e.explanation().to_owned()).collect(),
            },
        }
    }
}

/// The result of comparing two runs, as the console shows it.
#[derive(Debug, Clone, Serialize)]
pub struct Comparison {
    pub baseline: String,
    pub candidate: String,
    pub verdict: Verdict,
    pub differences: usize,
    pub first_divergence: Option<usize>,
    pub matched_prefix: usize,
    pub fill_counts: (usize, usize),
    pub pnl: (f64, f64),
    pub pnl_relative_error: Option<f64>,
    /// Whether this passes at the tolerance the caller asked for.
    ///
    /// False for an invalidated baseline, because "we cannot tell" is not
    /// "they agree" — the distinction `ParityReport::passes` already
    /// makes, carried through rather than flattened into a boolean the
    /// UI would colour green.
    pub passes: bool,
}

fn build(
    baseline_id: &str,
    candidate_id: &str,
    report: &ParityReport,
    tolerance: f64,
) -> Comparison {
    Comparison {
        baseline: baseline_id.to_owned(),
        candidate: candidate_id.to_owned(),
        verdict: Verdict::from(&report.baseline_status),
        differences: report.differences.len(),
        first_divergence: report.first_divergence,
        matched_prefix: report.matched_prefix,
        fill_counts: report.fill_counts,
        pnl: report.pnl,
        pnl_relative_error: report.pnl_relative_error,
        passes: report.passes(tolerance),
    }
}

/// Compare two runs in a directory.
///
/// # Errors
/// Either run is missing or will not read.
pub fn compare_ids(
    dir: &Path,
    baseline_id: &str,
    candidate_id: &str,
    tolerance: f64,
) -> Result<Comparison, String> {
    let baseline = read_one(dir, baseline_id)?;
    let candidate = read_one(dir, candidate_id)?;
    let report = compare(
        &baseline.manifest,
        &baseline.output,
        &candidate.manifest,
        &candidate.output,
    );
    Ok(build(baseline_id, candidate_id, &report, tolerance))
}

fn read_one(dir: &Path, id: &str) -> Result<Run, String> {
    let path = resolve(dir, id).ok_or_else(|| format!("no run named {id}"))?;
    read_run(&path)
}

/// Sum of realized P&L across the readable runs in a listing.
#[must_use]
pub fn total_pnl(entries: &[Entry]) -> f64 {
    entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::Read(summary) => Some(summary.pnl),
            Entry::Unreadable { .. } => None,
        })
        .sum()
}

/// Build a run output, for tests and fixtures.
#[must_use]
pub fn empty_output() -> RunOutput {
    RunOutput::new(Vec::new(), 0.0)
}
