//! The promotion gate: what it refuses, and what it says when it does.

use oq_deck_core::gate::{DEFAULT_OBSERVATION_HOURS, Evidence, Stage, advance, voided_by};
use oq_parity::manifest::RunManifest;

fn manifest(code: &str, data: &str, config: &str) -> RunManifest {
    RunManifest {
        code_commit: code.to_owned(),
        data_hash: data.to_owned(),
        config_hash: config.to_owned(),
        label: "L0".to_owned(),
    }
}

fn passed() -> Evidence {
    Evidence {
        backtest_run: Some("baseline".to_owned()),
        backtest_passed: true,
        ..Evidence::default()
    }
}

#[test]
fn a_draft_without_a_backtest_goes_nowhere() {
    let decision = advance(
        Stage::Draft,
        &Evidence::default(),
        DEFAULT_OBSERVATION_HOURS,
        1,
    );
    assert!(!decision.allowed);
    assert!(decision.reason.en.contains("no backtest"));
    assert!(decision.reason.zh.contains("回测"));
}

#[test]
fn a_failed_backtest_does_not_count() {
    let evidence = Evidence {
        backtest_passed: false,
        ..passed()
    };
    let decision = advance(Stage::Draft, &evidence, DEFAULT_OBSERVATION_HOURS, 1);
    assert!(!decision.allowed);
    assert!(decision.reason.en.contains("did not pass"));
}

#[test]
fn the_observation_window_must_actually_elapse() {
    let evidence = Evidence {
        observation_hours: 24,
        observation_fills: 10,
        ..passed()
    };
    let decision = advance(Stage::Observing, &evidence, DEFAULT_OBSERVATION_HOURS, 1);
    assert!(!decision.allowed);
    assert!(
        decision.reason.en.contains("48h of the 72h"),
        "{:?}",
        decision.reason
    );
}

#[test]
fn an_observation_that_never_traded_proves_nothing() {
    let evidence = Evidence {
        observation_hours: 100,
        observation_fills: 0,
        ..passed()
    };
    let decision = advance(Stage::Observing, &evidence, DEFAULT_OBSERVATION_HOURS, 1);
    assert!(!decision.allowed);
    assert!(
        decision.reason.en.contains("0 fills"),
        "{:?}",
        decision.reason
    );
}

#[test]
fn going_live_needs_a_human() {
    let evidence = Evidence {
        observation_hours: 100,
        observation_fills: 9,
        ..passed()
    };
    assert!(!advance(Stage::Confirmed, &evidence, DEFAULT_OBSERVATION_HOURS, 1).allowed);

    let signed = Evidence {
        confirmed_by: Some("operator".to_owned()),
        ..evidence
    };
    assert!(advance(Stage::Confirmed, &signed, DEFAULT_OBSERVATION_HOURS, 1).allowed);
}

#[test]
fn live_is_the_end_of_the_road() {
    let decision = advance(Stage::Live, &passed(), DEFAULT_OBSERVATION_HOURS, 1);
    assert!(!decision.allowed);
    assert_eq!(decision.reason.en, "it is already live");
    assert_eq!(decision.reason.zh, "已经是实盘了");
}

#[test]
fn a_moved_configuration_voids_the_evidence() {
    let evidence = Evidence {
        backtest_manifest: Some(manifest("a", "data-aaa", "config-aaa")),
        ..passed()
    };
    let voided = voided_by(&evidence, &manifest("a", "data-aaa", "config-bbb"));
    assert!(
        voided.is_some(),
        "a configuration change must send it back to draft"
    );
    assert!(voided.unwrap()[0].contains("rebased"));
}

#[test]
fn a_deploy_does_not_send_every_instance_back_to_draft() {
    // Code moving is the case a parity run is for. Treating it as
    // invalidation would make every release reset the whole fleet.
    let evidence = Evidence {
        backtest_manifest: Some(manifest("old", "data-aaa", "config-aaa")),
        ..passed()
    };
    assert!(voided_by(&evidence, &manifest("new", "data-aaa", "config-aaa")).is_none());
}
