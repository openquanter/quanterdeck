//! Strategy instances on the road from draft to live (UI-BRIEF §6).
//!
//! The stages and the rule for each step are `oq_deck_core::gate`'s; this
//! keeps the instances, gathers the evidence, and applies the one rule
//! that runs backwards: a configuration that changed since the evidence
//! was taken voids it, and the instance goes back to draft — including
//! one already live.
//!
//! "Configuration" is the strategy's config file as this agent holds it:
//! the evidence records its hash when a backtest is attached, and any
//! later write to that file is a different configuration. The file is
//! only written through this agent, so the hash cannot move unseen.

use std::path::{Path, PathBuf};

use oq_deck_core::gate::{self, Evidence, Stage};
use oq_deck_core::lang::Said;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Observation required before a sign-off, and fills within it.
pub const OBSERVATION_HOURS: i64 = gate::DEFAULT_OBSERVATION_HOURS;
pub const MIN_FILLS: usize = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub at_ms: i64,
    pub from: Stage,
    pub to: Stage,
    pub actor: String,
    /// Why this step was taken, in Chinese — or, when the operator typed
    /// it, in their own words, which [`Step::reason_en`] repeats.
    pub reason: String,
    /// The same reason in English. Steps written before the console had
    /// two languages carry only one; a reader falls back to `reason`.
    #[serde(default)]
    pub reason_en: String,
}

impl Step {
    /// The step's reason as the pair it is.
    fn said(at_ms: i64, from: Stage, to: Stage, actor: &str, why: &Said) -> Self {
        Self {
            at_ms,
            from,
            to,
            actor: actor.into(),
            reason: why.zh.clone(),
            reason_en: why.en.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    /// The config file, in the agent's config directory.
    pub config: String,
    pub stage: Stage,
    pub backtest_run: Option<String>,
    pub backtest_passed: bool,
    /// The config file's hash when the backtest was attached.
    pub config_sha: Option<String>,
    pub observing_since_ms: Option<i64>,
    pub confirmed_by: Option<String>,
    pub history: Vec<Step>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Store {
    instances: Vec<Instance>,
}

pub struct Strategies {
    path: PathBuf,
    store: Store,
}

fn config_sha(config_dir: &Path, name: &str) -> Option<String> {
    let v = crate::configs::get(config_dir, name, None).ok()?;
    v["sha"].as_str().map(str::to_string)
}

/// Fills booked in any journal in `dir` at or after `since_ms`.
fn fills_since(dir: &Path, since_ms: i64) -> usize {
    let since_ns = since_ms.saturating_mul(1_000_000);
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter_map(|e| {
                    let p = e.path();
                    (p.extension().is_some_and(|x| x == "oqj"))
                        .then(|| p.file_stem().map(|s| s.to_string_lossy().to_string()))
                        .flatten()
                })
                .map(|id| {
                    oq_deck_core::live::records(
                        dir,
                        &id,
                        &["fill".to_string()],
                        oq_deck_core::live::MAX_PAGE,
                        None,
                    )
                    .map(|page| {
                        page.records
                            .iter()
                            .filter(|r| r.at.unwrap_or(0) >= since_ns)
                            .count()
                    })
                    .unwrap_or(0)
                })
                .sum()
        })
        .unwrap_or(0)
}

impl Strategies {
    /// # Errors
    /// A store that exists and will not parse: carrying on would drop it.
    pub fn open(state_dir: &Path) -> Result<Self, String> {
        let path = state_dir.join("strategies.json");
        let store = match std::fs::read_to_string(&path) {
            Ok(t) => serde_json::from_str(&t).map_err(|e| format!("{}: {e}", path.display()))?,
            Err(_) => Store::default(),
        };
        Ok(Self { path, store })
    }

    fn save(&self) -> Result<(), Said> {
        let tmp = self.path.with_extension("json.new");
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(&self.store).map_err(|e| Said::same(e.to_string()))?,
        )
        .and_then(|()| std::fs::rename(&tmp, &self.path))
        .map_err(|e| Said::same(e.to_string()))
    }

