//! Scheduled reports: the part with a clock, a socket and a directory.
//!
//! What a report says lives in `oq_deck_core::report`; this fetches its
//! inputs and keeps the files. Every `OQ_DECK_REPORT_HOURS` the period
//! that just ended is reported — and once shortly after startup, if that
//! period has no report yet, so a deck that was down at the boundary
//! catches up. The operator can ask for one covering the last period up
//! to now, no more than once a minute.
//!
//! Reports are stored as JSON and rendered when read, so the language
//! follows whoever is reading rather than whoever was on duty when the
//! file was written.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use oq_deck_core::capabilities::Capability;
use oq_deck_core::lang::{Lang, Said};
use oq_deck_core::live;
use oq_deck_core::ops;
use oq_deck_core::report::{self, Entry, Inputs, Trigger};

use crate::settings::Settings;

/// How long after startup the first run looks for a missing report.
const FIRST_RUN: Duration = Duration::from_secs(60);
/// The longest the schedule sleeps before looking again: a write that
/// failed is tried again this soon, not a whole period later.
const RECHECK: Duration = Duration::from_secs(30 * 60);
/// The least time between two reports the operator asks for.
pub const MANUAL_EVERY: Duration = Duration::from_secs(60);
/// Samples asked of the black box: its most, so a day is every sample.
const BLACKBOX_POINTS: usize = 2000;
/// Who the agent's audit trail says asked.
const ACTOR: &str = "deck:report";

/// Why a report the operator asked for was not made.
#[derive(Debug)]
pub enum Refused {
    /// Reports are off; the reason says how.
    Off(Said),
    /// One was made less than a minute ago; try again after this long.
    TooSoon(Duration),
    /// It could not be written.
    Failed(Said),
}

pub struct Reports {
    dir: Option<PathBuf>,
    every_hours: u64,
    agent_socket: Option<PathBuf>,
    journals: Option<PathBuf>,
    venue_record: Option<PathBuf>,
    /// Held for the length of a run, so two never write at once.
    running: tokio::sync::Mutex<()>,
    last_manual: Mutex<Option<Instant>>,
}

impl Reports {
    #[must_use]
    pub fn from_settings(settings: &Settings) -> Arc<Self> {
        Arc::new(Self {
            dir: settings.reports_dir.clone(),
            every_hours: settings.report_every_hours,
            agent_socket: settings.agent_socket.clone(),
            journals: settings.journals_dir.clone(),
            venue_record: settings.venue_record.clone(),
            running: tokio::sync::Mutex::new(()),
            last_manual: Mutex::new(None),
        })
    }

    #[must_use]
    pub const fn every_hours(&self) -> u64 {
        self.every_hours
    }

    /// The directory, or why there are no reports.
    ///
    /// # Errors
    /// Reports are turned off, or there is nowhere to keep them.
    pub fn dir(&self) -> Result<&Path, Said> {
        if self.every_hours == 0 {
            return Err(Said::new(
                "已关闭：OQ_DECK_REPORT_HOURS=0，deck 不生成报告",
                "Off: OQ_DECK_REPORT_HOURS=0, so the deck writes no reports",
            ));
        }
        self.dir.as_deref().ok_or_else(|| {
            Said::new(
                "没有存放报告的目录：请设置 OQ_DECK_REPORTS_DIR，或给 deck 一个状态目录\
                 （OQ_DECK_STATE_DIR 或 systemd 的 StateDirectory=）",
                "There is nowhere to keep reports: set OQ_DECK_REPORTS_DIR, or give the deck a \
                 state directory (OQ_DECK_STATE_DIR or systemd's StateDirectory=)",
            )
        })
    }

    /// Whether reports work here, and if not, why.
    #[must_use]
    pub fn capability(&self, lang: Lang) -> Capability {
        match self.dir() {
            Err(why) => Capability::off(why.in_lang(lang)),
            Ok(dir) => match std::fs::read_dir(dir) {
                Ok(_) => Capability::on(),
                Err(e) => Capability::off(lang.pick(
                    format!("报告目录 {} 无法读取：{e}", dir.display()),
                    format!(
                        "The reports directory {} cannot be read: {e}",
                        dir.display()
                    ),
                )),
            },
        }
    }

    /// Make the directory, owner-only, so the capability can be on.
    ///
    /// # Errors
    /// It could not be made.
    pub fn prepare(&self) -> Result<(), String> {
        let Ok(dir) = self.dir() else {
            return Ok(());
        };
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(|e| format!("{}: {e}", dir.display()))
    }

    /// Make a report covering the last period up to now, because the
    /// operator asked — at most once a minute.
    ///
    /// # Errors
    /// Reports are off, one was asked for too recently, or it could not
    /// be written.
    pub async fn generate_now(&self) -> Result<Entry, Refused> {
        self.dir().map_err(Refused::Off)?;
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
        let to = now_ms();
        let from = to - i64::try_from(self.every_hours).unwrap_or(24) * 3_600_000;
        self.generate(from, to, Trigger::Manual)
            .await
            .map_err(Refused::Failed)
    }

