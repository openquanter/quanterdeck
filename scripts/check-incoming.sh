#!/usr/bin/env bash
# Fail if anything under web/src outside _incoming/ imports from it.
#
# tsconfig excludes web/src/_incoming, but exclusion only stops the
# compiler from starting there: a file it reaches through an import is
# compiled all the same. So the rule in web/src/_incoming/README.md —
# nothing outside may import from it — needs a check, and this is it.

set -euo pipefail
cd "$(dirname "$0")/../web/src"

hits="$(grep -rnE "from ['\"][^'\"]*_incoming|import\(['\"][^'\"]*_incoming" \
  --include='*.ts' --include='*.tsx' . | grep -v '^\./_incoming/' || true)"
if [ -n "$hits" ]; then
  echo "imports from web/src/_incoming, which is not wired up yet:" >&2
  echo "$hits" >&2
  exit 1
fi
echo "no imports from _incoming"
