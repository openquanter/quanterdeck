//! What this deck can actually see.
//!
//! The interface renders its navigation from this rather than from a
//! fixed list, so a capability that is off produces no control at all
//! instead of one that fails when pressed. Reporting a capability the
//! deck cannot back is therefore not optimism, it is a broken button.

use std::path::Path;

use serde::Serialize;

/// One capability, and when it is off, why.
#[derive(Debug, Clone, Serialize)]
pub struct Capability {
    pub available: bool,
    /// Empty when available. When not, this sentence is shown to the
    /// operator verbatim, so it says what is missing and not "disabled".
    pub reason: String,
}

impl Capability {
    #[must_use]
    pub fn on() -> Self {
        Self {
            available: true,
            reason: String::new(),
        }
    }

    #[must_use]
    pub fn off(reason: impl Into<String>) -> Self {
        Self {
            available: false,
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Capabilities {
    pub version: &'static str,
    /// Reading run files: listings, detail, parity comparison.
    pub runs: Capability,
    /// Journal inspection and reconciliation against a venue record.
    pub live: Capability,
    /// Gap attribution: the decomposition of live minus model.
    ///
    /// Needs both directories: the decomposition compares a live run
    /// against a model run, and names the journal the live one came from.
    pub attribution: Capability,
    /// Whether this deck may change anything at all.
    pub writes: Capability,
}

fn directory(configured: Option<&Path>, variable: &str) -> Capability {
    match configured {
        Some(dir) if dir.is_dir() => Capability::on(),
        Some(dir) => Capability::off(format!("{} 不是一个目录", dir.display())),
        None => Capability::off(format!("尚未配置目录；请设置 {variable}")),
    }
}

/// Work out what is available from what is on disk and configured.
#[must_use]
pub fn detect(
    runs_dir: Option<&Path>,
    journals_dir: Option<&Path>,
    writes_allowed: bool,
) -> Capabilities {
    let runs = directory(runs_dir, "OQ_DECK_RUNS_DIR");
    let live = directory(journals_dir, "OQ_DECK_JOURNALS_DIR");

    Capabilities {
        version: env!("CARGO_PKG_VERSION"),
        attribution: if runs.available {
            Capability::on()
        } else {
            Capability::off("归因要把一次实盘 run 与一次模型 run 相比；请先配置 OQ_DECK_RUNS_DIR")
        },
        runs,
        live,
        writes: if writes_allowed {
            Capability::on()
        } else {
            Capability::off("本 deck 处于只读模式；要修改任何东西，请先在设置中开启写入")
        },
    }
}
