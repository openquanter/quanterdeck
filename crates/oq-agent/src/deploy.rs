//! Releases: verified, installed, switched to, and switched back from.
//!
//! A release is staged in the incoming directory as `<id>/` holding the
//! binaries, a `manifest.json` naming each with its SHA-256, and
//! `manifest.json.sig`, an `ssh-keygen -Y sign` signature made on the
//! operator's machine with a key that exists nowhere else. The agent
//! trusts the signers file it is given and nothing the deck says: a
//! checksum proves a file arrived intact, a signature proves who built it,
//! and only the second stops a compromised deck from shipping its own
//! binary.
//!
//! Installed releases live under the releases directory, owned by this
//! agent; `current` and `previous` are links into it, swapped by rename.
//! The trader's launcher runs `current/<name>`.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::config::Config;
use crate::notify::{BLUE, GREEN, Message, RED};
use crate::system;

/// The binaries a release may carry.
pub const ALLOWED: &[&str] = &["oqp-live", "oq-recon"];

/// The namespace signatures are made in, so a signature for anything else
/// made with the same key does not pass here.
pub const NAMESPACE: &str = "oq-release";

/// A deployment in progress or the last one, for the deck to show.
#[derive(Debug, Default, Clone)]
pub struct Progress {
    pub running: bool,
    pub id: String,
    pub steps: Vec<(i64, String)>,
    pub outcome: Option<String>,
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0_u8; 1 << 16];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !id.starts_with('.')
}

/// Check a staged release: signature, then every file against the
/// manifest. Returns the manifest.
///
/// # Errors
/// Anything that does not match, named.
pub fn verify(cfg: &Config, id: &str) -> Result<Value, String> {
    if !valid_id(id) {
        return Err(format!("{id:?} is not a release id"));
    }
    let dir = cfg.incoming.join(id);
    let manifest_path = dir.join("manifest.json");
    let manifest_bytes = std::fs::read(&manifest_path).map_err(|e| format!("manifest: {e}"))?;
    let mut child = Command::new("ssh-keygen")
        .args(["-Y", "verify", "-f"])
        .arg(&cfg.signers)
        .args(["-I", NAMESPACE, "-n", NAMESPACE, "-s"])
        .arg(dir.join("manifest.json.sig"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ssh-keygen: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin
            .write_all(&manifest_bytes)
            .map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "the signature does not verify: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let manifest: Value =
        serde_json::from_slice(&manifest_bytes).map_err(|e| format!("manifest: {e}"))?;
    if manifest["id"].as_str() != Some(id) {
        return Err("the manifest names a different release".into());
    }
    let files = manifest["files"]
        .as_object()
        .ok_or("the manifest lists no files")?;
    if files.is_empty() {
        return Err("the manifest lists no files".into());
    }
    for (name, want) in files {
        if !ALLOWED.contains(&name.as_str()) {
            return Err(format!("{name} is not a binary a release may carry"));
        }
        let path = dir.join(name);
        let meta = std::fs::symlink_metadata(&path).map_err(|e| format!("{name}: {e}"))?;
        if !meta.file_type().is_file() {
            return Err(format!("{name} is not a regular file"));
        }
        let got = sha256_file(&path)?;
        if Some(got.as_str()) != want.as_str() {
            return Err(format!("{name} does not match the manifest"));
        }
    }
    Ok(manifest)
}

fn link_target(link: &Path) -> Option<String> {
    std::fs::read_link(link)
        .ok()
        .and_then(|t| t.file_name().map(|n| n.to_string_lossy().to_string()))
}

/// Staged and installed releases, and which run now.
#[must_use]
pub fn list(cfg: &Config, progress: &Progress) -> Value {
    let names = |dir: &Path| -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .filter(|n| valid_id(n))
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v.reverse();
        v
    };
    let staged: Vec<Value> = names(&cfg.incoming)
        .into_iter()
        .map(|id| match verify(cfg, &id) {
            Ok(m) => json!({"id": id, "verified": true, "manifest": m}),
            Err(e) => json!({"id": id, "verified": false, "problem": e}),
        })
        .collect();
    json!({
        "staged": staged,
        "installed": names(&cfg.releases),
        "current": link_target(&cfg.releases.join("current")),
        "previous": link_target(&cfg.releases.join("previous")),
        "progress": {
            "running": progress.running, "id": progress.id, "outcome": progress.outcome,
            "steps": progress.steps.iter().map(|(t, s)| json!({"at_ms": t, "step": s})).collect::<Vec<_>>(),
        },
    })
}

/// Point `link` at `target` by creating a new link and renaming it over.
fn relink(dir: &Path, link: &str, target: &str) -> Result<(), String> {
    let tmp = dir.join(format!(".{link}.new"));
    let _ = std::fs::remove_file(&tmp);
    std::os::unix::fs::symlink(target, &tmp).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join(link)).map_err(|e| e.to_string())
}

