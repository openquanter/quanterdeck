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

use oq_deck_core::lang::Said;

use crate::config::Config;
use crate::notify::{BLUE, GREEN, Message, RED};
use crate::system;

/// The binaries a release may carry.
pub const ALLOWED: &[&str] = &["oqp-live", "oq-recon"];

/// The namespace signatures are made in, so a signature for anything else
/// made with the same key does not pass here.
pub const NAMESPACE: &str = "oq-release";

/// A deployment in progress or the last one, for the deck to show.
///
/// Steps and the outcome are held in both languages: a deployment is
/// read while it runs and again weeks later, and a reader should not
/// need the language of whoever started it.
#[derive(Debug, Default, Clone)]
pub struct Progress {
    pub running: bool,
    pub id: String,
    pub steps: Vec<(i64, Said)>,
    pub outcome: Option<Said>,
}

impl Progress {
    fn file(state: &Path) -> PathBuf {
        state.join("last-deploy.json")
    }

    /// Kept on disk as it changes, so the last deployment's steps survive
    /// the agent restarting — which a deployment of the agent itself does.
    fn save(&self, state: &Path) {
        let v = json!({
            "running": self.running, "id": self.id,
            "outcome": self.outcome.as_ref().map(|o| o.zh.clone()),
            "outcome_en": self.outcome.as_ref().map(|o| o.en.clone()),
            "steps": self.steps.iter().map(|(t, s)| json!([t, s.zh, s.en])).collect::<Vec<_>>(),
        });
        let tmp = Self::file(state).with_extension("tmp");
        if std::fs::write(&tmp, v.to_string())
            .and_then(|()| std::fs::rename(&tmp, Self::file(state)))
            .is_err()
        {
            eprintln!("oq-agent: last deployment not saved");
        }
    }

