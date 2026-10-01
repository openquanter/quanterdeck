//! Whether what runs here is behind the framework's newest release.
//!
//! The framework publishes GitHub releases. The deck and the trader each
//! run some commit of it, and the question an operator wants answered is
//! "is that commit behind the release": the answer is GitHub's own
//! comparison of the two commits, not a version string matched by eye.
//!
//! Everything here is the half that has opinions — what a reply means,
//! which status words mean "includes the release", and what is kept when
//! a check fails. The network is behind [`Fetch`], so the tests feed it
//! canned replies and never leave the machine.
//!
//! The rule this module is built around is the console's own: a check
//! that could not be made is reported as *not made*, never as "up to
//! date". A rate limit, a timeout, a body that does not parse — each is
//! an error with its reason and its time, and the last answer that did
//! succeed is kept beside it with its own age, not passed off as fresh.

use serde::Serialize;
use serde_json::Value;

use crate::lang::Said;

/// One HTTP reply, reduced to what the check reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reply {
    pub status: u16,
    /// `X-RateLimit-Remaining`, as sent.
    pub rate_limit_remaining: Option<String>,
    /// `X-RateLimit-Reset`, Unix seconds, as sent.
    pub rate_limit_reset: Option<String>,
    pub body: String,
}

/// A GET against the GitHub API.
///
/// `path` starts with `/repos/`. An `Err` is a transport failure — no
/// reply at all; any reply, whatever its status, is an `Ok`.
pub trait Fetch {
    /// # Errors
    /// No reply arrived: the connection, the proxy or a timeout failed.
    fn get(&self, path: &str) -> Result<Reply, String>;
}

/// Which running program a revision belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum What {
    /// This console, as built: the framework commit its Cargo.lock pins.
    Deck,
    /// The trader, as the current release's manifest names it.
    Trader,
}

/// A revision of the framework something here runs, or why it is not
/// known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Running {
    pub what: What,
    /// The release the revision was read from, for the trader.
    pub release: Option<String>,
    pub rev: Result<String, Said>,
}

/// GitHub's word for how a revision stands against the release.
///
/// `compare/{release}...{rev}` reads with the release as the base, so
/// `ahead` means the revision has everything the release has and more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Identical,
    Ahead,
    Behind,
    Diverged,
}

/// What a status means for the operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The revision contains the release (`identical` or `ahead`).
    Includes,
    /// The release has commits the revision lacks, and nothing else.
    Behind,
    /// Each has commits the other lacks.
    Diverged,
    /// Not compared, and the reason says why. Never drawn as "includes".
    Unknown,
}

impl Status {
    /// Parse GitHub's word. Anything else is not a status this console
    /// knows how to read, and saying so beats guessing.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "identical" => Some(Self::Identical),
            "ahead" => Some(Self::Ahead),
            "behind" => Some(Self::Behind),
            "diverged" => Some(Self::Diverged),
            _ => None,
        }
    }

    #[must_use]
    pub const fn verdict(self) -> Verdict {
        match self {
            Self::Identical | Self::Ahead => Verdict::Includes,
            Self::Behind => Verdict::Behind,
            Self::Diverged => Verdict::Diverged,
        }
    }
}

/// The newest published release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Release {
    pub tag: String,
    pub name: Option<String>,
    pub published_at: Option<String>,
    pub url: String,
    pub prerelease: bool,
    /// The commit the tag points at.
    pub sha: String,
}

/// One running revision against the release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compared {
    pub what: What,
    pub release: Option<String>,
    pub rev: Option<String>,
    pub status: Option<Status>,
    pub ahead_by: Option<u64>,
    pub behind_by: Option<u64>,
    pub verdict: Verdict,
    /// Why the verdict is `Unknown`; `None` otherwise.
    pub reason: Option<Said>,
}

/// What a successful check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// `None`: the repository has published no release yet. That is a
    /// state of the world, not a failure to look.
    pub latest: Option<Release>,
    pub revisions: Vec<Compared>,
}

