//! Write the example run files, using the framework's own writer.
//!
//! Hand-writing these would mean hand-computing `body-sha256`, and a
//! fixture whose hash is wrong tests the error path while claiming to
//! test the happy one. `cargo run -p oq-deck-core --example make_fixtures`

use std::fs;
use std::path::PathBuf;

use oq_parity::manifest::RunManifest;
use oq_parity::record::{Fill, RunOutput};
use oq_parity::wire::Run;
use oq_types::Side;

fn manifest(code: &str, data: &str, config: &str, label: &str) -> RunManifest {
    RunManifest {
        code_commit: code.to_owned(),
        data_hash: data.to_owned(),
        config_hash: config.to_owned(),
        label: label.to_owned(),
    }
}

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/fixtures/runs");
    fs::create_dir_all(&dir).expect("fixture directory");

    let fills = vec![
        Fill::new(
            1_700_000_000_000_000_000,
            "BTCUSDT",
            Side::Buy,
            6_000_000,
            5,
        ),
        Fill::new(
            1_700_000_060_000_000_000,
            "BTCUSDT",
            Side::Sell,
            6_001_000,
            5,
        )
        .with_tag("exit"),
    ];

    // The baseline, and a rerun of the same experiment under new code:
    // together they are the case a parity run exists for.
    let baseline = Run::new(
        manifest("a1b2c3d", "data-aaa", "config-aaa", "L0"),
        RunOutput::new(fills.clone(), 123.456),
    );
    let same_experiment = Run::new(
        manifest("e4f5g6h", "data-aaa", "config-aaa", "L0"),
        RunOutput::new(fills.clone(), 123.456),
    );
    // A third whose configuration moved. Nothing about the engine can be
    // concluded from comparing it, and the console has to say so rather
    // than draw a red line.
    let rebased = Run::new(
        manifest("e4f5g6h", "data-aaa", "config-bbb", "L0"),
        RunOutput::new(fills, 481.5),
    );

    for (name, run) in [
        ("baseline", &baseline),
        ("same-experiment", &same_experiment),
        ("config-moved", &rebased),
    ] {
        let path = dir.join(format!("{name}.run"));
        fs::write(&path, run.render()).expect("write fixture");
        println!("wrote {}", path.display());
    }

    // A file that will not parse. The listing must show it with its
    // reason rather than drop it, so there is a fixture for that too.
    let broken = dir.join("truncated.run");
    let text = baseline.render();
    let cut = text.lines().take(3).collect::<Vec<_>>().join("\n");
    fs::write(&broken, cut).expect("write fixture");
    println!("wrote {}", broken.display());
}
