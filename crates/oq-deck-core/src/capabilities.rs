//! What this deck can actually see.
//!
//! The interface renders its navigation from this rather than from a
//! fixed list, so a capability that is off produces no control at all
//! instead of one that fails when pressed. Reporting a capability the
//! deck cannot back is therefore not optimism, it is a broken button.

use std::path::Path;

use serde::Serialize;

use crate::lang::Lang;

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
    /// Needs the runs directory: the decomposition compares a live run
    /// against a model run, and both are run files.
    pub attribution: Capability,
    /// Markouts: where the price went after each fill, for two runs.
    ///
    /// Needs both directories: the fills are in run files and the prices
    /// they are marked against are in tick files.
    pub markout: Capability,
    /// Host operations through the agent: units, the trader's status and
    /// control, logs, releases, alerts, the audit trail.
    pub ops: Capability,
    /// Whether this deck may change anything at all.
    pub writes: Capability,
    /// Checking the framework's newest GitHub release against what runs.
    ///
    /// On means the deck will ask; whether the last ask succeeded is
    /// the check's own report, not this.
    pub upstream: Capability,
}

fn directory(configured: Option<&Path>, variable: &str, lang: Lang) -> Capability {
    match configured {
        // A directory that exists and cannot be read is not available:
        // reported as on, its pages showed "nothing here" for "could not
        // look".
        Some(dir) if dir.is_dir() => match std::fs::read_dir(dir) {
            Ok(_) => Capability::on(),
            Err(e) => Capability::off(lang.pick(
                format!("{} 无法读取：{e}", dir.display()),
                format!("{} cannot be read: {e}", dir.display()),
            )),
        },
        Some(dir) => Capability::off(lang.pick(
            format!("{} 不是一个目录", dir.display()),
            format!("{} is not a directory", dir.display()),
        )),
        None => Capability::off(lang.pick(
            format!("尚未配置目录；请设置 {variable}"),
            format!("No directory is configured; set {variable}"),
        )),
    }
}

/// Work out what is available from what is on disk and configured.
#[must_use]
pub fn detect(
    runs_dir: Option<&Path>,
    journals_dir: Option<&Path>,
    ticks_dir: Option<&Path>,
    agent_socket: Option<&Path>,
    writes_allowed: bool,
    upstream_every_hours: u64,
    lang: Lang,
) -> Capabilities {
    let ops = match agent_socket {
        Some(sock) => {
            use std::os::unix::fs::FileTypeExt;
            match std::fs::metadata(sock) {
                Ok(m) if m.file_type().is_socket() => Capability::on(),
                Ok(_) => Capability::off(lang.pick(
                    format!("{} 不是 socket", sock.display()),
                    format!("{} is not a socket", sock.display()),
                )),
                Err(e) => Capability::off(lang.pick(
                    format!("连不上主机代理 {}：{e}", sock.display()),
                    format!("Cannot reach the host agent at {}: {e}", sock.display()),
                )),
            }
        }
        None => Capability::off(lang.pick(
            "尚未配置主机代理；请设置 OQ_DECK_AGENT_SOCKET",
            "No host agent is configured; set OQ_DECK_AGENT_SOCKET",
        )),
    };
    let runs = directory(runs_dir, "OQ_DECK_RUNS_DIR", lang);
    let live = directory(journals_dir, "OQ_DECK_JOURNALS_DIR", lang);
    let ticks = directory(ticks_dir, "OQ_DECK_TICKS_DIR", lang);

    Capabilities {
        version: env!("CARGO_PKG_VERSION"),
        // Two sources: a pair of run files, or the trader's own shadow
        // through the agent — the stronger one.
        attribution: if runs.available || ops.available {
            Capability::on()
        } else {
            Capability::off(lang.pick(
                "归因要把实盘与模型相比：需要 run 文件目录（OQ_DECK_RUNS_DIR）或主机代理（OQ_DECK_AGENT_SOCKET）",
                "Attribution compares live with the model: it needs the run files directory (OQ_DECK_RUNS_DIR) or the host agent (OQ_DECK_AGENT_SOCKET)",
            ))
        },
        markout: match (runs.available, ticks.available) {
            (true, true) => Capability::on(),
            (false, _) => Capability::off(lang.pick(
                "markout 要读 run 文件里的成交；请先配置 OQ_DECK_RUNS_DIR",
                "Markouts read the fills in run files; configure OQ_DECK_RUNS_DIR first",
            )),
            (true, false) => Capability::off(lang.pick(
                format!("markout 要用 tick 文件给成交定价：{}", ticks.reason),
                format!("Markouts price fills from tick files: {}", ticks.reason),
            )),
        },
        runs,
        live,
        // Writes go through the agent: allowed but with no agent is
        // still nothing to write with.
        writes: match (writes_allowed, ops.available) {
            (true, true) => Capability::on(),
            (true, false) => Capability::off(lang.pick(
                format!("已设置 OQ_DECK_ALLOW_WRITES，但{}", ops.reason),
                format!("OQ_DECK_ALLOW_WRITES is set, but: {}", ops.reason),
            )),
            (false, _) => Capability::off(lang.pick(
                "本 deck 处于只读模式；设置 OQ_DECK_ALLOW_WRITES=1 才能操作",
                "This deck is read-only; set OQ_DECK_ALLOW_WRITES=1 to act",
            )),
        },
        ops,
        upstream: if upstream_every_hours > 0 {
            Capability::on()
        } else {
            Capability::off(lang.pick(
                "已关闭：OQ_DECK_UPSTREAM_CHECK_HOURS=0，deck 不会访问 GitHub",
                "Off: OQ_DECK_UPSTREAM_CHECK_HOURS=0, so the deck makes no request to GitHub",
            ))
        },
    }
}

#[cfg(test)]
mod writes {
    use super::{Lang, detect};

    /// Allowed is not available without an agent: every write goes
    /// through it, and a capability reported on is a promise that a route
    /// delivers.
    #[test]
    fn writes_are_not_offered_before_there_is_anything_to_write_with() {
        let caps = detect(None, None, None, None, true, 6, Lang::Zh);
        assert!(!caps.writes.available);
        assert!(
            caps.writes.reason.contains("主机代理"),
            "{}",
            caps.writes.reason
        );
        assert!(!caps.ops.available);
        let en = detect(None, None, None, None, true, 6, Lang::En);
        assert!(
            en.writes.reason.contains("host agent"),
            "{}",
            en.writes.reason
        );
    }

    /// Turned off, the check says so and says how it was turned off.
    #[test]
    fn the_upstream_check_reports_when_it_is_off() {
        assert!(
            detect(None, None, None, None, false, 6, Lang::En)
                .upstream
                .available
        );
        let off = detect(None, None, None, None, false, 0, Lang::En).upstream;
        assert!(!off.available);
        assert!(
            off.reason.contains("OQ_DECK_UPSTREAM_CHECK_HOURS=0"),
            "{}",
            off.reason
        );
    }
}
