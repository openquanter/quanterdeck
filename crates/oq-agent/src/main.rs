//! oq-agent: the host side of quanterdeck.
//!
//! Runs on the trading host as its own user. It can read what the trader
//! reports and its logs, start and stop the trading units that polkit lets
//! it, command the trader through its control port, switch releases that
//! carry a trusted signature, and post alerts. It cannot read the venue
//! keys, the trader's memory or environment, or anyone's home directory,
//! and it runs nothing it was not built to run.
//!
//! One JSON request per connection on a Unix socket in its runtime
//! directory, from the users named in `OQ_AGENT_PEERS` only — checked by
//! the uid the kernel reports.

mod alerts;
mod audit;
mod blackbox;
mod config;
mod configs;
mod control;
mod deploy;
mod guard;
mod notify;
mod strategies;
mod system;

use std::sync::{Arc, Mutex};

use oq_deck_core::ops::{AgentRequest, AgentResponse, Op, Risk};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use crate::config::Config;
use crate::notify::{BLUE, Message};

struct State {
    cfg: Config,
    replay: Mutex<guard::Replay>,
    step_up: Mutex<guard::StepUp>,
    audit: Mutex<audit::Audit>,
    raised: Arc<Mutex<alerts::Raised>>,
    strategies: Mutex<strategies::Strategies>,
    progress: Arc<Mutex<deploy::Progress>>,
    notify: std::sync::mpsc::Sender<Message>,
}

/// Longest request line.
const MAX_REQUEST: u64 = 16 * 1024;

fn main() {
    let cfg = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("oq-agent: {e}");
            std::process::exit(2);
        }
    };
    let audit = match audit::Audit::open(&cfg.state_dir) {
        Ok(a) => a,
        Err(e) => {
            // A broken chain is someone's doing; carrying on would append
            // to it as if nothing happened.
            eprintln!("oq-agent: refusing to start: the audit trail is broken: {e}");
            std::process::exit(3);
        }
    };
    let step_up = guard::StepUp::new(cfg.credential("STEP_UP_TOTP"));
    if !step_up.configured() {
        eprintln!("oq-agent: no STEP_UP_TOTP credential; high-risk actions are refused");
    }
    let discord = match (
        cfg.credential("DISCORD_BOT_TOKEN"),
        cfg.discord_guild.clone(),
    ) {
        (Some(token), Some(guild)) => Some(notify::Discord::new(
            token,
            guild,
            cfg.discord_channel.clone(),
            cfg.proxy.clone(),
        )),
        _ => {
            eprintln!("oq-agent: no Discord credential or guild; alerts are only printed");
            None
        }
    };
    let notify = notify::start(discord, cfg.host.clone());
    let raised = Arc::new(Mutex::new(alerts::Raised::default()));
    {
        let (cfg, raised, notify) = (cfg.clone(), Arc::clone(&raised), notify.clone());
        std::thread::spawn(move || alerts::watch(cfg, raised, notify));
    }
    let strategies = match strategies::Strategies::open(&cfg.state_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("oq-agent: refusing to start: {e}");
            std::process::exit(3);
        }
    };
    let state = Arc::new(State {
        strategies: Mutex::new(strategies),
        cfg,
        replay: Mutex::new(guard::Replay::default()),
        step_up: Mutex::new(step_up),
        audit: Mutex::new(audit),
        raised,
        progress: Arc::new(Mutex::new(deploy::Progress::default())),
        notify,
    });

    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("oq-agent: {e}");
            std::process::exit(2);
        }
    };
    if let Err(e) = rt.block_on(serve(state)) {
        eprintln!("oq-agent: {e}");
        std::process::exit(1);
    }
}

