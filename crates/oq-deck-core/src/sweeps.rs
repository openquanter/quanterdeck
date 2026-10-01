//! The read side of a parameter sweep, as the console needs it.
//!
//! `oq_backtest::sweep_file` owns the `.sweep` format; this module lists a
//! directory of them and gives each a shape that serialises. It adds no
//! judgement of its own: whether a sweep may be deployed is the refusals
//! the file carries, written by the code that ran it.

use std::fs;
use std::path::{Path, PathBuf};

use oq_backtest::sweep_file::SweepFile;
use serde::Serialize;

/// A statistic, or why it could not be computed.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Stat<T> {
    Value { value: T },
    Missing { reason: String },
}

impl<T> From<Result<T, String>> for Stat<T> {
    fn from(r: Result<T, String>) -> Self {
        match r {
            Ok(value) => Self::Value { value },
            Err(reason) => Self::Missing { reason },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Pbo {
    pub pbo: f64,
    pub splits: usize,
    pub probability_of_loss: f64,
    pub median_oos_sharpe: f64,
    pub degradation: f64,
    pub logits: Vec<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Config {
    pub label: String,
    pub fills: usize,
    pub realized: f64,
    pub fees: f64,
    pub final_equity: f64,
    pub min_equity: f64,
    pub liquidations: usize,
    pub sharpe: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Thresholds {
    pub max_pbo: f64,
    pub min_deflated_sharpe: f64,
    pub min_degradation_slope: f64,
}

/// One sweep, whole.
#[derive(Debug, Clone, Serialize)]
pub struct Sweep {
    pub id: String,
    pub label: String,
    pub equity_every: usize,
    pub thresholds: Thresholds,
    pub deflated_sharpe: Stat<f64>,
    pub pbo: Stat<Pbo>,
    pub refusals: Vec<String>,
    pub configs: Vec<Config>,
    pub unscorable: Vec<String>,
    pub lookahead: Option<(String, String)>,
}

impl Sweep {
    fn from_file(id: &str, f: SweepFile) -> Self {
        Self {
            id: id.to_owned(),
            label: f.label,
            equity_every: f.equity_every,
            thresholds: Thresholds {
                max_pbo: f.thresholds.0,
                min_deflated_sharpe: f.thresholds.1,
                min_degradation_slope: f.thresholds.2,
            },
            deflated_sharpe: f.deflated_sharpe.into(),
            pbo: f
                .pbo
                .map(|p| Pbo {
                    pbo: p.pbo,
                    splits: p.splits,
                    probability_of_loss: p.probability_of_loss,
                    median_oos_sharpe: p.median_oos_sharpe,
                    degradation: p.degradation,
                    logits: p.logits,
                })
                .into(),
            refusals: f.refusals,
            configs: f
                .configs
                .into_iter()
                .map(|c| Config {
                    label: c.label,
                    fills: c.fills,
                    realized: c.realized,
                    fees: c.fees,
                    final_equity: c.final_equity,
                    min_equity: c.min_equity,
                    liquidations: c.liquidations,
                    sharpe: c.sharpe,
                })
                .collect(),
            unscorable: f.unscorable,
            lookahead: f.lookahead,
        }
    }
}

/// A sweep in a listing: enough to choose one.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Entry {
    Read {
        id: String,
        label: String,
        configs: usize,
        refused: bool,
        modified_ms: Option<u64>,
    },
    /// A file that is there and could not be read, listed rather than
    /// hidden: a sweep that vanished from the page would look like one
    /// that was never run.
    Unreadable { id: String, error: String },
}

fn read_file(path: &Path) -> Result<SweepFile, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    SweepFile::parse(&text)
}

fn modified_ms(path: &Path) -> Option<u64> {
    let t = fs::metadata(path).ok()?.modified().ok()?;
    let d = t.duration_since(std::time::UNIX_EPOCH).ok()?;
    u64::try_from(d.as_millis()).ok()
}

/// What a listing shows of a sweep that read.
#[derive(Debug, Clone)]
struct Head {
    label: String,
    configs: usize,
    refused: bool,
}

/// Listing heads already worked out, for files that have not changed.
#[derive(Debug, Default)]
pub struct Cache(crate::cache::ParseCache<Head>);

impl Cache {
    /// How many sweeps are remembered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether no sweep is remembered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Every `.sweep` file in a directory, newest first.
///
/// # Errors
/// The directory exists and cannot be read. A missing one is an empty
/// listing: nobody has written a sweep there yet.
pub fn list(dir: &Path) -> Result<Vec<Entry>, String> {
    list_cached(dir, &Cache::default())
}

/// [`list`], remembering each sweep's head in `cache` while its file is
/// unchanged. The listing is the same either way; see [`crate::cache`].
///
/// # Errors
/// As [`list`].
pub fn list_cached(dir: &Path, cache: &Cache) -> Result<Vec<Entry>, String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            cache.0.retain(&[]);
            return Ok(Vec::new());
        }
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    let mut paths: Vec<(Option<u64>, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|e| crate::regular_with(e, "sweep"))
        .map(|e| e.path())
        .map(|p| (modified_ms(&p), p))
        .collect();
    paths.sort_by(|a, b| b.cmp(a));
    let present: Vec<PathBuf> = paths.iter().map(|(_, p)| p.clone()).collect();
    cache.0.retain(&present);
    Ok(paths
        .into_iter()
        .map(|(modified, path)| {
            let id = path
                .file_stem()
                .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
            let head = cache.0.get_or_parse(&path, |path| {
                read_file(path).map(|f| Head {
                    label: f.label,
                    configs: f.configs.len(),
                    refused: !f.refusals.is_empty(),
                })
            });
            match head {
                Ok(head) => Entry::Read {
                    id,
                    label: head.label,
                    configs: head.configs,
                    refused: head.refused,
                    modified_ms: modified,
                },
                Err(error) => Entry::Unreadable { id, error },
            }
        })
        .collect())
}

/// One sweep by id, matched against the listing rather than joined onto
/// the directory, so an id from a URL cannot leave it.
///
/// # Errors
/// No such sweep, or it does not read.
pub fn detail(dir: &Path, id: &str) -> Result<Sweep, String> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(format!("no sweep named {id}"));
    }
    let path =
        crate::listed(dir, &format!("{id}.sweep")).ok_or_else(|| format!("no sweep named {id}"))?;
    Ok(Sweep::from_file(id, read_file(&path)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFUSED: &str = "openquanter-sweep 1\nlabel ma calm\nequity-every 50\nthresholds 0.35 0.95 0\n\
        deflated-sharpe 0.41\npbo 0.5 16 0.6 -0.2 -0.8\nlogits -1.2 0.4 0.1\n\
        refusal PBO 0.50 exceeds 0.35\nconfig fast=5 slow=20\t12\t3.5\t0.2\t1003.3\t990\t0\t0.8\n\
        config fast=10 slow=40\t4\t-1\t0.1\t998.9\t995\t0\t-\nunscorable fast=1 slow=2\n";

    #[test]
    fn a_directory_lists_every_sweep_including_one_that_does_not_read() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("a.sweep"), REFUSED).expect("write");
        fs::write(dir.path().join("b.sweep"), "openquanter-run 1\n").expect("write");
        fs::write(dir.path().join("c.run"), "not a sweep").expect("write");
        let listing = list(dir.path()).expect("lists");
        assert_eq!(listing.len(), 2, "{listing:?}");
        assert!(
            listing.iter().any(
                |e| matches!(e, Entry::Read { id, refused: true, configs: 2, .. } if id == "a")
            )
        );
        assert!(
            listing
                .iter()
                .any(|e| matches!(e, Entry::Unreadable { id, .. } if id == "b"))
        );
        assert!(list(&dir.path().join("missing")).expect("empty").is_empty());
    }

