# Quanterdeck

**A self-hosted console for OpenQuanter. Your keys stay on your machine.**

English · [中文](README.zh-CN.md)

> ⚠️ Early development (v0.0.1, M0). APIs are unstable. Not financial
> advice; use at your own risk.

---

## What this is

[OpenQuanter](https://github.com/openquanter/openquanter) accounts for
every cent — identity triples, fill-by-fill parity, gap attribution, the
residual that will not decompose. Those numbers currently exist in files
and in terminal output. Quanterdeck is the layer that makes them
something you can look at.

> **Someone with no quant experience gets their first equity curve on
> their own machine within thirty minutes; someone with experience never
> reads "cannot tell" as "all clear".**

The second half matters as much as the first. Part of this console's job
is to say no.

## What it is not

- **Not hosted, not a SaaS, and it does not hold your API keys.** Custody
  of user keys is a non-goal upstream, and it is one here.
- **Not a market terminal, a copy-trading network, or a strategy store.**
- **Not a judge of whether your strategy is any good.**
- **Not a change to the framework.** The console is a consumer with a
  one-way dependency on the public crates. The framework need not know it
  exists.

## Three rules, in code rather than in prose

**1. "Cannot tell" never renders as "they agree".** When a baseline is
invalidated — the input data or effective configuration moved — nothing
about the engine can be concluded. `passes` is false, the difference list
is empty, and the banner is amber rather than red, with what to do about
it: rebase. It is not a regression, and colouring it like one sends
someone hunting a bug that is not there.

**2. An incomplete decomposition has an unknown residual, not a zero
one.** When any component is unavailable, the residual is `None`. A zero
computed from a partial decomposition claims everything was explained,
which is precisely the claim it must not make.

**3. Reading never changes the runtime.** The console is an observer.
That is upstream's FR-CORE-7, applied here.

## Status

M0. Against a directory of run files it can:

- list every run **including the one that will not parse** — with its
  reason, rather than absent from the listing
- show the identity triple, the fills and the realized P&L; an untagged
  fill and an empty-tagged one look different, because they are
- compare two runs and distinguish three verdicts: comparable, code
  changed, and **baseline invalidated**
- report its own capabilities, rendering no control for what it cannot
  do and saying why
- start read-only and loopback-only, and **refuse to start** when told to
  listen elsewhere without a password and a second factor

Not yet: journal replay, live reconciliation, the attribution view,
sweeps, data quality, the setup wizard.

## Getting started

```bash
git clone https://github.com/openquanter/quanterdeck
cd quanterdeck

cd web && npm install && npm run build && cd ..   # Node is build-time only
export OQ_DECK_RUNS_DIR=/path/to/your/runs
cargo run -p oq-deck                              # http://127.0.0.1:8899
```

With no runs of your own, use the ones in the repository:

```bash
export OQ_DECK_RUNS_DIR=$PWD/examples/fixtures/runs
```

They are written by the framework's own writer
(`cargo run -p oq-deck-core --example make_fixtures`), so the format is
correct by construction — and one of them is deliberately truncated, to
show what the listing does with a file it cannot read.

## Licence

Apache-2.0. Contributions need a DCO sign-off (`git commit -s`); see
[CONTRIBUTING.md](CONTRIBUTING.md).