async fn serve(state: Arc<State>) -> Result<(), String> {
    let path = state.cfg.socket.clone();
    // A stale socket from this service's last run; anything else is refused.
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        use std::os::unix::fs::FileTypeExt;
        if meta.file_type().is_socket() {
            let _ = std::fs::remove_file(&path);
        } else {
            return Err(format!("{} exists and is not a socket", path.display()));
        }
    }
    let listener =
        tokio::net::UnixListener::bind(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o660))
            .map_err(|e| e.to_string())?;
    }
    eprintln!("oq-agent: listening on {}", path.display());
    loop {
        let (stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let uid = stream.peer_cred().map(|c| c.uid()).ok();
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            let (read, mut write) = stream.into_split();
            let answer = match uid {
                Some(uid) if state.cfg.peers.contains(&uid) => {
                    let mut line = String::new();
                    let mut reader = BufReader::new(read.take(MAX_REQUEST));
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(10),
                        reader.read_line(&mut line),
                    )
                    .await
                    {
                        Ok(Ok(_)) => {
                            let state = Arc::clone(&state);
                            tokio::task::spawn_blocking(move || handle(&state, &line))
                                .await
                                .unwrap_or_else(|e| AgentResponse::refused(e.to_string()))
                        }
                        _ => AgentResponse::refused("no request line"),
                    }
                }
                Some(uid) => {
                    eprintln!("oq-agent: refused uid {uid}");
                    AgentResponse::refused("this uid may not use the agent")
                }
                None => AgentResponse::refused("the peer could not be identified"),
            };
            let mut out = serde_json::to_string(&answer).unwrap_or_else(|_| "{}".into());
            out.push('\n');
            let _ = write.write_all(out.as_bytes()).await;
        });
    }
}

fn handle(state: &State, line: &str) -> AgentResponse {
    let req: AgentRequest = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => return AgentResponse::refused(format!("unreadable request: {e}")),
    };
    let now = system::now_ms();
    if let Err(e) = state
        .replay
        .lock()
        .map_err(|_| "replay state poisoned".to_string())
        .and_then(|mut r| r.admit(&req.nonce, req.expires_ms, now))
    {
        return AgentResponse::refused(e);
    }
    let risk = req.op.risk();
    let reason = req.reason.clone().unwrap_or_default();
    let what = req.op.describe();
    if risk >= Risk::Reduce && reason.trim().is_empty() {
        return AgentResponse::refused("a reason is required");
    }
    let mut credential = String::new();
    if risk == Risk::High {
        let verdict = state
            .step_up
            .lock()
            .map_err(|_| "step-up state poisoned".to_string())
            .and_then(|mut s| s.verify(req.step_up.as_deref(), now / 1000));
        if let Err(e) = verdict {
            // Refusals of risky requests are recorded too: repeated wrong
            // codes are what an intrusion looks like from here.
            let _ = record(state, &req.actor, &what, &reason, &format!("refused: {e}"));
            return AgentResponse::refused(e);
        }
        credential = " (step-up ok)".into();
    }
    let origin = format!("{}{credential}", req.actor);
    let cfg = &state.cfg;

    let result: Result<serde_json::Value, String> = match &req.op {
        Op::Host => Ok(system::host_health()),
        Op::Units => Ok(json!(
            cfg.units
                .iter()
                .map(|u| {
                    let mut s = system::unit_state(u);
                    s["manageable"] = json!(cfg.manageable.contains(u));
                    s
                })
                .collect::<Vec<_>>()
        )),
        Op::Status => control::ask(&cfg.control_dir, "status", &origin, ""),
        Op::Orders => control::ask(&cfg.control_dir, "orders", &origin, ""),
        Op::Metrics => control::ask(&cfg.control_dir, "metrics", &origin, ""),
        Op::Attribution => control::ask(&cfg.control_dir, "attribution", &origin, ""),
        Op::Logs => Ok(system::log_files(&cfg.log_dir)),
        Op::LogTail { name, lines, grep } => {
            system::tail(&cfg.log_dir, name, *lines, grep.as_deref())
        }
        Op::Audit { lines } => state
            .audit
            .lock()
            .map(|a| a.tail(*lines))
            .map_err(|_| "audit state poisoned".to_string()),
        Op::Alerts => state
            .raised
            .lock()
            .map(|r| json!({"active": alerts::list(&r), "history": alerts::history(&r)}))
            .map_err(|_| "alert state poisoned".to_string()),
        Op::Releases => state
            .progress
            .lock()
            .map(|p| deploy::list(cfg, &p))
            .map_err(|_| "deploy state poisoned".to_string()),
        Op::Accounts => Ok(accounts(&cfg.log_dir, &cfg.manageable)),
        Op::Resources { hours } => Ok(blackbox::resources(
            &cfg.state_dir,
            now,
            *hours.clamp(&1, &(blackbox::KEEP_DAYS * 24)),
            &cfg.units,
        )),
        Op::Blackbox {
            from_ms,
            to_ms,
            points,
        } => {
            // A week at a time at most: a review asks about a moment, and a
            // request for months would read every file there is.
            let from = (*from_ms).max(to_ms - 7 * 86_400_000);
            Ok(blackbox::window(
                &cfg.state_dir,
                from,
                *to_ms,
                (*points).clamp(10, 2000),
            ))
        }
        Op::BlackboxAt { at_ms } => Ok(blackbox::at(&cfg.state_dir, *at_ms)),
        Op::JournalLog {
            unit,
            since_ms,
            until_ms,
            lines,
            grep,
        } => {
            if !cfg.units.contains(unit) {
                return AgentResponse::refused(format!("{unit} is not a unit this agent watches"));
            }
            system::journal(unit, *since_ms, *until_ms, *lines, grep.as_deref())
        }
        Op::ConfigList => Ok(configs::list(&cfg.config_dir)),
        Op::ConfigGet { name, backup } => configs::get(&cfg.config_dir, name, backup.as_deref()),
        Op::Strategies => {
            let mut s = match state.strategies.lock() {
                Ok(s) => s,
                Err(_) => return AgentResponse::refused("strategy state poisoned"),
            };
            for (id, why) in s.void_changed(&cfg.config_dir, now) {
                let _ = record(
                    state,
                    "oq-agent",
                    &format!("strategy {id} back to draft"),
                    &why,
                    "voided",
                );
            }
            Ok(s.list(&cfg.config_dir, &cfg.journals, now))
        }
        // State-changing requests are written down before they are done,
        // and not done if they cannot be written down.
        mutating => {
            if let Err(e) = record(state, &req.actor, &what, &reason, "requested") {
                return AgentResponse::refused(format!(
                    "not done: the audit trail could not be written: {e}"
                ));
            }
            let outcome = act(state, mutating, &origin, &reason);
            let result = match &outcome {
                Ok(_) => "done".to_string(),
                Err(e) => format!("failed: {e}"),
            };
            let _ = record(state, &req.actor, &what, &reason, &result);
            outcome
        }
    };
    match result {
        Ok(data) => {
            if let Some(false) = data.get("ok").and_then(serde_json::Value::as_bool) {
                return AgentResponse::refused(
                    data["error"]
                        .as_str()
                        .unwrap_or("refused by the trader")
                        .to_string(),
                );
            }
            AgentResponse::ok(data)
        }
        Err(e) => AgentResponse::refused(e),
    }
}

