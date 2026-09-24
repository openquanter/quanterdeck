//! Reading a directory of runs.

use std::path::{Path, PathBuf};

use oq_deck_core::runs;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/fixtures/runs")
}

fn ids(entries: &[runs::Entry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| match entry {
            runs::Entry::Read(summary) => summary.id.clone(),
            runs::Entry::Unreadable { id, .. } => id.clone(),
        })
        .collect()
}

#[test]
fn a_listing_names_every_file_including_the_broken_one() {
    let entries = runs::list(&fixtures()).expect("the fixtures directory reads");
    let names = ids(&entries);
    assert!(names.contains(&"baseline".to_owned()));
    assert!(
        names.contains(&"truncated".to_owned()),
        "a file that will not parse must appear with its reason, not vanish"
    );
}

#[test]
fn the_broken_one_carries_the_reason() {
    let entries = runs::list(&fixtures()).expect("the fixtures directory reads");
    let broken = entries
        .iter()
        .find_map(|entry| match entry {
            runs::Entry::Unreadable { id, error, .. } if id == "truncated" => Some(error),
            _ => None,
        })
        .expect("the truncated fixture should not read");
    assert!(!broken.is_empty(), "a refusal must say why");
}

/// A total that quietly left a file out is a number nobody can check, so
/// with an unreadable file there is no total; over the readable ones,
/// nothing that failed contributes a zero.
#[test]
fn a_total_is_withheld_while_a_file_will_not_read() {
    let entries = runs::list(&fixtures()).expect("the fixtures directory reads");
    assert_eq!(runs::total_pnl(&entries), None);

    let readable: Vec<runs::Entry> = entries
        .into_iter()
        .filter(|e| matches!(e, runs::Entry::Read(_)))
        .collect();
    // Three readable fixtures: 123.456 + 123.456 + 481.5.
    let total = runs::total_pnl(&readable).expect("one kind of run, all read");
    assert!((total - 728.412).abs() < 1e-9, "got {total}");
}

/// A backtest's P&L plus a live run's is not the P&L of anything.
#[test]
fn runs_of_different_kinds_have_no_total() {
    let mut entries: Vec<runs::Entry> = runs::list(&fixtures())
        .expect("reads")
        .into_iter()
        .filter(|e| matches!(e, runs::Entry::Read(_)))
        .collect();
    if let Some(runs::Entry::Read(summary)) = entries.first_mut() {
        summary.identity.label = "live".to_owned();
    }
    assert_eq!(runs::total_pnl(&entries), None);
}

#[test]
fn a_missing_directory_is_an_empty_listing_not_a_failure() {
    assert!(
        runs::list(Path::new("/nonexistent/runs"))
            .expect("absent is empty, not an error")
            .is_empty()
    );
}

#[test]
fn an_id_cannot_escape_the_runs_directory() {
    for attempt in ["../../etc/passwd", "..", "a/b", "a\\b", ""] {
        assert!(
            runs::resolve(&fixtures(), attempt).is_none(),
            "{attempt} should not resolve"
        );
    }
}

#[test]
fn detail_carries_the_fills_and_keeps_an_absent_tag_absent() {
    let detail = runs::detail(&fixtures(), "baseline").expect("baseline reads");
    assert_eq!(detail.fills.len(), 2);
    assert_eq!(detail.fills[0].tag, None, "an untagged fill has no tag");
    assert_eq!(detail.fills[1].tag.as_deref(), Some("exit"));
    assert_eq!(detail.summary.identity.code_commit, "a1b2c3d");
}

#[test]
fn a_code_change_alone_is_still_comparable() {
    let comparison =
        runs::compare_ids(&fixtures(), "baseline", "same-experiment", 0.0).expect("both read");
    assert_eq!(comparison.verdict.status, "code_changed");
    assert!(comparison.verdict.conclusive);
    assert!(
        comparison.passes,
        "identical output under new code is a pass"
    );
}

#[test]
fn a_moved_configuration_is_not_a_regression() {
    let comparison =
        runs::compare_ids(&fixtures(), "baseline", "config-moved", 0.0).expect("both read");
    assert_eq!(comparison.verdict.status, "invalidated");
    assert!(!comparison.verdict.conclusive);
    assert!(
        !comparison.passes,
        "\"we cannot tell\" must never render as \"they agree\""
    );
    assert!(
        comparison.differences == 0,
        "no differences are reported from a stale baseline"
    );
    assert!(
        comparison
            .verdict
            .changed
            .iter()
            .any(|w| w.contains("rebased")),
        "the verdict must say what to do: {:?}",
        comparison.verdict.changed
    );
}

/// A directory the deck may not read is not an empty one. Listed as
/// empty, it said "no runs" when the deck could not look.
#[cfg(unix)]
#[test]
fn an_unreadable_directory_is_reported_not_listed_as_empty() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("oq-deck-sealed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    // Root reads through permissions; there is nothing to test there.
    let readable_anyway = std::fs::read_dir(&dir).is_ok();
    let listed = runs::list(&dir);
    let caps = oq_deck_core::capabilities::detect(Some(&dir), None, false);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("chmod back");
    std::fs::remove_dir_all(&dir).ok();
    if readable_anyway {
        return;
    }
    assert!(listed.is_err(), "unreadable is not empty");
    assert!(!caps.runs.available, "and not available either");
}