/// Whether `s` looks like a commit id: 7 to 40 hex digits.
///
/// Checked before it goes into a URL, so a manifest's odd field can at
/// worst produce "not a commit id", never a request for another path.
#[must_use]
pub fn is_commit_id(s: &str) -> bool {
    (7..=40).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Whether `repo` is `owner/name` in the characters GitHub allows.
#[must_use]
pub fn is_repo(repo: &str) -> bool {
    let mut parts = repo.split('/');
    let ok = |p: Option<&str>| {
        p.is_some_and(|p| {
            !p.is_empty()
                && p.len() <= 100
                && !p.starts_with('.')
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        })
    };
    ok(parts.next()) && ok(parts.next()) && parts.next().is_none()
}

/// Percent-encode a path segment: a tag is the publisher's text, and a
/// `/` or `?` in it must not become part of the request's shape.
fn segment(s: &str) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

/// A reply that says the API will not answer this address for a while.
///
/// GitHub answers an exhausted unauthenticated quota with 403 (or 429)
/// and `X-RateLimit-Remaining: 0`; a 429 alone is the secondary limit.
fn rate_limited(reply: &Reply) -> Option<Said> {
    let exhausted = reply.rate_limit_remaining.as_deref().map(str::trim) == Some("0");
    if !(reply.status == 429 || (reply.status == 403 && exhausted)) {
        return None;
    }
    let reset = reply
        .rate_limit_reset
        .as_deref()
        .and_then(|r| r.trim().parse::<i64>().ok());
    Some(match reset {
        Some(at) => Said::new(
            format!(
                "GitHub API 限流（未认证每小时 60 次），额度在 Unix 时间 {at} 恢复；这次没有检查成"
            ),
            format!(
                "GitHub API rate limit reached (60 an hour unauthenticated); it resets at Unix \
                 time {at}. This check was not made"
            ),
        ),
        None => Said::new(
            "GitHub API 限流；这次没有检查成",
            "GitHub API rate limit reached; this check was not made",
        ),
    })
}

/// GitHub's own `message`, when a body carries one.
fn message_of(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["message"].as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn http_error(what: &str, reply: &Reply) -> Said {
    let msg = message_of(&reply.body);
    let status = reply.status;
    Said::new(
        format!("读取 {what} 时 GitHub 返回 HTTP {status}：{msg}"),
        format!("GitHub answered HTTP {status} reading the {what}: {msg}"),
    )
}

fn parse_error(what: &str, why: &str) -> Said {
    Said::new(
        format!("无法解析 GitHub 返回的 {what}：{why}"),
        format!("The {what} GitHub returned does not parse: {why}"),
    )
}

fn network_error(why: &str) -> Said {
    Said::new(
        format!("连不上 GitHub API：{why}"),
        format!("Cannot reach the GitHub API: {why}"),
    )
}

fn json(what: &str, body: &str) -> Result<Value, Said> {
    serde_json::from_str(body).map_err(|e| parse_error(what, &e.to_string()))
}

/// The fields of `releases/latest`, before the tag is resolved.
///
/// # Errors
/// The body is not a release.
pub fn parse_release(body: &str) -> Result<(String, Release), Said> {
    let v = json("release", body)?;
    let tag = v["tag_name"]
        .as_str()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| parse_error("release", "no tag_name"))?
        .to_owned();
    let url = v["html_url"]
        .as_str()
        .ok_or_else(|| parse_error("release", "no html_url"))?
        .to_owned();
    // Only a link to the release's own page is passed on: the interface
    // renders it as a link, and a field from elsewhere is not a place to
    // take the operator.
    if !url.starts_with("https://github.com/") {
        return Err(parse_error("release", "html_url is not on github.com"));
    }
    let text = |k: &str| v[k].as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    Ok((
        tag.clone(),
        Release {
            tag,
            name: text("name"),
            published_at: text("published_at"),
            url,
            prerelease: v["prerelease"].as_bool().unwrap_or(false),
            sha: String::new(),
        },
    ))
}

/// The `sha` of `commits/{ref}`.
///
/// # Errors
/// The body is not a commit.
pub fn parse_commit(body: &str) -> Result<String, Said> {
    let v = json("commit", body)?;
    v["sha"]
        .as_str()
        .filter(|s| is_commit_id(s))
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| parse_error("commit", "no sha"))
}

/// `status`, `ahead_by` and `behind_by` of `compare/{base}...{head}`.
///
/// # Errors
/// The body is not a comparison, or names a status this does not know.
pub fn parse_compare(body: &str) -> Result<(Status, u64, u64), Said> {
    let v = json("comparison", body)?;
    let word = v["status"]
        .as_str()
        .ok_or_else(|| parse_error("comparison", "no status"))?;
    let status = Status::parse(word)
        .ok_or_else(|| parse_error("comparison", &format!("unknown status {word:?}")))?;
    let count = |k: &str| {
        v[k].as_u64()
            .ok_or_else(|| parse_error("comparison", &format!("no {k}")))
    };
    Ok((status, count("ahead_by")?, count("behind_by")?))
}

