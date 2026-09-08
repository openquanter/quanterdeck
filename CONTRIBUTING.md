# Contributing to Quanterdeck

Thanks for your interest. The project is early; the most useful things
right now are trying it against your own OpenQuanter checkout, filing
precise issues, and arguing with the design documents.

## Ground rules

- **Licence**: Apache-2.0. Contributions are accepted under it.
- **Sign-off**: every commit needs a DCO sign-off (`git commit -s`),
  certifying you have the right to submit the code. CI checks it.
- **Human attribution**: commits and pull requests must not credit an AI
  assistant. CI checks this too.
- **No proprietary content**: never submit exchange credentials, captured
  market data, strategies carrying live parameters, or deployment
  topology. This console is used against production systems; the
  repository must stay safe to make public.

## Before you open a pull request

```bash
pytest
ruff check src tests
scripts/check-adapter-deps.sh
cd web && npm run build
```

## The invariants

`AGENTS.md` lists eight rules that hold regardless of what the tests say.
Most were paid for by a real incident. If a change needs one of them
relaxed, that is a discussion to have in an issue first — not a diff.

## What a good issue looks like

The runtime you pointed it at (1.x or 2.0), its Python version, what you
expected, what happened, and the relevant lines from the console's own
output. A screenshot helps for UI problems and is not enough on its own.
