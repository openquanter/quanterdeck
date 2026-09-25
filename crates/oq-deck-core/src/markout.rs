//! Where the price went after each fill, for two runs side by side.
//!
//! The computation is `oq_parity::markout`'s; this module finds the files
//! and flattens the answer for transport. A run's fills are priced against
//! a tick file the operator picks, because a run records what it traded
//! and not the market it traded in — and pricing a live run against a
//! different day's ticks is a comparison of nothing, which is why the
//! choice is the operator's and is shown beside the result.

use std::path::{Path, PathBuf};

use oq_parity::markout::{DEFAULT_HORIZONS, Markout, Point, contrast, markout};
use serde::Serialize;

/// One horizon, flattened. The statistics are absent, not zero, when too
/// few fills reached it.
#[derive(Debug, Clone, Serialize)]
pub struct HorizonView {
    pub seconds: i64,
    pub samples: usize,
    pub measured: bool,
    pub mean_bps: Option<f64>,
    pub median_bps: Option<f64>,
    pub p10_bps: Option<f64>,
    pub p90_bps: Option<f64>,
    pub adverse_share: Option<f64>,
}

impl From<&Markout> for HorizonView {
    fn from(m: &Markout) -> Self {
        let seconds = m.horizon().0 / 1_000_000_000;
        match m {
            Markout::Measured(d) => Self {
                seconds,
                samples: d.samples,
                measured: true,
                mean_bps: Some(d.mean_bps),
                median_bps: Some(d.median_bps),
                p10_bps: Some(d.p10_bps),
                p90_bps: Some(d.p90_bps),
                adverse_share: Some(d.adverse_share),
            },
            Markout::TooFew { samples, .. } => Self {
                seconds,
                samples: *samples,
                measured: false,
                mean_bps: None,
                median_bps: None,
                p10_bps: None,
                p90_bps: None,
                adverse_share: None,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RunMarkout {
    pub id: String,
    pub horizons: Vec<HorizonView>,
}

/// The candidate's mean against the baseline's, per horizon.
#[derive(Debug, Clone, Serialize)]
pub struct ContrastView {
    pub seconds: i64,
    /// `None` when either side had too few fills.
    pub difference_bps: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarkoutComparison {
    /// The tick file the fills were priced against.
    pub ticks: String,
    pub baseline: RunMarkout,
    pub candidate: RunMarkout,
    pub contrast: Vec<ContrastView>,
}

/// Every tick file in a directory, by id.
///
/// A missing directory is an empty listing, as for runs.
///
/// # Errors
/// Any other failure to read the directory.
pub fn list_ticks(dir: &Path) -> Result<Vec<String>, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    let mut ids: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "oqtk"))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    ids.sort();
    Ok(ids)
}

/// A tick file id resolved inside `dir`, and nowhere else.
#[must_use]
pub fn resolve_ticks(dir: &Path, id: &str) -> Option<PathBuf> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        return None;
    }
    crate::listed(dir, &format!("{id}.oqtk"))
}

fn measure(fills: &[oq_parity::Fill], ticks: &Path) -> Result<Vec<Markout>, String> {
    let reader = oq_data::ticks::TickReader::open(ticks).map_err(|e| e.to_string())?;
    let mut failed = None;
    // Stops at the first bad record rather than skipping it: the prices
    // after one are prices nobody has shown to be right.
    let path = reader.map_while(|r| match r {
        Ok(t) => Some(Point {
            at: oq_parity::Nanos(t.stamp.exch.0),
            price: t.last,
        }),
        Err(e) => {
            failed = Some(e.to_string());
            None
        }
    });
    let out = markout(fills, path, &DEFAULT_HORIZONS);
    failed.map_or(Ok(out), Err)
}

/// Both runs' markouts against one tick file, and the difference.
///
/// # Errors
/// A run or the tick file is missing or will not read.
pub fn compare(
    runs_dir: &Path,
    ticks_dir: &Path,
    baseline: &str,
    candidate: &str,
    ticks: &str,
) -> Result<MarkoutComparison, String> {
    let path =
        resolve_ticks(ticks_dir, ticks).ok_or_else(|| format!("no tick file named {ticks}"))?;
    let a = crate::runs::read(runs_dir, baseline)?;
    let b = crate::runs::read(runs_dir, candidate)?;
    let ma = measure(&a.output.fills, &path).map_err(|e| format!("{ticks}: {e}"))?;
    let mb = measure(&b.output.fills, &path).map_err(|e| format!("{ticks}: {e}"))?;
    Ok(MarkoutComparison {
        ticks: ticks.to_owned(),
        contrast: contrast(&ma, &mb)
            .into_iter()
            .map(|(h, difference_bps)| ContrastView {
                seconds: h.0 / 1_000_000_000,
                difference_bps,
            })
            .collect(),
        baseline: RunMarkout {
            id: baseline.to_owned(),
            horizons: ma.iter().map(HorizonView::from).collect(),
        },
        candidate: RunMarkout {
            id: candidate.to_owned(),
            horizons: mb.iter().map(HorizonView::from).collect(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oq_parity::manifest::RunManifest;
    use oq_parity::wire::Run;
    use oq_parity::{Fill, RunOutput};
    use oq_types::Side;

    const S: i64 = 1_000_000_000;

    /// A directory of its own, removed when the guard drops. A name built
    /// from the pid and the clock was not unique: two tests in one process
    /// read the same microsecond, shared the directory, and the first to
    /// finish removed it under the other.
    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("dir")
    }

    fn ticks_file(dir: &Path) {
        let ticks: Vec<oq_engine::Tick> = (0..120)
            .map(|i| {
                let p = 10_000 + i;
                oq_engine::Tick::trades_only(oq_types::Stamp::new(i * S, i * S), p, p, p)
            })
            .collect();
        let bytes = oq_data::encode(1, &ticks);
        std::fs::write(dir.join("day.oqtk"), bytes).expect("ticks");
    }

    fn run_file(dir: &Path, id: &str, offset: i64) {
        let fills = (0..40)
            .map(|i| Fill::new(i * S / 10, "X", Side::Buy, 10_000 + offset, 1))
            .collect();
        let run = Run::new(
            RunManifest::from_content("c", b"d", b"g", "L0"),
            RunOutput::new(fills, 0.0),
        );
        std::fs::write(dir.join(format!("{id}.run")), run.render()).expect("run");
    }

    #[test]
    fn two_runs_are_priced_against_one_tick_file_and_contrasted() {
        let guard = dir();
        let d = guard.path();
        ticks_file(d);
        run_file(d, "model", 0);
        run_file(d, "live", 5);
        assert_eq!(list_ticks(d).expect("listed"), vec!["day".to_string()]);
        let c = compare(d, d, "model", "live", "day").expect("compared");
        assert_eq!(c.baseline.horizons.len(), 3);
        assert!(
            c.baseline
                .horizons
                .iter()
                .all(|h| h.measured && h.samples == 40)
        );
        let one = &c.contrast[0];
        assert_eq!(one.seconds, 1);
        assert!(
            (one.difference_bps.expect("measured") + 5.0).abs() < 0.01,
            "{one:?}"
        );
    }

    #[test]
    fn a_tick_file_is_resolved_inside_its_directory_only() {
        let guard = dir();
        let d = guard.path();
        assert!(resolve_ticks(d, "../etc/passwd").is_none());
        assert!(compare(d, d, "a", "b", "missing").is_err());
    }
}