/// One GET, with the failures every call shares turned into sentences.
fn fetch(f: &dyn Fetch, path: &str) -> Result<Reply, Said> {
    let reply = f.get(path).map_err(|e| network_error(&e))?;
    if let Some(limit) = rate_limited(&reply) {
        return Err(limit);
    }
    Ok(reply)
}

/// Ask GitHub how each running revision stands against the newest
/// release of `repo`.
///
/// Every failure that leaves the answer unknown — no connection, a rate
/// limit, a reply that does not parse — fails the whole check, so it is
/// never partly reported as though it were whole. One exception: a
/// revision GitHub does not know (404 on the comparison: a commit only
/// in a private fork, say) is that revision's own "cannot tell", and the
/// others are still compared.
///
/// # Errors
/// The check could not be made; the sentence says why.
pub fn check(f: &dyn Fetch, repo: &str, running: &[Running]) -> Result<Checked, Said> {
    let base = format!("/repos/{repo}");
    let reply = fetch(f, &format!("{base}/releases/latest"))?;
    if reply.status == 404 {
        // No release yet: there is nothing to be behind. Each revision
        // is listed as not compared, with that as the reason.
        let revisions = running
            .iter()
            .map(|r| {
                unknown(
                    r,
                    Said::new(
                        "上游还没有发布任何版本",
                        "Upstream has published no release yet",
                    ),
                )
            })
            .collect();
        return Ok(Checked {
            latest: None,
            revisions,
        });
    }
    if reply.status != 200 {
        return Err(http_error("release", &reply));
    }
    let (tag, mut release) = parse_release(&reply.body)?;

    let reply = fetch(f, &format!("{base}/commits/{}", segment(&tag)))?;
    if reply.status != 200 {
        return Err(http_error("commit", &reply));
    }
    release.sha = parse_commit(&reply.body)?;

    let mut revisions = Vec::with_capacity(running.len());
    for r in running {
        let rev = match &r.rev {
            Ok(rev) if is_commit_id(rev) => rev.to_ascii_lowercase(),
            Ok(rev) => {
                revisions.push(unknown(
                    r,
                    Said::new(
                        format!("{rev:?} 不是一个提交编号"),
                        format!("{rev:?} is not a commit id"),
                    ),
                ));
                continue;
            }
            Err(why) => {
                revisions.push(unknown(r, why.clone()));
                continue;
            }
        };
        // Compared from the tag's commit rather than its name: the same
        // answer, and nothing the publisher typed goes into the path.
        let reply = fetch(
            f,
            &format!("{base}/compare/{}...{rev}?per_page=1", release.sha),
        )?;
        match reply.status {
            200 => {
                let (status, ahead_by, behind_by) = parse_compare(&reply.body)?;
                revisions.push(Compared {
                    what: r.what,
                    release: r.release.clone(),
                    rev: Some(rev),
                    status: Some(status),
                    ahead_by: Some(ahead_by),
                    behind_by: Some(behind_by),
                    verdict: status.verdict(),
                    reason: None,
                });
            }
            404 => {
                let mut c = unknown(
                    r,
                    Said::new(
                        format!("{repo} 里找不到提交 {rev}，无法比较"),
                        format!("{repo} has no commit {rev}, so it cannot be compared"),
                    ),
                );
                c.rev = Some(rev);
                revisions.push(c);
            }
            _ => return Err(http_error("comparison", &reply)),
        }
    }
    Ok(Checked {
        latest: Some(release),
        revisions,
    })
}

fn unknown(r: &Running, reason: Said) -> Compared {
    Compared {
        what: r.what,
        release: r.release.clone(),
        rev: r.rev.as_ref().ok().cloned(),
        status: None,
        ahead_by: None,
        behind_by: None,
        verdict: Verdict::Unknown,
        reason: Some(reason),
    }
}