    /// Fetch, build and write the report for one period.
    ///
    /// # Errors
    /// There is no directory, or the file could not be written. Missing
    /// inputs are not errors: they are the report's `unavailable` list.
    pub async fn generate(
        &self,
        from_ms: i64,
        to_ms: i64,
        trigger: Trigger,
    ) -> Result<Entry, Said> {
        let dir = self.dir()?.to_path_buf();
        let _one = self.running.lock().await;
        let blackbox = self.blackbox(from_ms, to_ms).await;
        let reconciliation = self.reconciliation().await;
        let data = report::build(Inputs {
            from_ms,
            to_ms,
            generated_at_ms: now_ms(),
            trigger,
            blackbox,
            reconciliation,
        });
        let entry = Entry::of(&data);
        tokio::task::spawn_blocking(move || {
            let written = report::write(&dir, &data);
            let pruned = report::prune(&dir, now_ms(), report::KEEP_DAYS);
            if pruned > 0 {
                tracing::info!(
                    "removed {pruned} reports older than {} days",
                    report::KEEP_DAYS
                );
            }
            written
        })
        .await
        .map_err(|e| Said::same(e.to_string()))?
        .map_err(|e| {
            Said::new(
                format!("报告写不进去：{e}"),
                format!("The report could not be written: {e}"),
            )
        })?;
        Ok(entry)
    }

    async fn blackbox(&self, from_ms: i64, to_ms: i64) -> Result<serde_json::Value, Said> {
        let Some(sock) = &self.agent_socket else {
            return Err(Said::new(
                "尚未配置主机代理（OQ_DECK_AGENT_SOCKET），读不到黑匣子",
                "No host agent is configured (OQ_DECK_AGENT_SOCKET), so the black box cannot be \
                 read",
            ));
        };
        let nonce = oq_deck_core::auth::new_token().map_err(|e| Said::same(e.to_string()))?;
        let req = ops::AgentRequest {
            op: ops::Op::Blackbox {
                from_ms,
                to_ms,
                points: BLACKBOX_POINTS,
            },
            actor: ACTOR.to_owned(),
            nonce,
            expires_ms: now_ms() + 30_000,
            reason: None,
            step_up: None,
        };
        match crate::app::agent_call(sock, &req).await {
            Ok(resp) if resp.ok => Ok(resp.data),
            Ok(resp) => Err(Said::new(
                format!(
                    "主机代理拒绝了黑匣子请求：{}",
                    resp.why(Lang::Zh).unwrap_or("")
                ),
                format!(
                    "The host agent refused the black box request: {}",
                    resp.why(Lang::En).unwrap_or("")
                ),
            )),
            Err(e) => Err(Said::new(
                format!("主机代理无应答：{e}"),
                format!("The host agent did not answer: {e}"),
            )),
        }
    }

    async fn reconciliation(&self) -> Result<live::Reconciliation, Said> {
        let Some(dir) = self.journals.clone() else {
            return Err(Said::new(
                "尚未配置日志目录（OQ_DECK_JOURNALS_DIR），无法对账",
                "No journals directory is configured (OQ_DECK_JOURNALS_DIR), so nothing can be \
                 reconciled",
            ));
        };
        let Some(record) = self.venue_record.clone() else {
            return Err(Said::new(
                "尚未配置交易所最新记录（OQ_DECK_VENUE_RECORD），无法对账",
                "No latest venue record is configured (OQ_DECK_VENUE_RECORD), so nothing can be \
                 reconciled",
            ));
        };
        tokio::task::spawn_blocking(move || live::reconcile_newest(&dir, &record))
            .await
            .map_err(|e| Said::same(e.to_string()))?
            .map_err(|e| e.said())
    }

    /// One look: report the newest whole period if it has no report.
    ///
    /// A directory that cannot be read counts as "no report", so the
    /// schedule tries — and the write says what is wrong.
    pub async fn run_once(&self, now_ms: i64) {
        let Ok(dir) = self.dir() else {
            return;
        };
        let dir = dir.to_path_buf();
        let hours = self.every_hours;
        let due = tokio::task::spawn_blocking(move || {
            report::due(now_ms, hours, |from, to| report::exists(&dir, from, to))
        })
        .await
        .ok()
        .flatten();
        let Some((from, to)) = due else {
            return;
        };
        match self.generate(from, to, Trigger::Scheduled).await {
            Ok(entry) => tracing::info!(
                "wrote report {} ({} sections unavailable)",
                entry.id,
                entry.unavailable.len()
            ),
            Err(why) => tracing::warn!("report for {from}-{to} not written: {}", why.en),
        }
    }

    /// Look shortly after startup, then at each period's end, forever.
    pub fn spawn(self: &Arc<Self>) {
        if self.dir().is_err() {
            return;
        }
        let me = Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(FIRST_RUN).await;
            loop {
                me.run_once(now_ms()).await;
                let now = now_ms();
                let wait = u64::try_from(report::next_due(now, me.every_hours) - now).unwrap_or(0);
                tokio::time::sleep(
                    Duration::from_millis(wait).clamp(Duration::from_secs(1), RECHECK),
                )
                .await;
            }
        });
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}