    /// The last deployment, as saved. One still marked running was cut
    /// short by this agent stopping, and says so rather than claiming to
    /// be in progress.
    #[must_use]
    pub fn load(state: &Path) -> Self {
        let Some(v) = std::fs::read_to_string(Self::file(state))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        else {
            return Self::default();
        };
        let mut p = Self {
            running: false,
            id: v["id"].as_str().unwrap_or_default().to_string(),
            steps: v["steps"]
                .as_array()
                .map(|a| a.iter().filter_map(step_from).collect())
                .unwrap_or_default(),
            outcome: v["outcome"].as_str().map(|zh| {
                // A step written before outcomes carried two languages has
                // one, and is read as the same sentence in both rather
                // than being given a translation nobody made.
                Said::new(zh, v["outcome_en"].as_str().unwrap_or(zh))
            }),
        };
        if v["running"] == true {
            p.outcome = Some(Said::new(
                "中断：代理在这次部署期间停止了。",
                "interrupted: the agent stopped during this deployment",
            ));
        }
        p
    }
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
pub fn verify(cfg: &Config, id: &str) -> Result<Value, Said> {
    if !valid_id(id) {
        return Err(Said::new(
            format!("{id:?} 不是一个发布编号"),
            format!("{id:?} is not a release id"),
        ));
    }
    let dir = cfg.incoming.join(id);
    let manifest_path = dir.join("manifest.json");
    let manifest_bytes =
        std::fs::read(&manifest_path).map_err(|e| Said::same(format!("manifest: {e}")))?;
    let mut child = Command::new("ssh-keygen")
        .args(["-Y", "verify", "-f"])
        .arg(&cfg.signers)
        .args(["-I", NAMESPACE, "-n", NAMESPACE, "-s"])
        .arg(dir.join("manifest.json.sig"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Said::same(format!("ssh-keygen: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin
            .write_all(&manifest_bytes)
            .map_err(|e| Said::same(e.to_string()))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|e| Said::same(e.to_string()))?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(Said::new(
            format!("签名验证不通过：{why}"),
            format!("the signature does not verify: {why}"),
        ));
    }
    let manifest: Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| Said::same(format!("manifest: {e}")))?;
    if manifest["id"].as_str() != Some(id) {
        return Err(Said::new(
            "manifest 指向的是另一个发布",
            "the manifest names a different release",
        ));
    }
    let missing = Said::new("manifest 没有列出任何文件", "the manifest lists no files");
    let files = manifest["files"].as_object().ok_or(missing.clone())?;
    if files.is_empty() {
        return Err(missing);
    }
    for (name, want) in files {
        if !ALLOWED.contains(&name.as_str()) {
            return Err(Said::new(
                format!("{name} 不是发布可以携带的程序"),
                format!("{name} is not a binary a release may carry"),
            ));
        }
        let path = dir.join(name);
        let meta =
            std::fs::symlink_metadata(&path).map_err(|e| Said::same(format!("{name}: {e}")))?;
        if !meta.file_type().is_file() {
            return Err(Said::new(
                format!("{name} 不是一个普通文件"),
                format!("{name} is not a regular file"),
            ));
        }
        let got = sha256_file(&path).map_err(Said::same)?;
        if Some(got.as_str()) != want.as_str() {
            return Err(Said::new(
                format!("{name} 与 manifest 对不上"),
                format!("{name} does not match the manifest"),
            ));
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
            Err(e) => json!({"id": id, "verified": false, "problem": e.zh, "problem_en": e.en}),
        })
        .collect();
    json!({
        "staged": staged,
        "installed": names(&cfg.releases),
        "current": link_target(&cfg.releases.join("current")),
        "previous": link_target(&cfg.releases.join("previous")),
        "progress": {
            "running": progress.running, "id": progress.id,
            "outcome": progress.outcome.as_ref().map(|o| o.zh.clone()),
            "outcome_en": progress.outcome.as_ref().map(|o| o.en.clone()),
            "steps": progress.steps.iter().map(|(t, s)| json!({
                "at_ms": t, "step": s.zh, "step_en": s.en,
            })).collect::<Vec<_>>(),
        },
    })
}

/// Point `link` at `target` by creating a new link and renaming it over.
fn relink(dir: &Path, link: &str, target: &str) -> Result<(), Said> {
    let tmp = dir.join(format!(".{link}.new"));
    let _ = std::fs::remove_file(&tmp);
    std::os::unix::fs::symlink(target, &tmp).map_err(|e| Said::same(e.to_string()))?;
    std::fs::rename(&tmp, dir.join(link)).map_err(|e| Said::same(e.to_string()))
}

/// Copy a verified release into place under its id.
fn install(cfg: &Config, id: &str, manifest: &Value) -> Result<PathBuf, Said> {
    use std::os::unix::fs::PermissionsExt;
    let dest = cfg.releases.join(id);
    if dest.exists() {
        return Ok(dest);
    }
    let tmp = cfg.releases.join(format!(".{id}.tmp"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| Said::same(e.to_string()))?;
    for name in manifest["files"]
        .as_object()
        .map(|o| o.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default()
    {
        let to = tmp.join(&name);
        std::fs::copy(cfg.incoming.join(id).join(&name), &to)
            .map_err(|e| Said::same(format!("{name}: {e}")))?;
        std::fs::set_permissions(&to, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| Said::same(e.to_string()))?;
        // Checked again where it will run from: the copy is what executes.
        if Some(sha256_file(&to)?.as_str()) != manifest["files"][&name].as_str() {
            return Err(Said::new(
                format!("{name} 在安装过程中被改动了"),
                format!("{name} changed while being installed"),
            ));
        }
    }
    // A release carrying only some binaries keeps the others from the one
    // it replaces.
    if let Some(cur) = link_target(&cfg.releases.join("current")) {
        for name in ALLOWED {
            let to = tmp.join(name);
            let from = cfg.releases.join(&cur).join(name);
            if !to.exists() && from.exists() {
                std::fs::copy(&from, &to).map_err(|e| Said::same(format!("{name}: {e}")))?;
                std::fs::set_permissions(&to, std::fs::Permissions::from_mode(0o755))
                    .map_err(|e| Said::same(e.to_string()))?;
            }
        }
    }
    std::fs::rename(&tmp, &dest).map_err(|e| Said::same(e.to_string()))?;
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

/// A step as stored: `[at_ms, zh]`, or `[at_ms, zh, en]` for one written
/// since steps carried both. The older shape has one sentence and is read
/// as the same in both, rather than being given a translation that was
/// never made.
fn step_from(x: &Value) -> Option<(i64, Said)> {
    let a = x.as_array()?;
    let at = a.first()?.as_i64()?;
    let zh = a.get(1)?.as_str()?.to_string();
    let en = a.get(2).and_then(Value::as_str).unwrap_or(&zh).to_string();
    Some((at, Said::new(zh, en)))
}

/// How long a new release has to show it is healthy.
pub const HEALTH_WINDOW: Duration = Duration::from_secs(300);

/// Wait until the trader is up, answering, trading data, not halted and
/// holding what it held before; or say what never came right.
fn healthy(cfg: &Config, before: &[String]) -> Result<(), Said> {
    let deadline = std::time::Instant::now() + HEALTH_WINDOW;
    let mut last = Said::new("一直没有回应", "never answered");
    while std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(10));
        if !system::is_active(&cfg.trader_unit) {
            last = Said::new(
                format!("{} 没有在运行", cfg.trader_unit),
                format!("{} is not running", cfg.trader_unit),
            );
            continue;
        }
        match crate::control::ask(&cfg.control_dir, "status", "oq-agent deploy", "") {
            Err(e) => {
                last = Said::new(format!("状态查询：{}", e.zh), format!("status: {}", e.en));
            }
            Ok(s) if s["halted"] == true => {
                let why = s["halt_reason"].as_str().unwrap_or("");
                return Err(Said::new(
                    format!("已停机：{why}"),
                    format!("halted: {why}"),
                ));
            }
            Ok(s) if s["feed"]["unreadable"].as_u64().unwrap_or(0) > 0 => {
                return Err(Said::new("行情读不出来", "market data it cannot read"));
            }
            Ok(s) if s["ticks"].as_u64().unwrap_or(0) == 0 => {
                last = Said::new("还没有行情数据", "no market data yet");
            }
            Ok(s) if positions(&s) != before => {
                last = Said::new(
                    format!("持仓 {:?}，之前是 {:?}", positions(&s), before),
                    format!("positions {:?}, before {:?}", positions(&s), before),
                );
            }
            Ok(_) => return Ok(()),
        }
    }
    Err(last)
}

/// Stop the trader and require that it stopped cleanly: its own shutdown
/// withdrew every order and verified it, which is what exit 0 means.
fn stop_trader(cfg: &Config) -> Result<(), Said> {
    system::unit_action(&cfg.trader_unit, "stop")?;
    let state = system::unit_state(&cfg.trader_unit);
    let status = state["ExecMainStatus"].as_str().unwrap_or("?").to_string();
    if !matches!(status.as_str(), "0" | "98") {
        return Err(Said::new(
            format!("交易进程停止时退出码是 {status}：交易所那边可能还有挂单"),
            format!(
                "the trader exited {status} while stopping: orders may still rest at the venue"
            ),
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
    let step = |s: Said| {
        eprintln!("deploy: {}", s.en);
        if let Ok(mut p) = progress.lock() {
            p.steps.push((system::now_ms(), s));
            p.save(&cfg.state_dir);
        }
    };
    let finish = |outcome: Said, ok: bool| {
        if let Ok(mut p) = progress.lock() {
            p.running = false;
            p.outcome = Some(outcome.clone());
            p.save(&cfg.state_dir);
        }
        let _ = notify.send(Message {
            title: if ok {
                format!("{what} 完成")
            } else {
                format!("{what} 失败")
            },
            // The notification channel is read by a person at the time,
            // in Chinese; the record keeps both.
            body: outcome.zh,
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
    step(Said::new(
        format!("切换前的持仓：{before:?}"),
        format!("positions before: {before:?}"),
    ));
    let old = link_target(&cfg.releases.join("current"));
    if let Ok(mut r) = raised.lock() {
        r.trader_stopped_on_purpose = true;
    }
    step(Said::new(
        "停止交易进程（它会先撤掉自己的挂单）",
        "stopping the trader (it withdraws its own orders)",
    ));
    if let Err(e) = stop_trader(&cfg) {
        finish(
            Said::new(
                format!(
                    "已停止：{}。没有切换任何东西；交易进程保持停止，等人来看。",
                    e.zh
                ),
                format!(
                    "stopped: {}. Nothing was switched; the trader is left stopped for a person to look at.",
                    e.en
                ),
            ),
            false,
        );
        return;
    }
    if let Some(o) = &old
        && let Err(e) = relink(&cfg.releases, "previous", o)
    {
        finish(
            Said::new(
                format!("记不下上一个发布：{}", e.zh),
                format!("could not record the previous release: {}", e.en),
            ),
            false,
        );
        return;
    }
    if let Err(e) = relink(&cfg.releases, "current", &target) {
        finish(
            Said::new(
                format!("切换不了：{}", e.zh),
                format!("could not switch: {}", e.en),
            ),
            false,
        );
        return;
    }
    step(Said::new(
        format!("current -> {target}；正在启动"),
        format!("current -> {target}; starting"),
    ));
    let started = system::unit_action(&cfg.trader_unit, "start");
    if let Ok(mut r) = raised.lock() {
        r.trader_stopped_on_purpose = false;
    }
    let verdict = started.and_then(|()| {
        step(Said::new("检查健康状态", "checking health"));
        healthy(&cfg, &before)
    });
    match verdict {
        Ok(()) => finish(
            Said::new(
                format!("{target} 正在运行，健康"),
                format!("{target} is running and healthy"),
            ),
            true,
        ),
        Err(why) => {
            step(Said::new(
                format!("不健康：{}；正在回滚", why.zh),
                format!("unhealthy: {}; rolling back", why.en),
            ));
            let Some(o) = old else {
                finish(
                    Said::new(
                        format!("{target} 不健康（{}），而且没有可以回滚的版本", why.zh),
                        format!(
                            "{target} is unhealthy ({}) and there is nothing to roll back to",
                            why.en
                        ),
                    ),
                    false,
                );
                return;
            };
            let back = stop_trader(&cfg)
                .and_then(|()| relink(&cfg.releases, "current", &o))
                .and_then(|()| system::unit_action(&cfg.trader_unit, "start"));
            match back {
                Ok(()) => finish(
                    Said::new(
                        format!("{target} 不健康（{}）；已回滚到 {o}", why.zh),
                        format!("{target} was unhealthy ({}); rolled back to {o}", why.en),
                    ),
                    false,
                ),
                Err(e) => finish(
                    Said::new(
                        format!("{target} 不健康（{}），而且回滚失败：{}", why.zh, e.zh),
                        format!(
                            "{target} was unhealthy ({}) and the rollback failed: {}",
                            why.en, e.en
                        ),
                    ),
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
) -> Result<(), Said> {
    claim(progress, id)?;
    let installed = verify(cfg, id).and_then(|m| install(cfg, id, &m));
    if let Err(e) = installed {
        if let Ok(mut p) = progress.lock() {
            p.running = false;
            p.outcome = Some(e.clone());
            p.save(&cfg.state_dir);
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
) -> Result<(), Said> {
    let prev = link_target(&cfg.releases.join("previous")).ok_or(Said::new(
        "没有上一个发布可以回滚",
        "there is no previous release",
    ))?;
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

fn claim(progress: &Arc<Mutex<Progress>>, id: &str) -> Result<(), Said> {
    let mut p = progress
        .lock()
        .map_err(|_| Said::new("部署状态已损坏。", "deploy state poisoned"))?;
    if p.running {
        return Err(Said::new(
            format!("{} 的部署已经在进行中", p.id),
            format!("a deployment of {} is already running", p.id),
        ));
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
    #[test]
    fn the_last_deployment_outlives_the_agent_and_one_cut_short_says_so() {
        let d = tempfile::tempdir().expect("dir");
        let mut p = super::Progress {
            running: true,
            id: "r1".into(),
            steps: vec![(1, Said::new("正在停掉交易进程", "stopping the trader"))],
            outcome: None,
        };
        p.save(d.path());
        let back = super::Progress::load(d.path());
        assert!(!back.running, "nothing is running after a restart");
        assert_eq!(back.steps, p.steps);
        assert!(
            back.outcome
                .as_ref()
                .is_some_and(|o| o.en.starts_with("interrupted"))
        );
        p.running = false;
        p.outcome = Some(Said::new("r1 正在运行，健康", "r1 is running and healthy"));
        p.save(d.path());
        assert_eq!(super::Progress::load(d.path()).outcome, p.outcome);
        assert!(super::Progress::load(&d.path().join("none")).id.is_empty());
    }

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
            config_dir: root.join("config"),
            journals: root.join("journals"),
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
        assert!(verify(&c, "r2").unwrap_err().en.contains("does not match"));

        let other = tempfile::tempdir().expect("other");
        let c = cfg(other.path());
        stage(other.path(), "r3", false, false);
        assert!(verify(&c, "r3").unwrap_err().en.contains("signature"));
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