fn act(state: &State, op: &Op, origin: &str, reason: &str) -> Result<serde_json::Value, String> {
    let cfg = &state.cfg;
    let set_stopped = |on: bool| {
        if let Ok(mut r) = state.raised.lock() {
            r.trader_stopped_on_purpose = on;
        }
    };
    match op {
        Op::Halt => control::ask(&cfg.control_dir, "halt", origin, reason),
        Op::Resume => control::ask(&cfg.control_dir, "resume", origin, reason),
        Op::Shutdown => {
            set_stopped(true);
            control::ask(&cfg.control_dir, "shutdown", origin, reason)
        }
        Op::Unit { unit, verb } => {
            if !cfg.manageable.contains(unit) {
                return Err(format!("{unit} is not a unit this agent manages"));
            }
            if *unit == cfg.trader_unit {
                set_stopped(verb == "stop");
            }
            system::unit_action(unit, verb).map(|()| json!({"unit": unit, "verb": verb}))
        }
        Op::Deploy { id } => {
            deploy::start_deploy(cfg, id, &state.progress, &state.notify, &state.raised)
                .map(|()| json!({"started": id}))
        }
        Op::Rollback => deploy::start_rollback(cfg, &state.progress, &state.notify, &state.raised)
            .map(|()| json!({"started": "rollback"})),
        Op::AlertTest => {
            let _ = state.notify.send(Message {
                title: "测试消息".into(),
                body: format!("来自 deck 的告警渠道测试：{reason}"),
                color: BLUE,
            });
            Ok(json!({"sent": true}))
        }
        Op::AlertSilence { key, minutes } => {
            let until = system::now_ms() + minutes.clamp(&1, &(7 * 24 * 60)) * 60_000;
            state
                .raised
                .lock()
                .map(|mut r| {
                    r.silenced.insert(key.clone(), until);
                    json!({"key": key, "silenced_until_ms": until})
                })
                .map_err(|_| "alert state poisoned".to_string())
        }
        Op::ConfigPut {
            name,
            content,
            base_sha,
        } => configs::put(&cfg.config_dir, name, content, base_sha, system::now_ms()),
        Op::ConfigRollback {
            name,
            backup,
            base_sha,
        } => configs::rollback(&cfg.config_dir, name, backup, base_sha, system::now_ms()),
        Op::StrategyCreate { name, config } => state
            .strategies
            .lock()
            .map_err(|_| "strategy state poisoned".to_string())
            .and_then(|mut s| s.create(name, config, &cfg.config_dir, origin, system::now_ms())),
        Op::StrategyBacktest { id, run, passed } => state
            .strategies
            .lock()
            .map_err(|_| "strategy state poisoned".to_string())
            .and_then(|mut s| {
                s.evidence_backtest(id, run, *passed, &cfg.config_dir, &cfg.journals)
            }),
        Op::StrategyAdvance { id } => state
            .strategies
            .lock()
            .map_err(|_| "strategy state poisoned".to_string())
            .and_then(|mut s| s.advance(id, origin, reason, &cfg.journals, system::now_ms())),
        other => Err(format!("{} is not an action", other.describe())),
    }
}