/// The trader's framework revision, from the agent's release listing.
///
/// The listing names the `current` release and carries the verified
/// manifest of every staged one; the release script writes the framework
/// commit it built against as `framework`. A current release that is no
/// longer staged, did not verify, or predates that field is a revision
/// this cannot read — said as such, not guessed.
///
/// Returns the current release's id, when there is one, beside the
/// revision or the sentence saying why it cannot be read.
pub fn trader_rev(releases: &Value) -> (Option<String>, Result<String, Said>) {
    let Some(current) = releases["current"].as_str() else {
        return (
            None,
            Err(Said::new(
                "主机代理报告没有当前发布",
                "The host agent reports no current release",
            )),
        );
    };
    let id = Some(current.to_owned());
    let staged = releases["staged"]
        .as_array()
        .and_then(|s| s.iter().find(|r| r["id"].as_str() == Some(current)));
    let Some(staged) = staged else {
        return (
            id,
            Err(Said::new(
                format!("当前发布 {current} 的 manifest 已不在暂存目录，读不到它用的框架版本"),
                format!(
                    "The manifest of the current release {current} is no longer staged, so the \
                     framework revision it was built with cannot be read"
                ),
            )),
        );
    };
    if staged["verified"].as_bool() != Some(true) {
        let problem = staged["problem_en"]
            .as_str()
            .or_else(|| staged["problem"].as_str())
            .unwrap_or("");
        return (
            id,
            Err(Said::new(
                format!("当前发布 {current} 的 manifest 验证不通过：{problem}"),
                format!("The manifest of the current release {current} does not verify: {problem}"),
            )),
        );
    }
    match staged["manifest"]["framework"].as_str() {
        Some(rev) => (id, Ok(rev.to_owned())),
        None => (
            id,
            Err(Said::new(
                format!("当前发布 {current} 的 manifest 没有 framework 字段"),
                format!("The manifest of the current release {current} has no framework field"),
            )),
        ),
    }
}

// -- what is kept, and how it is reported -----------------------------------

/// The results of every check so far that matter: the last attempt and
/// the last success. In memory; a restart starts again.
#[derive(Debug, Default, Clone)]
pub struct Cache {
    attempted_at_ms: Option<i64>,
    error: Option<Said>,
    success: Option<(i64, Checked)>,
}

impl Cache {
    /// Record an attempt.
    ///
    /// A failure keeps the previous success: it is still the best thing
    /// known, and the report carries its age so it is not read as new.
    pub fn record(&mut self, at_ms: i64, outcome: Result<Checked, Said>) {
        self.attempted_at_ms = Some(at_ms);
        match outcome {
            Ok(checked) => {
                self.error = None;
                self.success = Some((at_ms, checked));
            }
            Err(why) => self.error = Some(why),
        }
    }

    /// What `GET /api/v1/upstream` answers.
    #[must_use]
    pub fn report(&self, repo: &str, every_hours: u64, checking: bool) -> Report {
        let (succeeded_at_ms, checked) = match &self.success {
            Some((at, c)) => (Some(*at), Some(c)),
            None => (None, None),
        };
        let revisions: Vec<RevisionView> = checked
            .map(|c| c.revisions.iter().map(RevisionView::from).collect())
            .unwrap_or_default();
        Report {
            enabled: every_hours > 0,
            reason: None,
            reason_en: None,
            repo: repo.to_owned(),
            every_hours,
            checking,
            checked_at_ms: self.attempted_at_ms,
            error: self.error.as_ref().map(|e| e.zh.clone()),
            error_en: self.error.as_ref().map(|e| e.en.clone()),
            succeeded_at_ms,
            // Before any success this is unknown, not "no release".
            published: checked.map(|c| c.latest.is_some()),
            latest: checked.and_then(|c| c.latest.clone()),
            behind: revisions.iter().any(|r| r.verdict == Verdict::Behind),
            revisions,
        }
    }
}

/// The report when the check is turned off: nothing was or will be
/// fetched, and the reason says how to turn it on.
#[must_use]
pub fn disabled(repo: &str) -> Report {
    let why = Said::new(
        "已关闭：OQ_DECK_UPSTREAM_CHECK_HOURS=0，deck 不会访问 GitHub",
        "Off: OQ_DECK_UPSTREAM_CHECK_HOURS=0, so the deck makes no request to GitHub",
    );
    Report {
        enabled: false,
        reason: Some(why.zh),
        reason_en: Some(why.en),
        repo: repo.to_owned(),
        every_hours: 0,
        checking: false,
        checked_at_ms: None,
        error: None,
        error_en: None,
        succeeded_at_ms: None,
        published: None,
        latest: None,
        behind: false,
        revisions: Vec::new(),
    }
}

