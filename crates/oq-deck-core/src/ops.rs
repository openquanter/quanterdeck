//! What the deck asks the host agent, and how risky each request is.
//!
//! The deck is the interface; the agent, running on the trading host as
//! its own user, is what touches systemd, the trading process's control
//! port and the release directory. They speak one JSON line each way over
//! a Unix socket. This module is that line, shared so neither side can
//! drift from the other.
//!
//! # Where authority sits
//!
//! The deck authenticates people. The agent decides what is allowed, and
//! for anything [`Risk::High`] it checks a one-time code against a secret
//! only it holds. A compromised deck can therefore ask for status and for
//! a halt — both of which only reduce what the process does — and for
//! nothing else without a person's code.

use serde::{Deserialize, Serialize};

/// One request to the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRequest {
    pub op: Op,
    /// Who asked, as the deck authenticated them.
    pub actor: String,
    /// Unique per request; the agent refuses one it has seen.
    pub nonce: String,
    /// Unix milliseconds after which the agent refuses the request.
    pub expires_ms: i64,
    /// Why, for anything that changes state.
    #[serde(default)]
    pub reason: Option<String>,
    /// The step-up code, for [`Risk::High`] requests.
    #[serde(default)]
    pub step_up: Option<String>,
}

/// What is asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Op {
    /// Load, memory, disks, clock.
    Host,
    /// The managed units' state.
    Units,
    /// The trading process's own status, through its control port.
    Status,
    /// The orders it believes resting.
    Orders,
    /// Its Prometheus text.
    Metrics,
    /// The gap between what the venue made and what its shadow backtest
    /// made, decomposed, from the trader's own evidence.
    Attribution,
    /// Stop opening, withdraw opening orders.
    Halt,
    /// Withdraw everything and exit.
    Shutdown,
    /// Clear the kill switch.
    Resume,
    /// Start, stop or restart a managed unit.
    Unit { unit: String, verb: String },
    /// The log files there are.
    Logs,
    /// The last lines of one, optionally only those containing `grep`.
    LogTail {
        name: String,
        lines: usize,
        #[serde(default)]
        grep: Option<String>,
    },
    /// The agent's audit trail, newest last, and whether its chain holds.
    Audit { lines: usize },
    /// Releases staged and installed, and which one is running.
    Releases,
    /// Verify a staged release and switch the trader to it.
    Deploy { id: String },
    /// Switch back to the release that ran before the current one.
    Rollback,
    /// Alerts raised and not yet cleared, and the recent history.
    Alerts,
    /// Post a test message to the alert channel.
    AlertTest,
    /// Stop notifying about one alert for a while.
    AlertSilence { key: String, minutes: i64 },
    /// Which account each process is using, by key fingerprint.
    Accounts,
    /// Each unit's memory and CPU over the last `hours`, from the agent's
    /// own samples.
    Resources { hours: i64 },
    /// The black box between two times: every event, and each series
    /// thinned to about `points`.
    Blackbox {
        from_ms: i64,
        to_ms: i64,
        points: usize,
    },
    /// What everything looked like at one moment.
    BlackboxAt { at_ms: i64 },
    /// A unit's output from the systemd journal, with its timestamps.
    JournalLog {
        unit: String,
        since_ms: Option<i64>,
        until_ms: Option<i64>,
        lines: usize,
        #[serde(default)]
        grep: Option<String>,
    },
    /// The strategy config files.
    ConfigList,
    /// One config file, or one of its backups.
    ConfigGet {
        name: String,
        #[serde(default)]
        backup: Option<String>,
    },
    /// Replace a config file, if it is still what was read.
    ConfigPut {
        name: String,
        content: String,
        base_sha: String,
    },
    /// Put a backup back.
    ConfigRollback {
        name: String,
        backup: String,
        base_sha: String,
    },
    /// Strategy instances and where each stands on the road to live.
    Strategies,
    /// A new instance, in draft.
    StrategyCreate { name: String, config: String },
    /// Attach the backtest that justifies an instance's configuration.
    StrategyBacktest {
        id: String,
        run: String,
        passed: bool,
    },
    /// One step forward, if the gate allows it.
    StrategyAdvance { id: String },
}

