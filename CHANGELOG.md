# Changelog

## Unreleased

### M0 — a console that can read a run

- Reads a directory of run files through `oq_parity::wire`, listing the
  ones that will not parse alongside the ones that did, with the reason.
- Distinguishes comparable, code-changed and invalidated baselines, and
  never reports an invalidated one as agreement.
- Capability self-report drives the interface: a section the deck cannot
  back produces no link, and the reason appears on the overview.
- Promotion gate as a domain type: backtest, observation window,
  sign-off; a configuration change voids the evidence, a code change
  does not.
- Read-only and loopback-only by default; both need a deliberate act to
  relax, and an under-configured public bind refuses to start.
- Backend rewritten from Python to Rust when the scope narrowed to
  OpenQuanter 2.0. The reasoning is in `docs/STACK.zh-CN.md`.