/// Append to the audit trail and mirror it off the host.
fn record(state: &State, actor: &str, op: &str, reason: &str, result: &str) -> Result<(), String> {
    let entry = state
        .audit
        .lock()
        .map_err(|_| "audit state poisoned".to_string())?
        .append(system::now_ms(), actor, op, reason, result)?;
    let _ = state.notify.send(Message {
        title: format!("操作 · {op}"),
        body: format!(
            "{actor}：{reason}\n结果：{result}\n审计 #{} {}",
            entry["seq"],
            entry["hash"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(12)
                .collect::<String>()
        ),
        color: BLUE,
    });
    Ok(())
}

/// Which venue account each process uses, from the fingerprint it writes
/// at every start — from the systemd journal, and from the log files of
/// the runs before output went there. Two processes on different accounts
/// — a watcher reading a key that is not the trader's — is the failure
/// this exists to show.
fn accounts(log_dir: &std::path::Path, units: &[String]) -> serde_json::Value {
    let mut latest: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    for unit in units {
        let found = system::journal(unit, None, None, 1, Some("account key"))
            .ok()
            .and_then(|v| v["lines"].as_array().and_then(|l| l.last().cloned()))
            .and_then(|l| l.as_str().map(str::to_string));
        if let Some(line) = found {
            let fingerprint = line
                .split("account key")
                .nth(1)
                .and_then(|rest| rest.split_whitespace().next())
                .map(str::to_string);
            latest.insert(
                unit.trim_end_matches(".service").to_string(),
                json!({"log": format!("journal: {unit}"), "fingerprint": fingerprint, "line": line}),
            );
        }
    }
    let files = system::log_files(log_dir);
    for f in files.as_array().cloned().unwrap_or_default() {
        let name = f["name"].as_str().unwrap_or_default().to_string();
        let Some(process) = name
            .rsplit_once('-')
            .and_then(|(a, _)| a.rsplit_once('-'))
            .map(|(p, _)| p.to_string())
        else {
            continue;
        };
        if latest.contains_key(&process) {
            continue;
        }
        let line = system::first_line(log_dir, &name).unwrap_or_default();
        let fingerprint = line.strip_prefix("account key").map(|rest| {
            rest.split_whitespace()
                .next()
                .unwrap_or_default()
                .to_string()
        });
        latest.insert(
            process,
            json!({"log": name, "fingerprint": fingerprint, "line": line}),
        );
    }
    let prints: std::collections::BTreeSet<String> = latest
        .values()
        .filter_map(|v| v["fingerprint"].as_str().map(str::to_string))
        .collect();
    json!({"processes": latest, "same_account": prints.len() <= 1})
}
