//! Each black-box sample of the trader, checked against the one before.
//!
//! The trader reports cumulative counters, its positions and its P&L every
//! thirty seconds, and some of those numbers constrain each other: a
//! position does not change without a fill, realized P&L does not move
//! without a fill, a cumulative counter never goes down, and the net P&L
//! is realized less fees plus funding. A pair of samples that breaks one
//! of these is the trader's own books disagreeing with themselves —
//! something no single snapshot shows, and which the recording catches
//! for free because it already holds both.
//!
//! Each check answers in three states. Missing inputs — a field the
//! trader did not report, fees it does not know, a restart between the
//! samples — are `CannotTell`, never `Agree`: a check that could not run
//! has not passed.

use oq_deck_core::lang::Said;
use serde_json::{Value, json};

/// What one check, or all of them, concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Agree,
    Disagree,
    CannotTell,
}

impl Verdict {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Agree => "agree",
            Self::Disagree => "disagree",
            Self::CannotTell => "cannot_tell",
        }
    }
}

/// One check's answer, and why when it is not agreement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub name: &'static str,
    pub verdict: Verdict,
    pub why: Option<Said>,
}

fn said(zh: impl Into<String>, en: impl Into<String>) -> Option<Said> {
    Some(Said {
        zh: zh.into(),
        en: en.into(),
    })
}

fn agree(name: &'static str) -> Finding {
    Finding {
        name,
        verdict: Verdict::Agree,
        why: None,
    }
}

fn unknown(name: &'static str, zh: &str, en: &str) -> Finding {
    Finding {
        name,
        verdict: Verdict::CannotTell,
        why: said(zh, en),
    }
}

fn money(v: &Value) -> Option<f64> {
    v.as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| v.as_f64())
}

/// Positions as a comparable list: side and amount, in a fixed order.
fn positions(t: &Value) -> Option<Vec<(String, String)>> {
    let mut out: Vec<(String, String)> = t["positions"]
        .as_array()?
        .iter()
        .map(|p| {
            (
                p["side"].as_str().unwrap_or("").to_string(),
                // "0.010" and "0.01" are the same position.
                money(&p["amount"]).map_or_else(|| p["amount"].to_string(), |a| format!("{a}")),
            )
        })
        .filter(|(_, a)| a != "0")
        .collect();
    out.sort();
    Some(out)
}

/// Check `now` against `prev`, both the trader fields the black box keeps.
#[must_use]
pub fn check(prev: &Value, now: &Value) -> Vec<Finding> {
    if prev["pid"].is_null() || prev["pid"] != now["pid"] {
        // A new process starts its counters and its P&L again; comparing
        // across the restart would find every one of them "decreasing".
        return vec![unknown(
            "same_process",
            "两次采样之间交易进程重启了，计数从头开始，无法比较",
            "the trader restarted between the samples; its counters started again, so there is nothing to compare",
        )];
    }
    let fills = (
        prev["counters"]["fills"].as_u64(),
        now["counters"]["fills"].as_u64(),
    );
    vec![
        counters_monotonic(prev, now),
        moved_without_fill(
            "position_needs_fill",
            positions(prev).zip(positions(now)).map(|(a, b)| a != b),
            fills,
            ("持仓", "the position"),
        ),
        moved_without_fill(
            "realized_needs_fill",
            money(&prev["pnl"]["realized"])
                .zip(money(&now["pnl"]["realized"]))
                .map(|(a, b)| (a - b).abs() > 1e-9),
            fills,
            ("已实现盈亏", "realized P&L"),
        ),
        net_identity(now),
    ]
}

fn counters_monotonic(prev: &Value, now: &Value) -> Finding {
    const NAME: &str = "counters_monotonic";
    let (Some(a), Some(b)) = (prev["counters"].as_object(), now["counters"].as_object()) else {
        return unknown(
            NAME,
            "交易进程没有报告计数器",
            "the trader reported no counters",
        );
    };
    let fell: Vec<String> = a
        .iter()
        .filter_map(|(k, v)| {
            let (was, is) = (v.as_u64()?, b.get(k)?.as_u64()?);
            (is < was).then(|| format!("{k} {was} → {is}"))
        })
        .collect();
    if fell.is_empty() {
        agree(NAME)
    } else {
        let list = fell.join(", ");
        Finding {
            name: NAME,
            verdict: Verdict::Disagree,
            why: said(
                format!("同一进程的累计计数变小了：{list}"),
                format!("a cumulative counter went down within one process: {list}"),
            ),
        }
    }
}

