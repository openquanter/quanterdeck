//! The road from a draft strategy to a live one.
//!
//! This is the part of the console that exists to say no. The stages
//! advance one at a time and never skip, and `advance` returns the reason
//! a step is unavailable so the interface can show it rather than
//! present a disabled control with nothing attached.
//!
//! # Why the evidence is a run id
//!
//! A backtest justifies the configuration it ran under, and nothing else.
//! Deciding whether that is still the configuration in front of you needs
//! an answer to "did anything move", and the framework already computes
//! one: a run file carries `config-sha256` and `data-sha256`, and
//! `RunManifest::compare` says what a difference between two of them
//! means. So the gate holds the run's identity and asks that question
//! rather than hashing the settings itself — a second opinion about what
//! counts as a change is a second opinion that will eventually disagree.

use oq_parity::manifest::{BaselineStatus, RunManifest};
use serde::{Deserialize, Serialize};

/// Where an instance sits on the road to live trading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Draft,
    Backtested,
    /// Paper or testnet.
    Observing,
    /// A human signed off.
    Confirmed,
    Live,
}

impl Stage {
    /// The pipeline, in order.
    pub const ORDER: [Self; 5] = [
        Self::Draft,
        Self::Backtested,
        Self::Observing,
        Self::Confirmed,
        Self::Live,
    ];

    /// The next stage, or `None` at the end.
    #[must_use]
    pub fn next(self) -> Option<Self> {
        let index = Self::ORDER.iter().position(|s| *s == self)?;
        Self::ORDER.get(index + 1).copied()
    }
}

/// Hours of paper or testnet running required before a sign-off.
///
/// Configurable per deployment. Never zero: an observation window of no
/// length is a checkbox, and a checkbox is what this whole mechanism
/// exists instead of.
pub const DEFAULT_OBSERVATION_HOURS: i64 = 72;

/// What the console knows about one instance's progress.
#[derive(Debug, Clone, Default)]
pub struct Evidence {
    /// The run that justified these settings.
    pub backtest_run: Option<String>,
    pub backtest_passed: bool,
    /// Identity of that run, kept so a later configuration change can be
    /// detected rather than assumed absent.
    pub backtest_manifest: Option<RunManifest>,
    pub observation_hours: i64,
    pub observation_fills: usize,
    pub confirmed_by: Option<String>,
}

/// Whether one step forward is allowed, and if not, why not.
#[derive(Debug, Clone, Serialize)]
pub struct Decision {
    pub allowed: bool,
    pub reason: String,
}

impl Decision {
    fn yes() -> Self {
        Self {
            allowed: true,
            reason: String::new(),
        }
    }

    fn no(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: reason.into(),
        }
    }
}

/// Whether this instance may take exactly one step forward.
#[must_use]
pub fn advance(
    current: Stage,
    evidence: &Evidence,
    observation_hours: i64,
    min_fills: usize,
) -> Decision {
    let Some(target) = current.next() else {
        return Decision::no("already live");
    };

    match target {
        Stage::Backtested => {
            let Some(run) = evidence.backtest_run.as_ref() else {
                return Decision::no("no backtest has been run for this configuration");
            };
            if !evidence.backtest_passed {
                return Decision::no(format!(
                    "run {run} did not pass; read the result before advancing"
                ));
            }
            Decision::yes()
        }
        Stage::Observing => Decision::yes(),
        Stage::Confirmed => {
            if evidence.observation_hours < observation_hours {
                let remaining = observation_hours - evidence.observation_hours;
                return Decision::no(format!(
                    "{remaining}h of the {observation_hours}h observation window remain"
                ));
            }
            if evidence.observation_fills < min_fills {
                return Decision::no(format!(
                    "observation produced {} fills; at least {min_fills} is required \
                     to say the strategy did anything at all",
                    evidence.observation_fills
                ));
            }
            Decision::yes()
        }
        Stage::Live => {
            if evidence.confirmed_by.is_none() {
                return Decision::no("nobody has signed off");
            }
            Decision::yes()
        }
        Stage::Draft => Decision::no("draft is the first stage"),
    }
}

/// Whether a configuration change has voided the evidence behind a stage.
///
/// A backtest justifies the configuration it ran under. If the effective
/// configuration moves, the instance goes back to draft — including one
/// already live, whose operator then walks it through again. That is the
/// intended cost, and it is the reason the identity triple exists.
///
/// A code change alone does not void it: `BaselineStatus::CodeChanged` is
/// the case a parity run is *for*, and treating it as invalidation would
/// send every instance back to draft on every deploy.
#[must_use]
pub fn voided_by(evidence: &Evidence, current: &RunManifest) -> Option<Vec<String>> {
    let baseline = evidence.backtest_manifest.as_ref()?;
    match baseline.compare(current) {
        BaselineStatus::Comparable | BaselineStatus::CodeChanged => None,
        BaselineStatus::Invalidated { changed } => {
            Some(changed.iter().map(|e| e.explanation().to_owned()).collect())
        }
    }
}