    fn evidence(i: &Instance, journals: &Path, now_ms: i64) -> Evidence {
        let (hours, fills) = match i.observing_since_ms {
            Some(since) => ((now_ms - since) / 3_600_000, fills_since(journals, since)),
            None => (0, 0),
        };
        Evidence {
            backtest_run: i.backtest_run.clone(),
            backtest_passed: i.backtest_passed,
            backtest_manifest: None,
            observation_hours: hours,
            observation_fills: fills,
            confirmed_by: i.confirmed_by.clone(),
        }
    }

    /// Send back to draft every instance whose configuration moved since
    /// its evidence. Returns what was voided, for the audit trail.
    pub fn void_changed(&mut self, config_dir: &Path, now_ms: i64) -> Vec<(String, Said)> {
        let mut voided = Vec::new();
        for i in &mut self.store.instances {
            let (Some(at), Some(now)) = (i.config_sha.clone(), config_sha(config_dir, &i.config))
            else {
                continue;
            };
            if i.stage != Stage::Draft && at != now {
                let reason = Said::new(
                    format!("配置文件 {} 在取得证据之后改过，证据作废", i.config),
                    format!(
                        "the config file {} changed after the evidence was taken; that evidence is void",
                        i.config
                    ),
                );
                i.history.push(Step::said(
                    now_ms,
                    i.stage,
                    Stage::Draft,
                    "oq-agent",
                    &reason,
                ));
                i.stage = Stage::Draft;
                i.backtest_run = None;
                i.backtest_passed = false;
                i.config_sha = None;
                i.observing_since_ms = None;
                i.confirmed_by = None;
                voided.push((i.id.clone(), reason));
            }
        }
        if !voided.is_empty() {
            let _ = self.save();
        }
        voided
    }

    /// Every instance with its evidence and whether the next step is open.
    #[must_use]
    pub fn list(&self, config_dir: &Path, journals: &Path, now_ms: i64) -> Value {
        json!(
            self.store
                .instances
                .iter()
                .map(|i| {
                    let e = Self::evidence(i, journals, now_ms);
                    let next = gate::advance(i.stage, &e, OBSERVATION_HOURS, MIN_FILLS);
                    json!({
                        "instance": i,
                        "evidence": {
                            "observation_hours": e.observation_hours,
                            "observation_fills": e.observation_fills,
                            "required_hours": OBSERVATION_HOURS,
                            "required_fills": MIN_FILLS,
                        },
                        "config_sha_now": config_sha(config_dir, &i.config),
                        "next": i.stage.next(),
                        "decision": next,
                    })
                })
                .collect::<Vec<_>>()
        )
    }

    fn find(&mut self, id: &str) -> Result<&mut Instance, Said> {
        self.store
            .instances
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or_else(|| {
                Said::new(
                    format!("没有实例 {id}。"),
                    format!("there is no instance {id}"),
                )
            })
    }

    /// # Errors
    /// A duplicate id or a config that is not there.
    pub fn create(
        &mut self,
        name: &str,
        config: &str,
        config_dir: &Path,
        actor: &str,
        now_ms: i64,
    ) -> Result<Value, Said> {
        config_sha(config_dir, config).ok_or_else(|| {
            Said::new(
                format!("没有配置文件 {config}。"),
                format!("there is no config file {config}"),
            )
        })?;
        let id: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        if id.trim_matches('-').is_empty() || self.store.instances.iter().any(|i| i.id == id) {
            return Err(Said::new(
                format!("实例 id {id:?} 是空的，或者已经有人用了。"),
                format!("the instance id {id:?} is empty or already taken"),
            ));
        }
        let i = Instance {
            id: id.clone(),
            name: name.into(),
            config: config.into(),
            stage: Stage::Draft,
            backtest_run: None,
            backtest_passed: false,
            config_sha: None,
            observing_since_ms: None,
            confirmed_by: None,
            history: vec![Step::said(
                now_ms,
                Stage::Draft,
                Stage::Draft,
                actor,
                &Said::new("已建立", "created"),
            )],
        };
        self.store.instances.push(i);
        self.save()?;
        Ok(json!({"id": id}))
    }

