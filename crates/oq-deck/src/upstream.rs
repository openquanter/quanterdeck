//! The upstream release check: the deck's one outbound request.
//!
//! What a reply means lives in `oq_deck_core::upstream`; this is the
//! part with a clock, a socket and a schedule. A check runs shortly
//! after startup, then every `OQ_DECK_UPSTREAM_CHECK_HOURS`, and on the
//! operator's request no more than once a minute. Results are kept in
//! memory and served from there — reading the page never reaches out.
//!
//! What leaves the host: unauthenticated GETs to `api.github.com` for
//! the configured repository's latest release, its tag's commit, and a
//! comparison per running revision — a `User-Agent` naming this program
//! and its version, and nothing that identifies the operator or the
//! host. Zero hours turns all of it off.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use oq_deck_core::lang::Said;
use oq_deck_core::ops;
use oq_deck_core::upstream::{self, Cache, Fetch, Reply, Report, Running, What};

use crate::settings::Settings;

/// The framework commit this binary was built against (see `build.rs`).
pub const DECK_FRAMEWORK_REV: &str = env!("OQ_DECK_FRAMEWORK_REV");
const DECK_FRAMEWORK_REV_WHY: &str = env!("OQ_DECK_FRAMEWORK_REV_WHY");

/// How long after startup the first check runs: long enough not to
/// compete with the deck coming up, short enough to have an answer by
/// the time anyone looks.
const FIRST_CHECK: Duration = Duration::from_secs(20);
/// How soon a failed check is tried again, when that is sooner than the
/// configured interval: a blip at startup should not leave the page on
/// an error for six hours.
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(30 * 60);
/// How often the scheduler looks for a trader restart between checks.
///
/// A trader changes revision only by restarting, and a restart opens a
/// new journal. Looking for one is a directory listing, so it can run
/// far more often than the GitHub check without costing anything there.
const WATCH_EVERY: Duration = Duration::from_secs(5 * 60);
/// The least time between two checks the operator asks for.
pub const MANUAL_EVERY: Duration = Duration::from_secs(60);

/// GETs against `api.github.com` through `ureq`.
pub struct GitHub {
    proxy: Option<String>,
}

impl GitHub {
    #[must_use]
    pub fn new(proxy: Option<String>) -> Self {
        Self { proxy }
    }
}

impl Fetch for GitHub {
    fn get(&self, path: &str) -> Result<Reply, String> {
        // Built per check rather than kept: a check runs every few
        // hours, and an agent held between them is a connection pool
        // holding nothing useful.
        let mut cfg = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .http_status_as_error(false)
            .https_only(true)
            // Only the proxy the settings name; not the environment's.
            .proxy(None);
        if let Some(p) = &self.proxy {
            cfg = cfg.proxy(Some(ureq::Proxy::new(p).map_err(|e| e.to_string())?));
        }
        let agent: ureq::Agent = cfg.build().into();
        let mut resp = agent
            .get(format!("https://api.github.com{path}"))
            .header("User-Agent", concat!("oq-deck/", env!("CARGO_PKG_VERSION")))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(|e| e.to_string())?;
        let header = |name: &str| {
            resp.headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        let rate_limit_remaining = header("x-ratelimit-remaining");
        let rate_limit_reset = header("x-ratelimit-reset");
        let status = resp.status().as_u16();
        let body = resp
            .body_mut()
            .with_config()
            .limit(4 << 20)
            .read_to_string()
            .map_err(|e| e.to_string())?;
        Ok(Reply {
            status,
            rate_limit_remaining,
            rate_limit_reset,
            body,
        })
    }
}

/// Why the operator's request for a check was not run.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// The check is turned off.
    Disabled,
    /// One ran less than a minute ago; try again after this long.
    TooSoon(Duration),
}

/// The check's settings, its results, and what keeps it to one at a time.
pub struct Upstream {
    repo: String,
    every_hours: u64,
    agent_socket: Option<PathBuf>,
    journals: Option<PathBuf>,
    /// The journal the trader was writing when the last check began.
    run_at_check: Mutex<Option<String>>,
    fetch: Arc<dyn Fetch + Send + Sync>,
    cache: Mutex<Cache>,
    /// Held for the length of a check. A request that finds it held
    /// waits for that check rather than starting a second one.
    running: tokio::sync::Mutex<()>,
    last_manual: Mutex<Option<Instant>>,
}