/// The upstream check, as the interface reads it.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub enabled: bool,
    /// Why the check is off, when it is.
    pub reason: Option<String>,
    pub reason_en: Option<String>,
    pub repo: String,
    pub every_hours: u64,
    /// A check is running now.
    pub checking: bool,
    /// The last attempt, successful or not. `None`: not checked yet.
    pub checked_at_ms: Option<i64>,
    /// Why the last attempt failed. `None` when it succeeded.
    pub error: Option<String>,
    pub error_en: Option<String>,
    /// When the result below was found. Older than `checked_at_ms`
    /// exactly when the last attempt failed.
    pub succeeded_at_ms: Option<i64>,
    /// Whether the repository has a release. `None` until a check has
    /// succeeded: not knowing is not "no release".
    pub published: Option<bool>,
    pub latest: Option<Release>,
    pub revisions: Vec<RevisionView>,
    /// Some revision is behind the release, per the last success.
    pub behind: bool,
}

/// One revision on the wire.
#[derive(Debug, Clone, Serialize)]
pub struct RevisionView {
    pub what: What,
    pub release: Option<String>,
    pub rev: Option<String>,
    pub status: Option<Status>,
    pub ahead_by: Option<u64>,
    pub behind_by: Option<u64>,
    pub verdict: Verdict,
    pub reason: Option<String>,
    pub reason_en: Option<String>,
}

