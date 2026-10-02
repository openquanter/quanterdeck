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
    let caps = oq_deck_core::capabilities::detect(
        Some(&dir),
        None,
        None,
        None,
        false,
        6,
        oq_deck_core::capabilities::Capability::on(),
        oq_deck_core::lang::Lang::Zh,
    );
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("chmod back");
    std::fs::remove_dir_all(&dir).ok();
    if readable_anyway {
        return;
    }
    assert!(listed.is_err(), "unreadable is not empty");
    assert!(!caps.runs.available, "and not available either");
}

/// A link in the directory is not a run in it: joined onto the path, an
/// id naming a symlink was followed wherever it pointed.
#[cfg(unix)]
#[test]
fn a_symbolic_link_is_not_resolved() {
    let dir = std::env::temp_dir().join(format!("oq-deck-links-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let outside = dir.with_extension("outside");
    std::fs::write(&outside, "not a run").expect("write");
    let link = dir.join("escape.run");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&outside, &link).expect("symlink");
    std::fs::write(dir.join("plain.run"), "x").expect("write");

    assert!(runs::resolve(&dir, "escape").is_none());
    assert!(runs::resolve(&dir, "plain").is_some());

    // And the listing agrees with the resolver. It did not: `is_file`
    // follows a link, so `escape` was offered on the page and then
    // refused when it was opened — and the page is the half that was
    // reached by anyone.
    let listed = format!("{:?}", runs::list(&dir).expect("the directory reads"));
    assert!(listed.contains("plain"), "{listed}");
    assert!(!listed.contains("escape"), "{listed}");

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_file(&outside).ok();
}

/// Put a file's modification time in the past, out of the window in
/// which the cache will not yet trust it.
fn settle(path: &Path, secs_ago: u64) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open")
        .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(secs_ago))
        .expect("set mtime");
}

fn summary<'a>(entries: &'a [runs::Entry], id: &str) -> &'a runs::RunSummary {
    entries
        .iter()
        .find_map(|e| match e {
            runs::Entry::Read(s) if s.id == id => Some(s),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{id} reads: {entries:?}"))
}

/// A cached listing says what an uncached one says, after every change:
/// a rewrite (the same length, even), a file that stops reading, and one
/// that is removed.
#[test]
fn a_cached_listing_follows_the_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let a = dir.path().join("a.run");
    let b = dir.path().join("b.run");
    std::fs::copy(fixtures().join("baseline.run"), &a).expect("copy");
    std::fs::copy(fixtures().join("config-moved.run"), &b).expect("copy");
    settle(&a, 600);
    settle(&b, 600);

    let cache = runs::Cache::default();
    let first = runs::list_cached(dir.path(), &cache).expect("reads");
    assert_eq!(cache.len(), 2, "both settled files are kept");
    assert_eq!(summary(&first, "a").identity.code_commit, "a1b2c3d");
    assert_eq!(
        format!("{first:?}"),
        format!("{:?}", runs::list(dir.path()).expect("reads"))
    );

    // Another run of exactly the same length, with an old time stamped
    // on it: what tells the cache is the inode's change time.
    assert_eq!(
        std::fs::metadata(fixtures().join("baseline.run"))
            .unwrap()
            .len(),
        std::fs::metadata(fixtures().join("same-experiment.run"))
            .unwrap()
            .len(),
    );
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::copy(fixtures().join("same-experiment.run"), &a).expect("copy");
    settle(&a, 600);
    let rewritten = runs::list_cached(dir.path(), &cache).expect("reads");
    assert_eq!(summary(&rewritten, "a").identity.code_commit, "e4f5g6h");

    // A file that stops reading is listed with its reason, and the total
    // is withheld, exactly as without a cache.
    std::fs::copy(fixtures().join("truncated.run"), &b).expect("copy");
    settle(&b, 300);
    let broken = runs::list_cached(dir.path(), &cache).expect("reads");
    assert!(
        broken
            .iter()
            .any(|e| matches!(e, runs::Entry::Unreadable { id, .. } if id == "b")),
        "{broken:?}"
    );
    assert_eq!(runs::total_pnl(&broken), None);

    std::fs::remove_file(&b).expect("remove");
    let removed = runs::list_cached(dir.path(), &cache).expect("reads");
    assert_eq!(ids(&removed), vec!["a".to_owned()]);
    assert_eq!(cache.len(), 1, "a removed file is forgotten");

    std::fs::remove_file(&a).expect("remove");
    assert!(
        runs::list_cached(dir.path(), &cache)
            .expect("reads")
            .is_empty()
    );
    assert!(cache.is_empty());
}