    /// Attach the backtest that justifies the current configuration.
    ///
    /// # Errors
    /// No such instance, run or config.
    pub fn evidence_backtest(
        &mut self,
        id: &str,
        run: &str,
        passed: bool,
        config_dir: &Path,
        runs_dir: &Path,
    ) -> Result<Value, Said> {
        oq_deck_core::runs::detail(runs_dir, run)
            .map_err(|e| Said::same(format!("run {run}: {e}")))?;
        let sha = {
            let config = self.find(id)?.config.clone();
            config_sha(config_dir, &config).ok_or(Said::new(
                "配置文件不在了。",
                "the config file is not there",
            ))?
        };
        let i = self.find(id)?;
        i.backtest_run = Some(run.into());
        i.backtest_passed = passed;
        i.config_sha = Some(sha);
        self.save()?;
        Ok(json!({"id": id, "backtest_run": run, "passed": passed}))
    }

    /// One step forward, if the gate allows it.
    ///
    /// # Errors
    /// The gate's reason, verbatim.
    pub fn advance(
        &mut self,
        id: &str,
        actor: &str,
        reason: &str,
        journals: &Path,
        now_ms: i64,
    ) -> Result<Value, Said> {
        let e = {
            let i = self.find(id)?;
            Self::evidence(i, journals, now_ms)
        };
        let i = self.find(id)?;
        let decision = gate::advance(i.stage, &e, OBSERVATION_HOURS, MIN_FILLS);
        if !decision.allowed {
            return Err(decision.reason);
        }
        let to = i.stage.next().ok_or(Said::new(
            "这个实例已经是实盘了。",
            "this instance is already live",
        ))?;
        // Confirmation is a person's act, recorded as the step itself.
        if to == Stage::Confirmed {
            i.confirmed_by = Some(actor.into());
        }
        if to == Stage::Observing {
            i.observing_since_ms = Some(now_ms);
        }
        // A person's reason for the step, in the words they used.
        i.history
            .push(Step::said(now_ms, i.stage, to, actor, &Said::same(reason)));
        i.stage = to;
        self.save()?;
        Ok(json!({"id": id, "stage": to}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_road_is_walked_one_step_at_a_time_and_a_config_change_sends_it_back() {
        let state = tempfile::tempdir().expect("state");
        let config = tempfile::tempdir().expect("config");
        let journals = tempfile::tempdir().expect("journals");
        std::fs::write(config.path().join("s.json"), r#"{"a":1}"#).expect("config");
        let mut s = Strategies::open(state.path()).expect("open");
        s.create("Stable AG", "s.json", config.path(), "deck", 0)
            .expect("create");

        // No backtest: the first step is refused, with the reason.
        let refused = s
            .advance("stable-ag", "deck", "go", journals.path(), 1)
            .unwrap_err();
        assert!(refused.en.contains("no backtest"), "{refused:?}");

        // A backtest from the runs directory: fixtures stand in.
        let runs = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/fixtures/runs");
        s.evidence_backtest("stable-ag", "baseline", true, config.path(), &runs)
            .expect("evidence");
        s.advance("stable-ag", "deck", "backtest read", journals.path(), 2)
            .expect("backtested");
        s.advance("stable-ag", "deck", "to testnet", journals.path(), 3)
            .expect("observing");
        let early = s
            .advance("stable-ag", "deck", "too soon", journals.path(), 4)
            .unwrap_err();
        assert!(early.en.contains("observation window"), "{early:?}");

        // The config file changes: everything goes back to draft.
        let sha = crate::configs::get(config.path(), "s.json", None).expect("get")["sha"]
            .as_str()
            .expect("sha")
            .to_string();
        crate::configs::put(config.path(), "s.json", r#"{"a":2}"#, &sha, 5).expect("put");
        let voided = s.void_changed(config.path(), 6);
        assert_eq!(voided.len(), 1);
        let listed = s.list(config.path(), journals.path(), 7);
        assert_eq!(listed[0]["instance"]["stage"], "draft");
        // The gate's refusal travels as a pair, so the console reads it
        // in the language its reader asked for.
        assert!(
            listed[0]["decision"]["reason"]["en"]
                .as_str()
                .expect("reason")
                .contains("no backtest")
        );
        assert!(
            listed[0]["decision"]["reason"]["zh"]
                .as_str()
                .expect("reason")
                .contains("回测")
        );

        // And the store survives a restart.
        let reopened = Strategies::open(state.path()).expect("reopen");
        assert_eq!(reopened.store.instances[0].history.len(), 4);
    }
}
