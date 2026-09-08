# Changelog

## Unreleased

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
