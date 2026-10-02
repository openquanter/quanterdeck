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
mod selfcheck;
mod strategies;
mod system;

use std::sync::{Arc, Mutex};

use oq_deck_core::lang::Said;
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
    let notify = notify::start(discord.clone(), cfg.host.clone());
    // The channel is the one copy of this trail that is not on this
    // machine. An entry deleted from the **end** of the local file leaves
    // a chain that still verifies — the check walks the entries that are
    // there, and there is nothing after the last one to disagree with —
    // so what was posted is what says the trail used to be longer. Read
    // back at startup, which is when a deletion between runs shows.
    //
    // What it cannot see: whoever can write the trail can also post to
    // the channel, and could mirror a line to match. It catches the
    // tampering that did not think of the channel, and every accidental
    // truncation — which is most of what happens.
    if let Some(d) = discord.as_ref() {
        match d.recent(50) {
            Ok(bodies) => match audit::anchored_in(&bodies) {
                Some((seq, short)) => match audit.anchor_mismatch(seq, &short) {
                    None => println!("oq-agent: audit entry {seq} {short} matches the channel"),
                    Some(why) => {
                        eprintln!(
                            "oq-agent: the audit trail disagrees with the copy off this host: {}",
                            why.zh
                        );
                        let _ = notify.send(notify::Message {
                            title: "审计链与告警频道不一致".to_string(),
                            body: why.zh.clone(),
                            color: notify::RED,
                        });
                    }
                },
                // Said out loud rather than passed over: a check that
                // found nothing to compare against and a check that
                // agreed are not the same answer, and silence reads as
                // the second.
                None => println!(
                    "oq-agent: nothing in the channel's last 50 messages names an audit entry; \
                     the trail was not checked against anything"
                ),
            },
            Err(e) => eprintln!("oq-agent: the alert channel could not be read back: {e}"),
        }
    }
    // The journal's anchor, which is its record count.
    //
    // The trader's journal has no chain — a record carries a CRC and
    // nothing that names the one before it — so a deletion at the end
    // leaves a shorter file that reads perfectly. How many records there
    // were is the fact that survives such a deletion, and it only
    // survives off this host.
    {
        let (cfg, notify, discord) = (cfg.clone(), notify.clone(), discord.clone());
        // Checked before the loop starts, which is when a deletion
        // between runs shows.
        journal_anchor_check(&cfg, discord.as_ref(), &notify);
        std::thread::spawn(move || journal_anchor(&cfg, discord.as_ref()));
    }
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
    let last_deploy = deploy::Progress::load(&cfg.state_dir);
    let state = Arc::new(State {
        strategies: Mutex::new(strategies),
        cfg,
        replay: Mutex::new(guard::Replay::default()),
        step_up: Mutex::new(step_up),
        audit: Mutex::new(audit),
        raised,
        progress: Arc::new(Mutex::new(last_deploy)),
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
                        _ => AgentResponse::refused(Said::new("没有请求内容。", "no request line")),
                    }
                }
                Some(uid) => {
                    eprintln!("oq-agent: refused uid {uid}");
                    AgentResponse::refused(Said::new(
                        "这个 uid 无权使用主机代理。",
                        "this uid may not use the agent",
                    ))
                }
                None => AgentResponse::refused(Said::new(
                    "无法确认对端身份。",
                    "the peer could not be identified",
                )),
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
        Err(e) => {
            return AgentResponse::refused(Said::new(
                format!("请求读不出来：{e}"),
                format!("unreadable request: {e}"),
            ));
        }
    };
    let now = system::now_ms();
    if let Err(e) = state
        .replay
        .lock()
        .map_err(|_| Said::new("重放状态已损坏。", "replay state poisoned"))
        .and_then(|mut r| r.admit(&req.nonce, req.expires_ms, now))
    {
        return AgentResponse::refused(e);
    }
    let risk = req.op.risk();
    let reason = req.reason.clone().unwrap_or_default();
    let what = req.op.describe();
    if risk >= Risk::Reduce && reason.trim().is_empty() {
        return AgentResponse::refused(Said::new("需要填写原因。", "a reason is required"));
    }
    let mut credential = String::new();
    if risk == Risk::High {
        let verdict = state
            .step_up
            .lock()
            .map_err(|_| Said::new("二次验证状态已损坏。", "step-up state poisoned"))
            .and_then(|mut s| s.verify(req.step_up.as_deref(), now / 1000));
        if let Err(e) = verdict {
            // Refusals of risky requests are recorded too: repeated wrong
            // codes are what an intrusion looks like from here.
            let _ = record(
                state,
                &req.actor,
                &what,
                &Said::same(&reason),
                &Said::new(format!("已拒绝：{}", e.zh), format!("refused: {}", e.en)),
            );
            return AgentResponse::refused(e);
        }
        credential = " (step-up ok)".into();
    }
    let origin = format!("{}{credential}", req.actor);
    let cfg = &state.cfg;

    let result: Result<serde_json::Value, Said> = match &req.op {
        Op::Host => {
            // The host's name as alerts carry it, so the console names the
            // machine it is operating rather than leaving that implicit.
            let mut h = system::host_health();
            h["name"] = json!(cfg.host);
            // Which unit is the trader, so a page asking for its output
            // does not have to assume a deployment's naming.
            h["trader_unit"] = json!(cfg.trader_unit);
            Ok(h)
        }
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
            .map_err(|_| Said::new("审计状态已损坏。", "audit state poisoned")),
        Op::Alerts => state
            .raised
            .lock()
            .map(|r| json!({"active": alerts::list(&r), "history": alerts::history(&r)}))
            .map_err(|_| Said::new("告警状态已损坏。", "alert state poisoned")),
        Op::Releases => state
            .progress
            .lock()
            .map(|p| deploy::list(cfg, &p))
            .map_err(|_| Said::new("部署状态已损坏。", "deploy state poisoned")),
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
                return AgentResponse::refused(Said::new(
                    format!("{unit} 不是本代理监视的服务。"),
                    format!("{unit} is not a unit this agent watches"),
                ));
            }
            system::journal(unit, *since_ms, *until_ms, *lines, grep.as_deref())
        }
        Op::ConfigList => Ok(configs::list(&cfg.config_dir)),
        Op::ConfigGet { name, backup } => configs::get(&cfg.config_dir, name, backup.as_deref()),
        Op::Strategies => {
            // A read. Voiding an instance whose configuration moved is a
            // change to the gate's state, so it happens where changes
            // happen — in `advance`, before it reads the evidence — and
            // not here. The page still shows which configuration is in
            // force, because `config_sha_now` is beside `config_sha`.
            let s = match state.strategies.lock() {
                Ok(s) => s,
                Err(_) => {
                    return AgentResponse::refused(Said::new(
                        "策略状态已损坏。",
                        "strategy state poisoned",
                    ));
                }
            };
            Ok(s.list(&cfg.config_dir, &cfg.journals, now))
        }
        // State-changing requests are written down before they are done,
        // and not done if they cannot be written down.
        mutating => {
            if let Err(e) = record(
                state,
                &req.actor,
                &what,
                &Said::same(&reason),
                &Said::new("已请求", "requested"),
            ) {
                return AgentResponse::refused(Said::new(
                    format!("没有执行：审计记录写不下去：{e}"),
                    format!("not done: the audit trail could not be written: {e}"),
                ));
            }
            let outcome = act(state, mutating, &origin, &reason);
            let result = match &outcome {
                Ok(_) => Said::new("已完成", "done"),
                Err(e) => Said::new(format!("失败：{}", e.zh), format!("failed: {}", e.en)),
            };
            let _ = record(state, &req.actor, &what, &Said::same(&reason), &result);
            outcome
        }
    };
    match result {
        Ok(data) => {
            if let Some(false) = data.get("ok").and_then(serde_json::Value::as_bool) {
                // The trader's own words: one language, shown as they are.
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

fn act(state: &State, op: &Op, origin: &str, reason: &str) -> Result<serde_json::Value, Said> {
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
                return Err(Said::new(
                    format!("{unit} 不是本代理管理的服务。"),
                    format!("{unit} is not a unit this agent manages"),
                ));
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
                .map_err(|_| Said::new("告警状态已损坏。", "alert state poisoned"))
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
            .map_err(|_| Said::new("策略状态已损坏。", "strategy state poisoned"))
            .and_then(|mut s| s.create(name, config, &cfg.config_dir, origin, system::now_ms())),
        Op::StrategyBacktest { id, run, passed } => state
            .strategies
            .lock()
            .map_err(|_| Said::new("策略状态已损坏。", "strategy state poisoned"))
            .and_then(|mut s| {
                s.evidence_backtest(id, run, *passed, &cfg.config_dir, &cfg.journals)
            }),
        Op::StrategyAdvance { id } => state
            .strategies
            .lock()
            .map_err(|_| Said::new("策略状态已损坏。", "strategy state poisoned"))
            .and_then(|mut s| {
                s.advance(
                    id,
                    origin,
                    reason,
                    &cfg.config_dir,
                    &cfg.journals,
                    system::now_ms(),
                )
            }),
        other => Err(Said::new(
            format!("{} 不是一个操作。", other.describe()),
            format!("{} is not an action", other.describe()),
        )),
    }
}

/// How often the journal's head is mirrored. An hour is often enough to
/// put a bound on what a deletion could hide, and rare enough that a
/// monitoring channel does not become a log file.
const ANCHOR_EVERY: std::time::Duration = std::time::Duration::from_secs(3600);

/// Mirror the trader's journal head off this host, and check the last one
/// it mirrored.
///
/// The head is the newest journal's record count. Posted, it says how
/// many records existed at that moment; a file that is now shorter has
/// had some taken off the end of it, which is the one thing a reader of
/// that journal cannot see for itself.
///
/// What it cannot see is the same bound the audit trail's anchor has:
/// whoever can write the journal can also post to the channel.
fn journal_anchor(cfg: &Config, discord: Option<&notify::Discord>) {
    let mut last: Option<(String, usize)> = None;
    loop {
        if let Some((name, count)) = newest_journal_head(&cfg.journals)
            && last.as_ref() != Some(&(name.clone(), count))
        {
            // Posted, not written down here: an anchor on the machine it
            // is meant to be evidence about is not an anchor.
            if let Some(d) = discord {
                let body = format!("journal {name} #{count}");
                match d.send(
                    &Message {
                        title: "journal 头".to_string(),
                        body: body.clone(),
                        color: BLUE,
                    },
                    &cfg.host,
                ) {
                    Ok(()) => println!("oq-agent: journal anchor posted: {body}"),
                    Err(e) => eprintln!("oq-agent: the journal anchor was not posted: {e}"),
                }
            } else {
                println!("oq-agent: journal anchor {name} {count} (no channel to post it to)");
            }
            last = Some((name, count));
        }
        std::thread::sleep(ANCHOR_EVERY);
    }
}

/// The head the channel last said the journal had, against what it holds
/// now.
fn journal_anchor_check(
    cfg: &Config,
    discord: Option<&notify::Discord>,
    notify: &std::sync::mpsc::Sender<Message>,
) {
    let Some(d) = discord else {
        println!("oq-agent: no channel, so the journal's head was not checked against anything");
        return;
    };
    let bodies = match d.recent(50) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("oq-agent: the alert channel could not be read back: {e}");
            return;
        }
    };
    let Some((name, was)) = anchored_journal_in(&bodies) else {
        println!("oq-agent: nothing in the channel's last 50 messages names a journal head");
        return;
    };
    let Some((now_name, now)) = newest_journal_head(&cfg.journals) else {
        return;
    };
    if now_name == name && now < was {
        let why = format!(
            "journal {name} had {was} records when this host last said so and has {now} now; \
             records were taken off the end"
        );
        eprintln!("oq-agent: {why}");
        let _ = notify.send(Message {
            title: "交易进程的 journal 变短了".to_string(),
            body: why,
            color: notify::RED,
        });
    } else {
        println!("oq-agent: journal {name} has {now} records; the channel said {was}");
    }
}

