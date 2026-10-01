# Changelog

Each `## X.Y.Z` section is that release's notes: the release workflow
publishes it verbatim, and refuses a tag without one.

## Unreleased

## 1.0.0 — 2026-10-01

The first tagged release. Everything below is "since the repository
started" rather than since a previous release.

### Whether this console is behind quanterdeck's newest release

- **The same card checks the console itself.** Quanterdeck publishes
  GitHub releases too; with each framework check the deck also reads the
  newest release of `OQ_DECK_SELF_REPO` (`openquanter/quanterdeck` by
  default) and compares its tag with the version the deck was built as —
  by version, not commit, because a build from a `git archive` has no
  commit to read while the version is always compiled in. Equal is
  current, newer is ahead (built from a branch after the release), older
  is behind. A tag that is not `X.Y.Z` (with or without `v`, optionally
  with a pre-release) is "cannot tell", with the reason; no release yet
  is its own state. The overview card gains a "This console (quanterdeck
  release)" section, and the top-bar badge names which release is newer.
- **Kept apart from the framework check.** One more GET per check (five
  at most), on the same schedule, switch and proxy. Each half keeps its
  own error and last success, so GitHub refusing one leaves the other's
  answer standing. `GET /api/v1/upstream` gains a `console` object; the
  existing fields, `behind` included, still describe the framework.

### Whether what runs is behind the framework's newest release

- **The deck checks the framework's GitHub releases.** Shortly after
  startup and then every `OQ_DECK_UPSTREAM_CHECK_HOURS` (6 by default),
  it reads the newest release of `OQ_DECK_UPSTREAM_REPO`, resolves its
  tag to a commit, and asks GitHub to compare that commit with two
  running revisions: the deck's own, embedded at build time from
  `Cargo.lock`, and the trader's, from the `framework` field of the
  current release's manifest through the host agent. The overview has an
  "Upstream release" card and the top bar a badge when something is
  behind. A trader restart — a new journal in `OQ_DECK_JOURNALS_DIR`,
  looked for every 5 minutes without touching the network — brings the
  next check forward, so a deploy shows within minutes rather than at
  the next scheduled check.
- **"Cannot tell" stays "cannot tell".** A timeout, an exhausted rate
  limit or a reply that does not parse is an error with its reason and
  time, shown beside the last successful answer and that answer's age —
  never as "up to date". No release published yet is its own state, and
  a revision GitHub does not know (or no agent to ask) is that
  revision's "cannot tell", with the reason.
- **One outbound request, and a way to turn it off.** Unauthenticated
  GETs to `api.github.com` carrying a `User-Agent` and the repository
  path, nothing else; `OQ_DECK_UPSTREAM_CHECK_HOURS=0` makes none at all,
  and the capability says so. `OQ_DECK_UPSTREAM_PROXY` names a proxy for
  this check only. A manual check (`POST /api/v1/upstream/refresh`) is
  checked for `Origin` like any write but is not behind
  `OQ_DECK_ALLOW_WRITES` — it changes nothing on the host — and runs at
  most once a minute.

### A large directory no longer stalls the console

- **File work runs off the async workers.** Listing runs, sweeps and
  journals, a run's or a journal's detail, comparison, attribution and
  reconciliation all parse files — the journal listing replays every
  journal whole — and did it on the threads that answer every other
  request, the session checks included. They run on the blocking pool
  now.
- **A listing remembers what an unchanged file parsed to.** Keyed by
  inode, length, modification and change time; a file modified in the
  last two seconds, or one that would not read, is read again every
  time. The listing says exactly what it said before — an unreadable
  file is still listed with its reason and still withholds the total —
  and a removed file is forgotten with the listing that missed it.

### Signing in once, on the machines you choose

- **A browser you enrol is not asked again.** The login page offers to
  remember this device; that browser then carries a device credential
  instead of a password and a code. It is a second way in with one
  factor rather than two, which is why it is named, listed in Settings
  and revocable — and why withdrawing one is deliberately not behind
  `OQ_DECK_ALLOW_WRITES`. Only its SHA-256 is written, so a copy of the
  file is a list of what exists rather than a set of keys to use.
- **Sessions no longer expire at a fixed hour.** `OQ_DECK_SESSION_HOURS`
  and `OQ_DECK_SESSION_IDLE_MINUTES` are settings, bounded and refused
  rather than clamped, and `/runtime/settings` reports the ones in force
  instead of the two numbers it used to hardcode.
- **Signing out ends the session, not the device.** Taking a device away
  is a separate act with its own page; otherwise signing out on a shared
  machine would un-enrol it.
