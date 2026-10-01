//! Embed the framework commit this deck is built against.
//!
//! Read from the workspace's `Cargo.lock` rather than `Cargo.toml`: the
//! manifest pins a short rev, the lock records the full commit it
//! resolved to, and that is what the deck actually compiled.
//!
//! Rerun when the lock changes, so moving the pin cannot leave an old
//! revision baked into a new binary.

#[path = "src/lockfile.rs"]
mod lockfile;

fn main() {
    let lock = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock");
    println!("cargo::rerun-if-changed={}", lock.display());
    println!("cargo::rerun-if-changed=src/lockfile.rs");
    let found = std::fs::read_to_string(&lock)
        .map_err(|e| format!("{}: {e}", lock.display()))
        .and_then(|text| lockfile::framework_rev(&text));
    // Always set, empty when unknown: `env!` in the deck then compiles
    // either way, and an empty revision is reported as "cannot tell"
    // with the reason beside it.
    match found {
        Ok(rev) => {
            println!("cargo::rustc-env=OQ_DECK_FRAMEWORK_REV={rev}");
            println!("cargo::rustc-env=OQ_DECK_FRAMEWORK_REV_WHY=");
        }
        Err(why) => {
            println!("cargo::warning=no framework revision embedded: {why}");
            println!("cargo::rustc-env=OQ_DECK_FRAMEWORK_REV=");
            println!("cargo::rustc-env=OQ_DECK_FRAMEWORK_REV_WHY={why}");
        }
    }
}