/// The newest journal head a set of mirrored messages names.
fn anchored_journal_in(bodies: &[String]) -> Option<(String, usize)> {
    let mut newest: Option<(String, usize)> = None;
    for body in bodies {
        let Some(rest) = body.trim().strip_prefix("journal ") else {
            continue;
        };
        let Some((name, count)) = rest.rsplit_once(" #") else {
            continue;
        };
        let Ok(count) = count.trim().parse::<usize>() else {
            continue;
        };
        newest = Some((name.to_string(), count));
        break; // newest first
    }
    newest
}

/// The newest journal's name and how many records it holds.
fn newest_journal_head(journals: &std::path::Path) -> Option<(String, usize)> {
    let mut newest: Option<(std::time::SystemTime, String)> = None;
    for entry in std::fs::read_dir(journals).ok()?.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "oqj") {
            continue;
        }
        let Some(name) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        let Ok(at) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        if newest.as_ref().is_none_or(|(when, _)| at > *when) {
            newest = Some((at, name));
        }
    }
    let (_, name) = newest?;
    let page = oq_deck_core::live::records(journals, &name, &[], 1, None).ok()?;
    Some((name, page.total))
}

/// Append to the audit trail and mirror it off the host.
fn record(
    state: &State,
    actor: &str,
    op: &str,
    reason: &Said,
    result: &Said,
) -> Result<(), String> {
    let entry = state
        .audit
        .lock()
        .map_err(|_| "audit state poisoned".to_string())?
        .append(system::now_ms(), actor, op, reason, result)?;
    let _ = state.notify.send(Message {
        title: format!("操作 · {op}"),
        body: format!(
            "{actor}：{}\n结果：{}\n审计 #{} {}",
            reason.zh,
            result.zh,
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

#[cfg(test)]
mod anchor_tests {
    use super::*;

    /// The anchor is whatever the channel still has, so the reader has
    /// to find it among the messages that are not anchors.
    #[test]
    fn the_newest_posted_journal_head_is_the_anchor() {
        let bodies: Vec<String> = [
            "deck：halt\n结果：halted\n审计 #4 aaaaaaaaaaaa",
            "journal oqp-live-20260926-000000 #41",
            "a log line that happens to say journal something",
            "journal oqp-live-20260925-000000 #7",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        assert_eq!(
            anchored_journal_in(&bodies),
            Some(("oqp-live-20260926-000000".to_string(), 41))
        );
        assert_eq!(anchored_journal_in(&[]), None);
        assert_eq!(
            anchored_journal_in(&["journal x #notanumber".to_string()]),
            None
        );
    }
}
