# Changelog

## Unreleased

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