- Sessions still live in memory, so a restart still ends them — and an
  enrolled browser does not notice, which is what the device credential
  is for.
- **A machine can be enrolled by proving an SSH key**, for the case the
  box above does not cover: a laptop you have a key on and no password
  on. `scripts/deck-enrol.sh` asks the deck for a challenge, signs it,
  and prints a link; opening the link in the browser enrols it. The
  keys trusted are the ones an `allowed_signers` file lists
  (`OQ_DECK_TRUSTED_KEYS`), verified with the same `ssh-keygen -Y
  verify` the host agent trusts a release with — under its own
  namespace, so a release signature cannot be replayed here. Off unless
  that variable is set, and the console reports which it is.
- **`deck-enrol.sh` takes `--cacert`** (or `OQ_DECK_CA`), because a deck
  behind a proxy with its own CA is one curl refuses to talk to, and the
  reference deployment is exactly that. `--insecure` exists too, and says
  in the script what it costs: the challenge this script signs is what
  authorises enrolling a browser, so a machine in the middle of that
  connection can collect the signature and enrol one of its own.

### The console says what it knows

- **A fee the run has not measured is not a zero.** The books charge a
  fee from a schedule, and a live run is built without one — so `fees`
  was zero for every live run and the console drew it beside realized
  and funding as though it were a measurement. The venue states the
  commission on each fill; a run books that now, checks the total
  against the venue's own trade records when it ends, and reports
  *not measured* until it has one. A measured zero and a figure that
  could not be measured are opposite facts, and the page no longer
  conflates them.
- **A venue reading older than the run is not a disagreement.** The
  console compares the newest journal against a reading it rewrites
  about once a minute, so for that minute after a restart it was
  comparing two runs — every order they have apart counted in both
  directions. It says which run the reading is of, and that it catches
  up.
- **A refusal says which fact stopped it.** `undecodable`,
  `no adoption record`, or `the reading predates the run` were one
  sentence; the sentence could not describe a case it did not know
  about, and the new one had no sentence at all.
- **The step-up code counts its failures** — five wrong ones close it
  for fifteen minutes, the same numbers signing in uses. A six-digit
  code is a million guesses and what sits behind this one is stopping
  the trader.
- **Listing the strategies is a read.** It called the code that sends
  an instance whose configuration moved back to draft, so opening a page
  moved a strategy. The step taken refuses instead, which is also where
  the gate has to see it.
- **The two codes are two entries, and the console says so.** The host
  agent's step-up secret is the agent's — a compromised deck cannot mint
  one — so nothing here can show it, and nothing said where it comes
  from. The field that asks for it, first-run setup, and Settings all do
  now; enrolling only the console's is why a first deploy stops at the
  code.
- **The sign-in screens are in this round's style.** They were not: a
  different product mark, a card shadow heavier than every other card's
  and not from the theme, and the language and theme switches — the two
  things this release is about — unreachable until after signing in.
- **This repository names no deployment of its own.** A release's
  allow-list was a constant holding one operator's private binary, and
  the unit, channel and prefix defaults were that deployment's. They are
  configuration now, the mark and the copy are generic, and the
  defaults a deployment must set are in the README's table.

### English, beside Chinese

- **The interface is in Chinese and English,** switched from the top bar
  or Settings and remembered in the browser. Every piece of copy is
  written as `tr("中文", "English")` where it is used, and
  `scripts/check-i18n.py` (run in CI) fails on Chinese that has no
  English beside it.
- **The deck answers in the reader's language:** refusals, capability
  reasons and attribution's missing inputs follow the request's
  `Accept-Language`.
- **So does the host agent.** Every sentence it words is a pair
  (`oq_deck_core::lang::Said`) — a config file that changed under a
  write, a deploy whose health check failed, a promotion the gate
  refuses, a control port that stops answering. The agent does not know
  which language the console is being read in, so it sends both and the
  deck picks.
- **The promotion gate's refusal is a pair on the wire**
  (`decision.reason` is `{zh, en}`). The console used to recover the
  English by matching the gate's Chinese with a regex table that had to
  be kept in step by hand; the table is gone.
- **Records keep both renderings, in sibling fields:** `reason` and
  `reason_en`, `result` and `result_en`, `problem`, `outcome`, `step`,
  `message`. A record written before the console had two languages
  carries one rendering, and a reader is shown that one rather than a
  blank.
- **A person's own words are not translated.** The reason an operator
  types for an action is stored as they typed it, in both fields.
- The notification channel keeps the Chinese. Settings chooses the
  language and the theme instead of describing them.

### v4 interface — organised by task, and a console rather than a list