    #[test]
    fn a_cached_listing_follows_a_rewrite_and_a_removal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("a.sweep");
        let settle = || {
            fs::File::options()
                .write(true)
                .open(&path)
                .expect("open")
                .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(600))
                .expect("set mtime");
        };
        fs::write(&path, REFUSED).expect("write");
        settle();
        let cache = Cache::default();
        let first = list_cached(dir.path(), &cache).expect("lists");
        assert!(matches!(&first[0], Entry::Read { refused: true, .. }));
        assert_eq!(cache.len(), 1);

        // The refusal line taken out: the same sweep, now deployable.
        let accepted: String = REFUSED
            .lines()
            .filter(|l| !l.starts_with("refusal "))
            .map(|l| format!("{l}\n"))
            .collect();
        fs::write(&path, accepted).expect("rewrite");
        settle();
        let second = list_cached(dir.path(), &cache).expect("lists");
        assert!(
            matches!(&second[0], Entry::Read { refused: false, .. }),
            "{second:?}"
        );

        fs::remove_file(&path).expect("remove");
        assert!(list_cached(dir.path(), &cache).expect("lists").is_empty());
        assert!(cache.is_empty());
    }

    #[test]
    fn a_sweep_reads_whole_and_an_id_cannot_leave_the_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("a.sweep"), REFUSED).expect("write");
        let s = detail(dir.path(), "a").expect("reads");
        assert_eq!(s.configs.len(), 2);
        assert_eq!(s.configs[1].sharpe, None);
        assert_eq!(s.refusals, vec!["PBO 0.50 exceeds 0.35".to_string()]);
        assert!(matches!(&s.pbo, Stat::Value { value } if value.logits.len() == 3));
        assert!(detail(dir.path(), "../a").is_err());
        assert!(detail(dir.path(), "nope").is_err());
    }
}
