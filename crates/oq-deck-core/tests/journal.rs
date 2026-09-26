//! The deck reads journals as the framework it is pinned to writes them.
//!
//! The pin fell thirty-three commits behind the writer: a journal with a
//! cancellation (record kind 9) read here as undecodable frames, and the
//! cancelled orders as still resting — the reconciliation page reported
//! orders the venue no longer held. This writes a journal with the
//! framework's own writer, so a pin that cannot read what the framework
//! writes fails here rather than on the operator's screen.

use oq_journal::{SyncPolicy, Writer};
use oq_live::record::{OutcomeTag, Record};
use oq_types::{Nanos, PriceTicks, QtyLots, Side};

#[test]
fn a_cancelled_order_is_neither_resting_nor_undecodable() {
    let dir = std::env::temp_dir().join(format!("oq-deck-journal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("run.oqj");
    let _ = std::fs::remove_file(&path);
    {
        let mut w = Writer::open(&path, SyncPolicy::EveryRecord).expect("journal opens");
        for r in [
            Record::SessionStart {
                prefix: "oq".into(),
                symbol: "BTCUSDT".into(),
                price_scale: 2,
                qty_scale: 3,
            },
            Record::Submitted {
                at: Nanos(1),
                client_id: "oq-1".into(),
                side: Side::Buy,
                limit_price: PriceTicks(6_000_000),
                qty: QtyLots(1),
                reduce_only: false,
                leg: String::new(),
            },
            Record::Outcome {
                at: Nanos(2),
                client_id: "oq-1".into(),
                tag: OutcomeTag::Accepted,
                detail: String::new(),
            },
            Record::Cancelled {
                at: Nanos(3),
                client_id: "oq-1".into(),
            },
        ] {
            w.append(r.kind(), &r.encode()).expect("append");
        }
        w.sync().expect("sync");
    }
    let view = oq_deck_core::live::belief(&dir, "run").expect("readable");
    assert_eq!(
        view.undecodable, 0,
        "every record the framework writes is read"
    );
    assert!(
        view.resting.is_empty(),
        "a cancelled order is not resting: {:?}",
        view.resting
    );
    std::fs::remove_dir_all(&dir).ok();
}

fn journal(dir: &std::path::Path, records: &[Record]) {
    std::fs::create_dir_all(dir).expect("dir");
    let path = dir.join("run.oqj");
    let _ = std::fs::remove_file(&path);
    let mut w = Writer::open(&path, SyncPolicy::EveryRecord).expect("journal opens");
    for r in records {
        w.append(r.kind(), &r.encode()).expect("append");
    }
    w.sync().expect("sync");
}

fn start() -> Record {
    Record::SessionStart {
        prefix: "oq".into(),
        symbol: "BTCUSDT".into(),
        price_scale: 2,
        qty_scale: 3,
    }
}

const FLAT_VENUE: &str = "symbol BTCUSDT\nread_at_ms 1";

/// No difference is not agreement when the belief has a hole: a journal
/// that never recorded the position it took over reads as flat whether
/// or not it was.
#[test]
fn no_difference_over_a_belief_with_holes_is_cannot_tell() {
    let dir = std::env::temp_dir().join(format!("oq-deck-verdict-a-{}", std::process::id()));
    journal(&dir, &[start()]);
    let r = oq_deck_core::live::reconcile(&dir, "run", FLAT_VENUE).expect("reconciles");
    assert!(r.differences.is_empty());
    assert_eq!(r.verdict, oq_deck_core::live::Verdict::CannotTell);
    assert!(!r.agrees);
    std::fs::remove_dir_all(&dir).ok();
}

/// A reading taken before this run began is a reading of the run before
/// it, and the differences between two runs are not a disagreement.
///
/// This is what a deploy looks like on the console for the minute the
/// reader takes to catch up — the new run's ladder against the old run's,
/// every rung counted in both directions. On the testnet host that was
/// twenty-six red differences, seconds after a deploy that had worked,
/// on the page whose job is to say whether the books are right.
#[test]
fn a_reading_older_than_the_run_is_not_a_disagreement() {
    use oq_deck_core::live::{CannotTell, Verdict};
    const SEC: i64 = 1_000_000_000;
    let dir = std::env::temp_dir().join(format!("oq-deck-verdict-c-{}", std::process::id()));
    journal(
        &dir,
        &[
            start(),
            Record::Reconciled {
                at: Nanos(10 * SEC),
                legs: Vec::new(),
            },
            Record::Submitted {
                at: Nanos(11 * SEC),
                client_id: "oq-1".into(),
                side: Side::Buy,
                limit_price: PriceTicks(6_000_000),
                qty: QtyLots(1),
                reduce_only: false,
                leg: String::new(),
            },
            Record::Outcome {
                at: Nanos(12 * SEC),
                client_id: "oq-1".into(),
                tag: OutcomeTag::Accepted,
                detail: String::new(),
            },
        ],
    );

    // The venue still holds the run before this one's order, and the
    // reading was taken a second before this run adopted.
    let older = "symbol BTCUSDT\nread_at_ms 9000\norder oq-0";
    let r = oq_deck_core::live::reconcile(&dir, "run", older).expect("reconciles");
    assert!(!r.differences.is_empty(), "two runs do differ");
    assert_eq!(r.verdict, Verdict::CannotTell, "{:?}", r.differences);
    assert_eq!(r.cannot_tell, Some(CannotTell::ReadingPredatesTheRun));
    assert!(!r.agrees);

    // And a reading from after this run began compares as it always did.
    let current = "symbol BTCUSDT\nread_at_ms 12000\norder oq-1";
    let r = oq_deck_core::live::reconcile(&dir, "run", current).expect("reconciles");
    assert!(r.differences.is_empty(), "{:?}", r.differences);
    assert_eq!(r.verdict, Verdict::Agree);
    assert_eq!(r.cannot_tell, None);
    std::fs::remove_dir_all(&dir).ok();
}

/// And a whole belief that matches is agreement.
#[test]
fn no_difference_over_a_whole_belief_is_agreement() {
    let dir = std::env::temp_dir().join(format!("oq-deck-verdict-b-{}", std::process::id()));
    journal(
        &dir,
        &[
            start(),
            Record::Reconciled {
                at: Nanos(1),
                legs: Vec::new(),
            },
        ],
    );
    let r = oq_deck_core::live::reconcile(&dir, "run", FLAT_VENUE).expect("reconciles");
    assert_eq!(
        r.verdict,
        oq_deck_core::live::Verdict::Agree,
        "{:?}",
        r.differences
    );
    assert!(r.agrees);
    std::fs::remove_dir_all(&dir).ok();
}

/// A replay reads newest first, filters by kind, and pages by sequence.
#[test]
fn records_page_newest_first_by_kind() {
    let dir = tempfile::tempdir().expect("dir");
    let mut rs = vec![start()];
    for i in 0..5 {
        rs.push(Record::Tick {
            at: Nanos(100 + i),
            seen: Nanos(200 + i),
            last: PriceTicks(6_000_000 + i),
            bid: PriceTicks(5_999_999),
            ask: PriceTicks(6_000_001),
            volume: QtyLots(i),
        });
    }
    rs.push(Record::Operator {
        at: Nanos(300),
        command: "halt".into(),
        reason: "looking".into(),
        origin: "uid 1 deck".into(),
        outcome: "halted".into(),
    });
    journal(dir.path(), &rs);

    let all = oq_deck_core::live::records(dir.path(), "run", &[], 100, None).expect("reads");
    assert_eq!(all.total, 7);
    assert_eq!(all.records[0].kind, "operator", "newest first");
    assert_eq!((all.price_scale, all.qty_scale), (2, 3));

    let ticks =
        oq_deck_core::live::records(dir.path(), "run", &["tick".into()], 2, None).expect("reads");
    assert_eq!(ticks.total, 5);
    assert_eq!(ticks.records.len(), 2);
    assert_eq!(ticks.records[0].fields["last"], 6_000_004);
    let older =
        oq_deck_core::live::records(dir.path(), "run", &["tick".into()], 2, ticks.next_before)
            .expect("reads");
    assert_eq!(older.records[0].fields["last"], 6_000_002);
    assert!(oq_deck_core::live::records(dir.path(), "../x", &[], 1, None).is_err());
}
