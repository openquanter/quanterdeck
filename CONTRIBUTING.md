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

The same checks CI runs:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p oq-deck-core --example make_fixtures && git diff --exit-code examples/fixtures/runs
scripts/check-incoming.sh
scripts/check-i18n.py
cd web && npm ci && npm run build
```

## Interface copy is written in both languages

The console is in Chinese and English. Every piece of interface copy is
written as `tr("中文", "English")` where it is used, both languages
side by side, so one cannot be added without the other;
`scripts/check-i18n.py` fails on Chinese anywhere else. Text the deck or
the agent writes for a person follows the same rule: the deck answers in
the request's `Accept-Language`, and the agent sends `message` with
`message_en` beside it. Data — log lines, journal fields, a venue's own
error text — is shown as it is.

The READMEs' pictures follow the same split: each README shows the
console reading in its own language, so `scripts/screenshots.py` is run
once per `--lang` and the sets live apart under `docs/screenshots/<lang>/`.
One set standing in for both is how the English README came to show
Chinese screens.

## The invariants

`AGENTS.md` lists ten rules that hold regardless of what the tests say.
Most were paid for by a real incident. If a change needs one of them
relaxed, that is a discussion to have in an issue first — not a diff.

## What a good issue looks like

The OpenQuanter revision it was built against (the `rev` in
`Cargo.toml`), your OS and browser, what you expected, what happened, and the relevant lines from the console's own
output. A screenshot helps for UI problems and is not enough on its own.