/// Copy a verified release into place under its id.
fn install(cfg: &Config, id: &str, manifest: &Value) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    let dest = cfg.releases.join(id);
    if dest.exists() {
        return Ok(dest);
    }
    let tmp = cfg.releases.join(format!(".{id}.tmp"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    for name in manifest["files"]
        .as_object()
        .map(|o| o.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default()
    {
        let to = tmp.join(&name);
        std::fs::copy(cfg.incoming.join(id).join(&name), &to)
            .map_err(|e| format!("{name}: {e}"))?;
        std::fs::set_permissions(&to, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        // Checked again where it will run from: the copy is what executes.
        if Some(sha256_file(&to)?.as_str()) != manifest["files"][&name].as_str() {
            return Err(format!("{name} changed while being installed"));
        }
    }
    // A release carrying only some binaries keeps the others from the one
    // it replaces.
    if let Some(cur) = link_target(&cfg.releases.join("current")) {
        for name in ALLOWED {
            let to = tmp.join(name);
            let from = cfg.releases.join(&cur).join(name);
            if !to.exists() && from.exists() {
                std::fs::copy(&from, &to).map_err(|e| format!("{name}: {e}"))?;
                std::fs::set_permissions(&to, std::fs::Permissions::from_mode(0o755))
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

/// Positions from a status answer, as sorted "side amount" strings.
fn positions(status: &Value) -> Vec<String> {
    let mut v: Vec<String> = status["positions"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|p| {
                    format!(
                        "{} {}",
                        p["side"].as_str().unwrap_or(""),
                        p["amount"].as_str().unwrap_or("")
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// How long a new release has to show it is healthy.
pub const HEALTH_WINDOW: Duration = Duration::from_secs(300);

/// Wait until the trader is up, answering, trading data, not halted and
/// holding what it held before; or say what never came right.
fn healthy(cfg: &Config, before: &[String]) -> Result<(), String> {
    let deadline = std::time::Instant::now() + HEALTH_WINDOW;
    let mut last = String::from("never answered");
    while std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(10));
        if !system::is_active(&cfg.trader_unit) {
            last = format!("{} is not running", cfg.trader_unit);
            continue;
        }
        match crate::control::ask(&cfg.control_dir, "status", "oq-agent deploy", "") {
            Err(e) => last = format!("status: {e}"),
            Ok(s) if s["halted"] == true => {
                return Err(format!(
                    "halted: {}",
                    s["halt_reason"].as_str().unwrap_or("")
                ));
            }
            Ok(s) if s["feed"]["unreadable"].as_u64().unwrap_or(0) > 0 => {
                return Err("market data it cannot read".into());
            }
            Ok(s) if s["ticks"].as_u64().unwrap_or(0) == 0 => last = "no market data yet".into(),
            Ok(s) if positions(&s) != before => {
                last = format!("positions {:?}, before {:?}", positions(&s), before);
            }
            Ok(_) => return Ok(()),
        }
    }
    Err(last)
}

/// Stop the trader and require that it stopped cleanly: its own shutdown
/// withdrew every order and verified it, which is what exit 0 means.
fn stop_trader(cfg: &Config) -> Result<(), String> {
    system::unit_action(&cfg.trader_unit, "stop")?;
    let state = system::unit_state(&cfg.trader_unit);
    let status = state["ExecMainStatus"].as_str().unwrap_or("?").to_string();
    if !matches!(status.as_str(), "0" | "98") {
        return Err(format!(
            "the trader exited {status} while stopping: orders may still rest at the venue"
        ));
    }
    Ok(())
}

/// Switch to `target` (an installed id), with a health check, rolling
/// back to what ran before if it fails. Runs on its own thread.
pub fn switch(
    cfg: Config,
    target: String,
    what: String,
    progress: Arc<Mutex<Progress>>,
    notify: std::sync::mpsc::Sender<Message>,
    raised: Arc<Mutex<crate::alerts::Raised>>,
) {
    let step = |s: String| {
        eprintln!("deploy: {s}");
        if let Ok(mut p) = progress.lock() {
            p.steps.push((system::now_ms(), s));
        }
    };
    let finish = |outcome: String, ok: bool| {
        if let Ok(mut p) = progress.lock() {
            p.running = false;
            p.outcome = Some(outcome.clone());
        }
        let _ = notify.send(Message {
            title: if ok {
                format!("{what} 完成")
            } else {
                format!("{what} 失败")
            },
            body: outcome,
            color: if ok { GREEN } else { RED },
        });
    };
    let _ = notify.send(Message {
        title: format!("{what} 开始"),
        body: format!("切换到 {target}"),
        color: BLUE,
    });

    let before = crate::control::ask(&cfg.control_dir, "status", "oq-agent deploy", "")
        .map(|s| positions(&s))
        .unwrap_or_default();
    step(format!("positions before: {before:?}"));
    let old = link_target(&cfg.releases.join("current"));
    if let Ok(mut r) = raised.lock() {
        r.trader_stopped_on_purpose = true;
    }
    step("stopping the trader (it withdraws its own orders)".into());
    if let Err(e) = stop_trader(&cfg) {
        finish(
            format!(
                "stopped: {e}. Nothing was switched; the trader is left stopped for a person to look at."
            ),
            false,
        );
        return;
    }
    if let Some(o) = &old
        && let Err(e) = relink(&cfg.releases, "previous", o)
    {
        finish(format!("could not record the previous release: {e}"), false);
        return;
    }
    if let Err(e) = relink(&cfg.releases, "current", &target) {
        finish(format!("could not switch: {e}"), false);
        return;
    }
    step(format!("current -> {target}; starting"));
    let started = system::unit_action(&cfg.trader_unit, "start");
    if let Ok(mut r) = raised.lock() {
        r.trader_stopped_on_purpose = false;
    }
    let verdict = started.and_then(|()| {
        step("checking health".into());
        healthy(&cfg, &before)
    });
    match verdict {
        Ok(()) => finish(format!("{target} is running and healthy"), true),
        Err(why) => {
            step(format!("unhealthy: {why}; rolling back"));
            let Some(o) = old else {
                finish(
                    format!("{target} is unhealthy ({why}) and there is nothing to roll back to"),
                    false,
                );
                return;
            };
            let back = stop_trader(&cfg)
                .and_then(|()| relink(&cfg.releases, "current", &o))
                .and_then(|()| system::unit_action(&cfg.trader_unit, "start"));
            match back {
                Ok(()) => finish(
                    format!("{target} was unhealthy ({why}); rolled back to {o}"),
                    false,
                ),
                Err(e) => finish(
                    format!("{target} was unhealthy ({why}) and the rollback failed: {e}"),
                    false,
                ),
            }
        }
    }
}

/// Start a deployment of a staged release.
///
/// # Errors
/// One is already running, or the release does not verify.
pub fn start_deploy(
    cfg: &Config,
    id: &str,
    progress: &Arc<Mutex<Progress>>,
    notify: &std::sync::mpsc::Sender<Message>,
    raised: &Arc<Mutex<crate::alerts::Raised>>,
) -> Result<(), String> {
    claim(progress, id)?;
    let installed = verify(cfg, id).and_then(|m| install(cfg, id, &m));
    if let Err(e) = installed {
        if let Ok(mut p) = progress.lock() {
            p.running = false;
            p.outcome = Some(e.clone());
        }
        return Err(e);
    }
    let (cfg, id, progress, notify, raised) = (
        cfg.clone(),
        id.to_string(),
        Arc::clone(progress),
        notify.clone(),
        Arc::clone(raised),
    );
    std::thread::spawn(move || {
        switch(
            cfg,
            id.clone(),
            format!("部署 {id}"),
            progress,
            notify,
            raised,
        )
    });
    Ok(())
}

/// Start a rollback to the previous release.
///
/// # Errors
/// One is already running, or there is no previous release.
pub fn start_rollback(
    cfg: &Config,
    progress: &Arc<Mutex<Progress>>,
    notify: &std::sync::mpsc::Sender<Message>,
    raised: &Arc<Mutex<crate::alerts::Raised>>,
) -> Result<(), String> {
    let prev = link_target(&cfg.releases.join("previous")).ok_or("there is no previous release")?;
    claim(progress, &prev)?;
    let (cfg, progress, notify, raised) = (
        cfg.clone(),
        Arc::clone(progress),
        notify.clone(),
        Arc::clone(raised),
    );
    std::thread::spawn(move || {
        switch(
            cfg,
            prev.clone(),
            format!("回滚到 {prev}"),
            progress,
            notify,
            raised,
        )
    });
    Ok(())
}

fn claim(progress: &Arc<Mutex<Progress>>, id: &str) -> Result<(), String> {
    let mut p = progress.lock().map_err(|_| "deploy state poisoned")?;
    if p.running {
        return Err(format!("a deployment of {} is already running", p.id));
    }
    *p = Progress {
        running: true,
        id: id.to_string(),
        steps: Vec::new(),
        outcome: None,
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(root: &Path) -> Config {
        Config {
            socket: root.join("s"),
            peers: vec![],
            units: vec![],
            manageable: vec![],
            trader_unit: "t".into(),
            control_dir: root.join("run"),
            log_dir: root.join("log"),
            state_dir: root.join("state"),
            releases: root.join("releases"),
            incoming: root.join("incoming"),
            signers: root.join("allowed_signers"),
            credentials: None,
            host: "test".into(),
            discord_guild: None,
            discord_channel: "c".into(),
            proxy: None,
        }
    }

    /// Stage a release signed with a fresh key, trusted or not.
    fn stage(root: &Path, id: &str, trusted: bool, tamper: bool) {
        let key = root.join("key");
        if !key.exists() {
            let ok = Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(&key)
                .status()
                .expect("ssh-keygen")
                .success();
            assert!(ok);
            let pubkey = std::fs::read_to_string(root.join("key.pub")).expect("pub");
            let line = if trusted {
                format!("{NAMESPACE} namespaces=\"{NAMESPACE}\" {pubkey}")
            } else {
                String::new()
            };
            std::fs::write(root.join("allowed_signers"), line).expect("signers");
        }
        let dir = root.join("incoming").join(id);
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("oqp-live"), b"#!/bin/sh\necho new\n").expect("bin");
        let sum = sha256_file(&dir.join("oqp-live")).expect("sum");
        let manifest = json!({"id": id, "files": {"oqp-live": sum}});
        std::fs::write(dir.join("manifest.json"), manifest.to_string()).expect("manifest");
        let ok = Command::new("ssh-keygen")
            .args(["-Y", "sign", "-q", "-n", NAMESPACE, "-f"])
            .arg(&key)
            .arg(dir.join("manifest.json"))
            .status()
            .expect("sign")
            .success();
        assert!(ok);
        if tamper {
            std::fs::write(dir.join("oqp-live"), b"#!/bin/sh\necho evil\n").expect("tamper");
        }
    }

    #[test]
    fn a_signed_release_verifies_and_installs() {
        let root = tempfile::tempdir().expect("root");
        let c = cfg(root.path());
        std::fs::create_dir_all(&c.releases).expect("releases");
        stage(root.path(), "r1", true, false);
        let m = verify(&c, "r1").expect("verifies");
        let dest = install(&c, "r1", &m).expect("installs");
        assert!(dest.join("oqp-live").exists());
        let listed = list(&c, &Progress::default());
        assert_eq!(listed["staged"][0]["verified"], true, "{listed}");
        assert_eq!(listed["installed"], json!(["r1"]));
    }

    #[test]
    fn a_changed_binary_or_an_unknown_signer_is_refused() {
        let root = tempfile::tempdir().expect("root");
        let c = cfg(root.path());
        stage(root.path(), "r2", true, true);
        assert!(verify(&c, "r2").unwrap_err().contains("does not match"));

        let other = tempfile::tempdir().expect("other");
        let c = cfg(other.path());
        stage(other.path(), "r3", false, false);
        assert!(verify(&c, "r3").unwrap_err().contains("signature"));
    }

    #[test]
    fn ids_cannot_name_anything_outside() {
        let root = tempfile::tempdir().expect("root");
        let c = cfg(root.path());
        for id in ["..", "../x", "a/b", "", ".hidden"] {
            assert!(verify(&c, id).is_err(), "{id}");
        }
    }
}
