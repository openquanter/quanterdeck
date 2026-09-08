# Quanterdeck

**A self-hosted console for OpenQuanter. Your keys stay on your machine.**

English · [中文](README.zh-CN.md)

> ⚠️ Early development (v0.0.1, M0). APIs are unstable. Not financial
> advice; use at your own risk.

---

## What this is

OpenQuanter is a trading framework with no interface. Running it means
editing JSON, remembering what `manager.sh` accepts, and knowing which
actions cancel your resting orders. Quanterdeck is the layer that was
missing:

> **Someone with no quant experience gets their first equity curve on
> their own machine within thirty minutes; someone with experience never
> loses their resting orders to a mis-clicked button.**

The second half matters as much as the first. Part of this console's job
is to say no.

## What it is not

- **Not hosted, not a SaaS, and it does not hold your API keys.** Keys are
  encrypted on your machine and never appear in an API response, a log, or
  a commit. Custody of user keys is a non-goal upstream, and it is one
  here.
- **Not a market terminal, a copy-trading network, or a strategy store.**
- **Not a judge of whether your strategy is any good.** It guarantees you
  ran a backtest before going live. It does not guarantee the backtest
  meant anything.

## Three rules, in code rather than in prose

**1. Reading never changes the runtime.** Every read path is free of
side effects on the running system. The authority for a position is the
exchange, not a handover file on disk — so the console neither reads nor
writes `cta_strategy_data.json`.

**2. Dangerous actions are typed, not toggled.** The trading daemon
offers Restart, which signals it to hand its orders over. Stop and Start
sit in a danger zone, require the service name typed out, and state the
consequence verbatim: every resting order cancelled, order ownership
lost.

**3. A write is a backup plus a write.** They are one operation and there
is no flag that separates them. A stale etag is refused rather than
merged, because nothing here knows which of the two versions you meant.

## Status

M0. Against a real OpenQuanter 1.x checkout it can already:

- read the runtime's capabilities and render from them, so a feature the
  runtime lacks produces no control rather than one that errors
- list and drive the ten services behind `manager.sh`
- **generate parameter forms from the strategy classes themselves** —
  the fields come from the runtime's code, not from a schema kept here
- read and write configuration, with backups, diffs and conflict detection
- refuse every write while in read-only mode, which is the default

Not yet: backtest submission, the live view, the promotion gate, the
setup wizard, the 2.0 adapter. See [docs/BLUEPRINT.zh-CN.md](docs/BLUEPRINT.zh-CN.md).

## Getting started

```bash
git clone https://github.com/openquanter/quanterdeck
cd quanterdeck

pip install -e ".[dev]"
cd web && npm install && npm run build && cd ..

export OQ_DECK_RUNTIME_ROOT=/path/to/openquanter
export OQ_DECK_RUNTIME_PYTHON=/path/to/that/repo/env/bin/python

oq-deck up          # http://127.0.0.1:8899
```

`OQ_DECK_RUNTIME_PYTHON` is the *runtime's* interpreter, which is usually
not the one running the console. The console never imports the runtime;
[docs/STACK.zh-CN.md](docs/STACK.zh-CN.md) §3 explains why.

It binds `127.0.0.1` and starts read-only. Binding anywhere else without
a password and a second factor is refused at startup — a console that can
place orders does not get a convenient default that exposes it.

## Licence

Apache-2.0. Contributions need a DCO sign-off (`git commit -s`); see
[CONTRIBUTING.md](CONTRIBUTING.md).