impl From<&Compared> for RevisionView {
    fn from(c: &Compared) -> Self {
        Self {
            what: c.what,
            release: c.release.clone(),
            rev: c.rev.clone(),
            status: c.status,
            ahead_by: c.ahead_by,
            behind_by: c.behind_by,
            verdict: c.verdict,
            reason: c.reason.as_ref().map(|r| r.zh.clone()),
            reason_en: c.reason.as_ref().map(|r| r.en.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    const RELEASE: &str = r#"{
        "tag_name": "v2.0.1", "name": "2.0.1", "prerelease": false,
        "published_at": "2026-10-01T08:00:00Z",
        "html_url": "https://github.com/openquanter/openquanter/releases/tag/v2.0.1"
    }"#;
    const TAG_SHA: &str = "1111111111111111111111111111111111111111";
    const DECK: &str = "bbdcb4e10d5b5b8a32f3ba51e4f353d6a9850a7b";
    const TRADER: &str = "cccccccccccccccccccccccccccccccccccccccc";

    /// Canned replies by path; a path with none is a transport failure.
    #[derive(Default)]
    struct Canned {
        replies: HashMap<String, Reply>,
        asked: RefCell<Vec<String>>,
    }

    impl Canned {
        fn with(mut self, path: &str, status: u16, body: &str) -> Self {
            self.replies.insert(
                path.to_owned(),
                Reply {
                    status,
                    body: body.to_owned(),
                    ..Reply::default()
                },
            );
            self
        }
    }

    impl Fetch for Canned {
        fn get(&self, path: &str) -> Result<Reply, String> {
            self.asked.borrow_mut().push(path.to_owned());
            self.replies
                .get(path)
                .cloned()
                .ok_or_else(|| "connection refused".to_owned())
        }
    }

    const REPO: &str = "openquanter/openquanter";

    fn compare(base: &str, head: &str) -> String {
        format!("/repos/{REPO}/compare/{base}...{head}?per_page=1")
    }

    fn compared(status: &str, ahead: u64, behind: u64) -> String {
        format!(r#"{{"status":"{status}","ahead_by":{ahead},"behind_by":{behind},"commits":[]}}"#)
    }

    fn published() -> Canned {
        Canned::default()
            .with(&format!("/repos/{REPO}/releases/latest"), 200, RELEASE)
            .with(
                &format!("/repos/{REPO}/commits/v2.0.1"),
                200,
                &format!(r#"{{"sha":"{TAG_SHA}","commit":{{}}}}"#),
            )
    }

    fn both() -> Vec<Running> {
        vec![
            Running {
                what: What::Deck,
                release: None,
                rev: Ok(DECK.to_owned()),
            },
            Running {
                what: What::Trader,
                release: Some("r42".to_owned()),
                rev: Ok(TRADER.to_owned()),
            },
        ]
    }

    #[test]
    fn status_words_map_to_what_they_mean_for_the_operator() {
        assert_eq!(
            Status::parse("identical").map(Status::verdict),
            Some(Verdict::Includes)
        );
        assert_eq!(
            Status::parse("ahead").map(Status::verdict),
            Some(Verdict::Includes)
        );
        assert_eq!(
            Status::parse("behind").map(Status::verdict),
            Some(Verdict::Behind)
        );
        assert_eq!(
            Status::parse("diverged").map(Status::verdict),
            Some(Verdict::Diverged)
        );
        // A word this console does not know is not quietly "fine".
        assert_eq!(Status::parse("unrelated"), None);
        assert!(parse_compare(&compared("unrelated", 0, 0)).is_err());
    }

    #[test]
    fn a_release_its_commit_and_two_comparisons() {
        let f = published()
            .with(&compare(TAG_SHA, DECK), 200, &compared("behind", 0, 7))
            .with(&compare(TAG_SHA, TRADER), 200, &compared("ahead", 3, 0));
        let c = check(&f, REPO, &both()).expect("checked");
        let latest = c.latest.expect("a release");
        assert_eq!(latest.tag, "v2.0.1");
        assert_eq!(latest.sha, TAG_SHA);
        assert_eq!(latest.name.as_deref(), Some("2.0.1"));
        assert!(latest.url.ends_with("/releases/tag/v2.0.1"));
        assert_eq!(c.revisions[0].verdict, Verdict::Behind);
        assert_eq!(c.revisions[0].behind_by, Some(7));
        assert_eq!(c.revisions[1].verdict, Verdict::Includes);
        assert_eq!(c.revisions[1].ahead_by, Some(3));
        assert_eq!(c.revisions[1].release.as_deref(), Some("r42"));
    }

    #[test]
    fn no_release_yet_is_a_state_not_an_error() {
        let f = Canned::default().with(
            &format!("/repos/{REPO}/releases/latest"),
            404,
            r#"{"message":"Not Found"}"#,
        );
        let c = check(&f, REPO, &both()).expect("a 404 here is an answer");
        assert!(c.latest.is_none());
        assert!(c.revisions.iter().all(|r| r.verdict == Verdict::Unknown));
        // Nothing to compare against, so nothing else was asked.
        assert_eq!(f.asked.borrow().len(), 1);
        let mut cache = Cache::default();
        cache.record(5, Ok(c));
        let report = cache.report(REPO, 6, false);
        assert_eq!(report.published, Some(false));
        assert!(!report.behind);
    }

    #[test]
    fn an_exhausted_rate_limit_is_an_error_not_an_answer() {
        let mut f = Canned::default();
        f.replies.insert(
            format!("/repos/{REPO}/releases/latest"),
            Reply {
                status: 403,
                rate_limit_remaining: Some("0".into()),
                rate_limit_reset: Some("1790000000".into()),
                body: r#"{"message":"API rate limit exceeded"}"#.into(),
            },
        );
        let e = check(&f, REPO, &both()).expect_err("not checked");
        assert!(e.en.contains("rate limit"), "{}", e.en);
        assert!(e.en.contains("1790000000"), "{}", e.en);
        assert!(e.zh.contains("限流"), "{}", e.zh);
    }

    #[test]
    fn a_rate_limit_midway_fails_the_whole_check() {
        // The release reads, then the comparisons are refused: reporting
        // the release with revisions "unknown" would look like an answer.
        let mut f = published();
        f.replies.insert(
            compare(TAG_SHA, DECK),
            Reply {
                status: 429,
                body: "{}".into(),
                ..Reply::default()
            },
        );
        assert!(check(&f, REPO, &both()).is_err());
    }

    #[test]
    fn a_body_that_does_not_parse_is_an_error() {
        let f = Canned::default().with(&format!("/repos/{REPO}/releases/latest"), 200, "<html>");
        let e = check(&f, REPO, &both()).expect_err("unparsed");
        assert!(e.en.contains("does not parse"), "{}", e.en);
        // A link off github.com is refused, not passed to the interface.
        let off = RELEASE.replace("https://github.com/", "https://example.com/");
        assert!(parse_release(&off).is_err());
    }

    #[test]
    fn no_connection_is_an_error() {
        let e = check(&Canned::default(), REPO, &both()).expect_err("no reply");
        assert!(e.en.contains("Cannot reach"), "{}", e.en);
    }

    #[test]
    fn a_revision_github_does_not_know_is_that_revisions_cannot_tell() {
        let f = published()
            .with(&compare(TAG_SHA, DECK), 200, &compared("identical", 0, 0))
            .with(&compare(TAG_SHA, TRADER), 404, r#"{"message":"Not Found"}"#);
        let c = check(&f, REPO, &both()).expect("checked");
        assert_eq!(c.revisions[0].verdict, Verdict::Includes);
        assert_eq!(c.revisions[1].verdict, Verdict::Unknown);
        assert_eq!(c.revisions[1].rev.as_deref(), Some(TRADER));
    }

    #[test]
    fn an_unknown_revision_is_not_asked_about() {
        let running = vec![
            Running {
                what: What::Deck,
                release: None,
                rev: Ok("main; rm -rf".into()),
            },
            Running {
                what: What::Trader,
                release: None,
                rev: Err(Said::same("no agent")),
            },
        ];
        let f = published();
        let c = check(&f, REPO, &running).expect("checked");
        assert!(c.revisions.iter().all(|r| r.verdict == Verdict::Unknown));
        // The release and its commit, and no comparison.
        assert_eq!(f.asked.borrow().len(), 2, "{:?}", f.asked.borrow());
    }

    #[test]
    fn a_tag_is_one_path_segment() {
        assert_eq!(segment("v2.0.1"), "v2.0.1");
        assert_eq!(segment("a/../b?x"), "a%2F..%2Fb%3Fx");
    }

    #[test]
    fn a_failure_keeps_the_last_success_with_its_age() {
        let f = published()
            .with(&compare(TAG_SHA, DECK), 200, &compared("behind", 0, 2))
            .with(&compare(TAG_SHA, TRADER), 200, &compared("diverged", 1, 1));
        let mut cache = Cache::default();
        cache.record(1_000, check(&f, REPO, &both()));
        cache.record(2_000, Err(Said::same("timed out")));
        let r = cache.report(REPO, 6, false);
        assert_eq!(r.checked_at_ms, Some(2_000));
        assert_eq!(r.succeeded_at_ms, Some(1_000));
        assert_eq!(r.error.as_deref(), Some("timed out"));
        assert_eq!(r.latest.as_ref().map(|l| l.tag.as_str()), Some("v2.0.1"));
        assert!(r.behind);
        assert_eq!(r.revisions[1].verdict, Verdict::Diverged);
    }

    #[test]
    fn before_any_success_nothing_is_claimed() {
        let mut cache = Cache::default();
        cache.record(1, Err(Said::same("timed out")));
        let r = cache.report(REPO, 6, false);
        assert_eq!(r.published, None, "unknown is not 'no release'");
        assert!(r.latest.is_none());
        assert!(r.revisions.is_empty());
        assert!(!r.behind);
        assert!(r.error.is_some());
    }

    #[test]
    fn the_trader_revision_comes_from_the_current_releases_manifest() {
        let listing = serde_json::json!({
            "current": "r42",
            "staged": [
                {"id": "r43", "verified": true, "manifest": {"framework": "dddddddd"}},
                {"id": "r42", "verified": true, "manifest": {"framework": TRADER}},
            ],
        });
        assert_eq!(
            trader_rev(&listing),
            (Some("r42".into()), Ok(TRADER.into()))
        );

        let gone = serde_json::json!({"current": "r41", "staged": []});
        assert!(
            trader_rev(&gone)
                .1
                .unwrap_err()
                .en
                .contains("no longer staged")
        );
        let old = serde_json::json!({
            "current": "r42",
            "staged": [{"id": "r42", "verified": true, "manifest": {"files": {}}}],
        });
        assert!(trader_rev(&old).1.unwrap_err().en.contains("no framework"));
        let bad = serde_json::json!({
            "current": "r42",
            "staged": [{"id": "r42", "verified": false, "problem": "x", "problem_en": "bad sig"}],
        });
        assert!(trader_rev(&bad).1.unwrap_err().en.contains("bad sig"));
        assert!(trader_rev(&serde_json::json!({"current": null})).1.is_err());
    }

    #[test]
    fn repos_and_commit_ids_are_checked_before_they_reach_a_url() {
        assert!(is_repo("openquanter/openquanter"));
        assert!(is_repo("a-b/c.d_e"));
        assert!(!is_repo("openquanter"));
        assert!(!is_repo("a/b/c"));
        assert!(!is_repo("a/../b"));
        assert!(!is_repo("a/b?x"));
        assert!(is_commit_id("bbdcb4e"));
        assert!(is_commit_id(DECK));
        assert!(!is_commit_id("bbdcb4"));
        assert!(!is_commit_id("main"));
    }
}
