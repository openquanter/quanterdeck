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
    let entries = runs::list(&fixtures());
    let names = ids(&entries);
    assert!(names.contains(&"baseline".to_owned()));
    assert!(
        names.contains(&"truncated".to_owned()),
        "a file that will not parse must appear with its reason, not vanish"
    );
}

#[test]
fn the_broken_one_carries_the_reason() {
    let entries = runs::list(&fixtures());
    let broken = entries
        .iter()
        .find_map(|entry| match entry {
            runs::Entry::Unreadable { id, error, .. } if id == "truncated" => Some(error),
            _ => None,
        })
        .expect("the truncated fixture should not read");
    assert!(!broken.is_empty(), "a refusal must say why");
}

#[test]
fn totals_ignore_what_could_not_be_read() {
    let entries = runs::list(&fixtures());
    // Three readable fixtures: 123.456 + 123.456 + 481.5.
    let total = runs::total_pnl(&entries);
    assert!(
        (total - 728.412).abs() < 1e-9,
        "a file that did not read must not contribute a zero, got {total}"
    );
}

#[test]
fn a_missing_directory_is_an_empty_listing_not_a_failure() {
    assert!(runs::list(Path::new("/nonexistent/runs")).is_empty());
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