fn moved_without_fill(
    name: &'static str,
    moved: Option<bool>,
    fills: (Option<u64>, Option<u64>),
    what: (&str, &str),
) -> Finding {
    let (Some(moved), (Some(was), Some(is))) = (moved, fills) else {
        return unknown(
            name,
            &format!("缺少{}或成交计数，无法核对", what.0),
            &format!(
                "{} or the fill count was not reported, so this cannot be checked",
                what.1
            ),
        );
    };
    if moved && is == was {
        Finding {
            name,
            verdict: Verdict::Disagree,
            why: said(
                format!(
                    "{}变了，但这段时间没有任何成交（成交计数停在 {is}）",
                    what.0
                ),
                format!(
                    "{} changed with no fill in between (the fill count stayed at {is})",
                    what.1
                ),
            ),
        }
    } else {
        agree(name)
    }
}

fn net_identity(now: &Value) -> Finding {
    const NAME: &str = "net_identity";
    let pnl = &now["pnl"];
    let (Some(realized), Some(funding)) = (money(&pnl["realized"]), money(&pnl["funding"])) else {
        return unknown(NAME, "交易进程没有报告盈亏", "the trader reported no P&L");
    };
    let (Some(fees), Some(net)) = (money(&pnl["fees"]), money(&pnl["net"])) else {
        return unknown(
            NAME,
            "手续费未知，净盈亏无从核对",
            "fees are unknown, so the net P&L cannot be checked",
        );
    };
    let expected = realized - fees + funding;
    // The figures arrive as decimal strings of a fixed-point amount; a
    // gap below a millionth of the larger is the printing, not the books.
    let tolerance = 1e-6 * (1.0 + expected.abs().max(net.abs()));
    if (net - expected).abs() <= tolerance {
        agree(NAME)
    } else {
        Finding {
            name: NAME,
            verdict: Verdict::Disagree,
            why: said(
                format!("净盈亏 {net} ≠ 已实现 {realized} − 手续费 {fees} + 资金费 {funding}"),
                format!("net P&L {net} ≠ realized {realized} − fees {fees} + funding {funding}"),
            ),
        }
    }
}

/// All the checks taken together: any disagreement disagrees; otherwise
/// any check that could not tell leaves the whole unable to tell.
#[must_use]
pub fn overall(findings: &[Finding]) -> Verdict {
    if findings.iter().any(|f| f.verdict == Verdict::Disagree) {
        Verdict::Disagree
    } else if findings.is_empty() || findings.iter().any(|f| f.verdict == Verdict::CannotTell) {
        Verdict::CannotTell
    } else {
        Verdict::Agree
    }
}

/// The findings as the black box records them, inside the trader sample.
#[must_use]
pub fn to_json(findings: &[Finding]) -> Value {
    json!({
        "verdict": overall(findings).as_str(),
        "checks": findings.iter().map(|f| json!({
            "name": f.name,
            "verdict": f.verdict.as_str(),
            "why": f.why.as_ref().map(|s| s.zh.clone()),
            "why_en": f.why.as_ref().map(|s| s.en.clone()),
        })).collect::<Vec<_>>(),
    })
}