/// How much harm a request can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    /// Changes nothing.
    Read,
    /// Makes the process do less; needs a reason, no step-up. Kept cheap
    /// on purpose: when something is wrong, stopping has to be fast.
    Reduce,
    /// Can make it trade, change what runs, or take protection away;
    /// needs a reason and a step-up code the agent verifies.
    High,
}

impl Op {
    #[must_use]
    pub fn risk(&self) -> Risk {
        match self {
            Self::Host
            | Self::Units
            | Self::Status
            | Self::Orders
            | Self::Metrics
            | Self::Attribution
            | Self::Logs
            | Self::LogTail { .. }
            | Self::Audit { .. }
            | Self::Releases
            | Self::Alerts
            | Self::Accounts
            | Self::Resources { .. }
            | Self::Blackbox { .. }
            | Self::BlackboxAt { .. }
            | Self::JournalLog { .. }
            | Self::ConfigList
            | Self::ConfigGet { .. }
            | Self::Strategies => Risk::Read,
            Self::Halt | Self::AlertTest | Self::AlertSilence { .. } => Risk::Reduce,
            // Shutdown withdraws the take-profits too, leaving the position
            // unprotected and unmanaged: not a risk reduction.
            Self::Shutdown
            | Self::Resume
            | Self::Unit { .. }
            | Self::Deploy { .. }
            | Self::Rollback
            | Self::ConfigPut { .. }
            | Self::ConfigRollback { .. }
            | Self::StrategyCreate { .. }
            | Self::StrategyBacktest { .. }
            | Self::StrategyAdvance { .. } => Risk::High,
        }
    }

    /// A short name for audit lines and messages.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Unit { unit, verb } => format!("{verb} {unit}"),
            Self::LogTail { name, .. } => format!("log {name}"),
            Self::Deploy { id } => format!("deploy {id}"),
            Self::ConfigPut { name, .. } => format!("config write {name}"),
            Self::ConfigRollback { name, backup, .. } => {
                format!("config rollback {name} to {backup}")
            }
            Self::ConfigGet { name, .. } => format!("config read {name}"),
            Self::StrategyCreate { name, .. } => format!("strategy create {name}"),
            Self::StrategyBacktest { id, run, passed } => {
                format!("strategy {id} backtest {run} passed={passed}")
            }
            Self::StrategyAdvance { id } => format!("strategy {id} advance"),
            Self::AlertSilence { key, minutes } => format!("silence {key} {minutes}m"),
            Self::Audit { .. } => "audit".into(),
            other => serde_json::to_value(other)
                .ok()
                .and_then(|v| v.get("kind").and_then(|k| k.as_str()).map(str::to_string))
                .unwrap_or_default(),
        }
    }
}

/// The agent's answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentResponse {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub data: serde_json::Value,
}

impl AgentResponse {
    #[must_use]
    pub fn ok(data: serde_json::Value) -> Self {
        Self {
            ok: true,
            error: None,
            data,
        }
    }

    #[must_use]
    pub fn refused(why: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(why.into()),
            data: serde_json::Value::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_survives_the_wire() {
        let r = AgentRequest {
            op: Op::Unit {
                unit: "oqp-live.service".into(),
                verb: "restart".into(),
            },
            actor: "deck:admin".into(),
            nonce: "n1".into(),
            expires_ms: 5,
            reason: Some("because".into()),
            step_up: Some("123456".into()),
        };
        let line = serde_json::to_string(&r).expect("encodes");
        assert!(line.contains(r#""kind":"unit""#), "{line}");
        assert_eq!(
            serde_json::from_str::<AgentRequest>(&line).expect("decodes"),
            r
        );
    }

    #[test]
    fn only_reads_and_the_halt_go_without_a_step_up() {
        assert_eq!(Op::Halt.risk(), Risk::Reduce);
        for op in [
            Op::Shutdown,
            Op::Resume,
            Op::Rollback,
            Op::Deploy { id: "x".into() },
            Op::Unit {
                unit: "u".into(),
                verb: "stop".into(),
            },
        ] {
            assert_eq!(op.risk(), Risk::High, "{op:?}");
        }
        assert_eq!(Op::Status.risk(), Risk::Read);
        assert_eq!(Op::Halt.describe(), "halt");
    }
}