impl Upstream {
    #[must_use]
    pub fn new(settings: &Settings, fetch: Arc<dyn Fetch + Send + Sync>) -> Arc<Self> {
        Arc::new(Self {
            repo: settings.upstream_repo.clone(),
            every_hours: settings.upstream_every_hours,
            agent_socket: settings.agent_socket.clone(),
            journals: settings.journals_dir.clone(),
            run_at_check: Mutex::new(None),
            fetch,
            cache: Mutex::new(Cache::default()),
            running: tokio::sync::Mutex::new(()),
            last_manual: Mutex::new(None),
        })
    }

    /// The real thing: GitHub, through the configured proxy if any.
    #[must_use]
    pub fn from_settings(settings: &Settings) -> Arc<Self> {
        Self::new(
            settings,
            Arc::new(GitHub::new(settings.upstream_proxy.clone())),
        )
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.every_hours > 0
    }

    /// What is known now, from memory.
    #[must_use]
    pub fn report(&self) -> Report {
        if !self.enabled() {
            return upstream::disabled(&self.repo);
        }
        let checking = self.running.try_lock().is_err();
        self.cache.lock().map_or_else(
            |_| upstream::disabled(&self.repo),
            |c| c.report(&self.repo, self.every_hours, checking),
        )
    }

    /// Run a check because the operator asked, at most once a minute.
    ///
    /// # Errors
    /// The check is off, or one was asked for too recently.
    pub async fn refresh(&self) -> Result<Report, Refused> {
        if !self.enabled() {
            return Err(Refused::Disabled);
        }
        {
            let Ok(mut last) = self.last_manual.lock() else {
                return Err(Refused::TooSoon(MANUAL_EVERY));
            };
            let now = Instant::now();
            if let Some(at) = *last {
                let since = now.saturating_duration_since(at);
                if since < MANUAL_EVERY {
                    return Err(Refused::TooSoon(MANUAL_EVERY - since));
                }
            }
            *last = Some(now);
        }
        self.check().await;
        Ok(self.report())
    }

    /// Run one check now and keep its result. Returns whether it
    /// succeeded, for the schedule.
    pub async fn check(&self) -> bool {
        if !self.enabled() {
            return false;
        }
        let _one = self.running.lock().await;
        let run = self.current_run().await;
        if let Ok(mut seen) = self.run_at_check.lock() {
            *seen = run;
        }
        let running = vec![deck_running(), self.trader_running().await];
        let fetch = Arc::clone(&self.fetch);
        let repo = self.repo.clone();
        let outcome =
            tokio::task::spawn_blocking(move || upstream::check(fetch.as_ref(), &repo, &running))
                .await
                .unwrap_or_else(|e| Err(Said::same(e.to_string())));
        let ok = outcome.is_ok();
        if let Err(why) = &outcome {
            tracing::warn!("upstream check failed: {}", why.en);
        }
        if let Ok(mut cache) = self.cache.lock() {
            cache.record(now_ms(), outcome);
        }
        ok
    }

    /// The trader's revision, from the host agent's release listing.
    async fn trader_running(&self) -> Running {
        let unknown = |why: Said| Running {
            what: What::Trader,
            release: None,
            rev: Err(why),
        };
        let Some(sock) = &self.agent_socket else {
            return unknown(Said::new(
                "尚未配置主机代理（OQ_DECK_AGENT_SOCKET），读不到交易进程用的版本",
                "No host agent is configured (OQ_DECK_AGENT_SOCKET), so the trader's revision \
                 cannot be read",
            ));
        };
        let nonce = match oq_deck_core::auth::new_token() {
            Ok(n) => n,
            Err(e) => return unknown(Said::same(e.to_string())),
        };
        let req = ops::AgentRequest {
            op: ops::Op::Releases,
            actor: "deck:upstream-check".to_owned(),
            nonce,
            expires_ms: now_ms() + 30_000,
            reason: None,
            step_up: None,
        };
        match crate::app::agent_call(sock, &req).await {
            Ok(resp) if resp.ok => {
                let (release, rev) = upstream::trader_rev(&resp.data);
                Running {
                    what: What::Trader,
                    release,
                    rev,
                }
            }
            Ok(resp) => unknown(Said::new(
                format!(
                    "主机代理拒绝了发布列表请求：{}",
                    resp.why(oq_deck_core::lang::Lang::Zh).unwrap_or("")
                ),
                format!(
                    "The host agent refused the release listing: {}",
                    resp.why(oq_deck_core::lang::Lang::En).unwrap_or("")
                ),
            )),
            Err(e) => unknown(Said::new(
                format!("主机代理无应答：{e}"),
                format!("The host agent did not answer: {e}"),
            )),
        }
    }