- **Navigation grouped by what the operator does** (交易 / 诊断 / 变更 /
  研究 / 系统), with icons, instead of nineteen flat entries. Old paths
  redirect to their new places.
- **A top bar that is always in view:** environment (testnet / mainnet)
  and host, the trader's state, the alert count, the user menu.
- **The overview answers "is everything all right":** a status banner
  with halt beside it, the run's P&L, equity, positions and orders, a
  health checklist where each line links to where it is dealt with,
  alerts, recent activity, and the last day's resources.
- **One page for the live trader** (positions, orders, fills, market,
  risk limits), and one for reconciliation and attribution.
- **Every screen rebuilt on one component kit** (cards, stats, tables,
  tabs, drawers, charts on ECharts), each screen loaded when first
  visited. The novice/expert switch is gone: terms explain themselves
  on hover, and detail folds away.
- The black box reads its files a line at a time and judges memory
  growth every ten minutes in one pass, instead of parsing a day of
  samples every thirty seconds for each service; a week's window reads
  in under 200 ms. The last deployment's steps survive the agent
  restarting, and unit start times come from systemd as timestamps.

### M2 — operating the host, and every screen of the brief

- **A host agent, `oq-agent`,** runs on the trading host as its own user
  and is the only thing the deck asks to act. It reads the trader's
  status, resting orders and halt state from its control port; starts,
  stops and restarts the units it is allowed to; halts, shuts down and
  resumes the trader; and deploys releases only if they carry a trusted
  signature, with a five-minute health check and automatic rollback.
  Risky requests need a reason and a one-time code the agent verifies,
  so a compromised deck cannot act alone. Every action goes into a
  hash-chained audit trail and to the alert channel. (#14, #15)
- **The screens the brief asks for:** attribution from run files or live
  from the trader's shadow; live reconciliation against the watcher's
  latest reading without pasting, with a market block and the fills;
  journal replay event by event; settings; novice and expert modes with
  a glossary; the four states on every screen. (#17)
- **Configuration** (form and raw JSON, a diff before saving, a backup
  of every version, rollback), **the promotion gate** for strategy
  instances (a configuration change sends one back to draft), **alerts**
  (history, test send, silencing), **accounts** (which key fingerprint
  each process uses) and **the live feed's quality**. (#18)
- **A black box.** Every 30 s, kept 90 days: the host, each service and
  the trader's status, with state changes and alerts as events; a review
  page that opens any moment with the snapshot, the trader's decisions
  and fills around it, and its systemd-journal output around it. Logs
  read each service's journal; a service whose memory keeps growing
  raises an alert. (#19, #20, #22)
- **Sweeps.** `.sweep` files are shown verdict first: refusals, deflated
  Sharpe and PBO before the table of configurations. (#21)
- **Markouts** on the compare page. (#11)
- **The trader's P&L and risk limits** on the operations page and in the
  black box. (#23)

### M1 — reconciliation, attribution, and a door

- **Authentication is now unconditional.** It was previously required
  only off the loopback interface, on the reasoning that the operating
  system keeps strangers off `127.0.0.1`. It does not: loopback is not a
  user boundary, DNS rebinding reaches it from the operator's own
  browser, `ssh -L` dissolves the binding, and with writes enabled the
  console places orders. Argon2id passwords, a `Host` allowlist, an
  `Origin` check on writes, `HttpOnly`/`SameSite=Strict` sessions with
  idle and absolute expiry, lockout after repeated failures, and TOTP —
  optional on loopback, mandatory off it. First run prints a one-time
  token to its own terminal. See `docs/SECURITY.zh-CN.md`.
- **Live reconciliation.** Rebuilds what a process believed it held from
  its own journal and compares it against a venue record the operator
  pastes in. Reports undecodable frames, because a belief rebuilt from a
  journal with holes may agree by luck.
- **Gap attribution.** Decomposes live minus model into five causes,
  with the evidence assembled from the framework's own fill alignment
  rather than a private pairing. Keeps "measured zero" and "not
  measured" apart, returns a null residual whenever any cause is
  unavailable, and says what would make the missing ones available.

### M0 — a console that can read a run

- Reads a directory of run files through `oq_parity::wire`, listing the
  ones that will not parse alongside the ones that did, with the reason.
- Distinguishes comparable, code-changed and invalidated baselines, and
  never reports an invalidated one as agreement.
- Capability self-report drives the interface.
- Promotion gate as a domain type.
- Backend rewritten from Python to Rust when the scope narrowed to
  OpenQuanter 2.0. The reasoning is in `docs/STACK.zh-CN.md`.
