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
    /// Journal inspection: what a live process decided, in order.
    pub journal: Capability,
    /// Gap attribution: the decomposition of live minus model.
    pub attribution: Capability,
    /// Live metrics and alert conditions.
    pub live: Capability,
    /// Whether this deck may change anything at all.
    pub writes: Capability,
}

/// Work out what is available from what is on disk and configured.
#[must_use]
pub fn detect(runs_dir: Option<&Path>, writes_allowed: bool) -> Capabilities {
    let runs = match runs_dir {
        Some(dir) if dir.is_dir() => Capability::on(),
        Some(dir) => Capability::off(format!("{} is not a directory", dir.display())),
        None => Capability::off("no runs directory configured; set OQ_DECK_RUNS_DIR"),
    };

    Capabilities {
        version: env!("CARGO_PKG_VERSION"),
        runs,
        journal: Capability::off(
            "journal reading arrives in M1; it needs oq-journal linked and a \
             journal path configured",
        ),
        attribution: Capability::off(
            "attribution needs a live result to compare a run against, and the \
             deck has not been given an account to read",
        ),
        live: Capability::off(
            "live metrics are read from a running oq-trade session; none is \
             configured",
        ),
        writes: if writes_allowed {
            Capability::on()
        } else {
            Capability::off("this deck is read-only; enable writes in settings")
        },
    }
}