/// The disagreements in one sentence, for an alert.
#[must_use]
pub fn disagreement(findings: &[Finding]) -> Option<Said> {
    let bad: Vec<&Said> = findings
        .iter()
        .filter(|f| f.verdict == Verdict::Disagree)
        .filter_map(|f| f.why.as_ref())
        .collect();
    if bad.is_empty() {
        return None;
    }
    Some(Said {
        zh: format!(
            "交易进程的快照前后对不上：{}",
            bad.iter()
                .map(|s| s.zh.as_str())
                .collect::<Vec<_>>()
                .join("；")
        ),
        en: format!(
            "the trader's snapshots contradict each other: {}",
            bad.iter()
                .map(|s| s.en.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(pid: u64, fills: u64, pos: &str, realized: &str, net: Option<&str>) -> Value {
        json!({
            "pid": pid,
            "positions": if pos.is_empty() { json!([]) } else { json!([{"side": "LONG", "amount": pos}]) },
            "counters": {"fills": fills, "sent": 10 + fills, "disconnects": 0},
            "pnl": {"realized": realized, "fees": net.map(|_| "1.5"), "funding": "0.5", "net": net},
        })
    }

    fn verdict_of(findings: &[Finding], name: &str) -> Verdict {
        findings
            .iter()
            .find(|f| f.name == name)
            .expect(name)
            .verdict
    }

    #[test]
    fn a_fill_explains_a_moved_position_and_realized_pnl() {
        let f = check(
            &sample(7, 3, "0.010", "10", Some("9")),
            &sample(7, 4, "0.02", "12", Some("11")),
        );
        assert_eq!(overall(&f), Verdict::Agree, "{f:?}");
        assert_eq!(disagreement(&f), None);
    }

    #[test]
    fn a_position_that_moved_without_a_fill_disagrees() {
        let f = check(
            &sample(7, 3, "0.01", "10", Some("9")),
            &sample(7, 3, "0.02", "10", Some("9")),
        );
        assert_eq!(verdict_of(&f, "position_needs_fill"), Verdict::Disagree);
        assert_eq!(overall(&f), Verdict::Disagree);
        let msg = disagreement(&f).expect("worded");
        assert!(
            msg.en.contains("the position changed with no fill"),
            "{}",
            msg.en
        );
    }

    #[test]
    fn the_same_position_written_differently_is_the_same_position() {
        let f = check(
            &sample(7, 3, "0.010", "10", Some("9")),
            &sample(7, 3, "0.01", "10", Some("9")),
        );
        assert_eq!(verdict_of(&f, "position_needs_fill"), Verdict::Agree);
    }

    #[test]
    fn realized_pnl_that_moved_without_a_fill_disagrees() {
        let f = check(
            &sample(7, 3, "0.01", "10", Some("9")),
            &sample(7, 3, "0.01", "11", Some("10")),
        );
        assert_eq!(verdict_of(&f, "realized_needs_fill"), Verdict::Disagree);
    }

    #[test]
    fn a_counter_that_went_down_disagrees() {
        let mut now = sample(7, 2, "0.01", "10", Some("9"));
        now["counters"]["sent"] = json!(1);
        let f = check(&sample(7, 3, "0.01", "10", Some("9")), &now);
        assert_eq!(verdict_of(&f, "counters_monotonic"), Verdict::Disagree);
    }

    #[test]
    fn a_net_that_is_not_realized_less_fees_plus_funding_disagrees() {
        // 10 − 1.5 + 0.5 = 9, not 8.
        let f = check(
            &sample(7, 3, "0.01", "10", Some("9")),
            &sample(7, 3, "0.01", "10", Some("8")),
        );
        assert_eq!(verdict_of(&f, "net_identity"), Verdict::Disagree);
    }

    #[test]
    fn unknown_fees_cannot_tell_and_are_not_agreement() {
        let f = check(
            &sample(7, 3, "0.01", "10", None),
            &sample(7, 3, "0.01", "10", None),
        );
        assert_eq!(verdict_of(&f, "net_identity"), Verdict::CannotTell);
        assert_eq!(overall(&f), Verdict::CannotTell);
        assert_eq!(disagreement(&f), None);
    }

    #[test]
    fn a_restart_between_samples_cannot_tell() {
        let f = check(
            &sample(7, 30, "0.01", "10", Some("9")),
            &sample(8, 0, "", "0", Some("-1")),
        );
        assert_eq!(overall(&f), Verdict::CannotTell);
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn the_recorded_form_carries_every_check_in_both_languages() {
        let f = check(
            &sample(7, 3, "0.01", "10", Some("9")),
            &sample(7, 3, "0.02", "10", Some("9")),
        );
        let v = to_json(&f);
        assert_eq!(v["verdict"], "disagree");
        assert_eq!(v["checks"].as_array().map(Vec::len), Some(4));
        assert!(v["checks"][1]["why"].as_str().is_some());
        assert!(v["checks"][1]["why_en"].as_str().is_some());
    }
}
