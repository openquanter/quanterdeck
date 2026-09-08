# `_incoming/` — where generated designs land first

Design work produced outside this repository (Readdy and anything like it)
is committed here **unmodified**, in its own commit. The second commit
wires it up: split into `components/`, drop framework-specific imports,
replace the mock data with calls from `src/api/`, and replace hardcoded
colours with the tokens in `src/design/tokens.css`.

Two commits rather than one because the first is the only record of what
the design actually said. Once a screen has been adapted, deleted mock
data and renamed elements make it impossible to tell what was intended
and what was an accident of the port.

Rules:

- `tsconfig.json` excludes this directory, and nothing under `src/`
  outside it may import from it. `scripts/check-incoming.sh` fails the
  build if something does.
- A file here is temporary. When its screen ships, the file goes.