    /// Check shortly after startup, then on the schedule, forever.
    /// The journal the trader is writing now: the most recently modified
    /// `.oqj` in the journals directory, by file name.
    ///
    /// `None` when no directory is configured or nothing is in it; a
    /// restart cannot be told apart then, and the schedule alone applies.
    pub async fn current_run(&self) -> Option<String> {
        let dir = self.journals.clone()?;
        tokio::task::spawn_blocking(move || newest_journal(&dir))
            .await
            .ok()
            .flatten()
    }

    /// Whether the trader has restarted since the last check began.
    ///
    /// Restarting is the only way its revision changes, so this is what
    /// makes a deploy show up within minutes rather than at the next
    /// scheduled check — until it did, the card went on comparing the
    /// release the trader had been running for up to six hours. Before
    /// the first check there is nothing to compare against: `false`.
    pub async fn trader_restarted(&self) -> bool {
        let seen = self.run_at_check.lock().ok().and_then(|s| s.clone());
        let Some(seen) = seen else {
            return false;
        };
        self.current_run().await.is_some_and(|now| now != seen)
    }

    pub fn spawn(self: &Arc<Self>) {
        if !self.enabled() {
            return;
        }
        let me = Arc::clone(self);
        tokio::spawn(async move {
            let every = Duration::from_secs(me.every_hours * 3600);
            let mut due = Instant::now() + FIRST_CHECK;
            loop {
                if Instant::now() >= due || me.trader_restarted().await {
                    let ok = me.check().await;
                    due = Instant::now()
                        + if ok {
                            every
                        } else {
                            every.min(RETRY_AFTER_FAILURE)
                        };
                }
                let until_due = due.saturating_duration_since(Instant::now());
                tokio::time::sleep(until_due.min(WATCH_EVERY)).await;
            }
        });
    }
}

/// This deck's own revision, as built.
/// The most recently modified `.oqj` file in `dir`, by name.
///
/// Modification time rather than name order: the running journal is the
/// one being appended to, and file names are only sortable within one
/// strategy's prefix.
#[must_use]
pub fn newest_journal(dir: &std::path::Path) -> Option<String> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "oqj"))
        .filter_map(|e| {
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((modified, e.file_name().to_string_lossy().into_owned()))
        })
        .max()
        .map(|(_, name)| name)
}

fn deck_running() -> Running {
    Running {
        what: What::Deck,
        release: None,
        rev: if DECK_FRAMEWORK_REV.is_empty() {
            Err(Said::new(
                format!("构建时没有记录框架版本：{DECK_FRAMEWORK_REV_WHY}"),
                format!("The build recorded no framework revision: {DECK_FRAMEWORK_REV_WHY}"),
            ))
        } else {
            Ok(DECK_FRAMEWORK_REV.to_owned())
        },
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::DECK_FRAMEWORK_REV;

    /// The revision embedded is the one this workspace's lock names.
    #[test]
    fn the_build_embeds_the_locked_framework_commit() {
        let lock = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"),
        )
        .expect("the workspace lock");
        assert_eq!(
            crate::lockfile::framework_rev(&lock).as_deref(),
            Ok(DECK_FRAMEWORK_REV)
        );
        assert_eq!(DECK_FRAMEWORK_REV.len(), 40);
    }
}
