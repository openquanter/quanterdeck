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
