#!/usr/bin/env bash
# Fail if `oq_adapters` has acquired a third-party import.
#
# The adapter layer is this project's extension point. A dependency here
# is one every third-party adapter author inherits, so the boundary is
# checked rather than asserted.
set -euo pipefail

cd "$(dirname "$0")/.."

python - <<'PY'
import ast
import pathlib
import sys
import sysconfig

stdlib = set(sys.stdlib_module_names)
root = pathlib.Path("src/oq_adapters")

# `probe.py` is not part of this package's import graph. It is copied out
# and executed by the *runtime's* interpreter, and importing the runtime
# is the whole of its job. Excluding it is the point of the exclusion
# list, not a hole in the check: any other file that needs to be here
# should be argued for in the pull request.
EXEMPT = {root / "legacy_py" / "probe.py"}

offences = []

for path in sorted(root.rglob("*.py")):
    if path in EXEMPT:
        continue
    tree = ast.parse(path.read_text())
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            names = [alias.name for alias in node.names]
        elif isinstance(node, ast.ImportFrom):
            if node.level:  # relative import, inside the package
                continue
            names = [node.module or ""]
        else:
            continue
        for name in names:
            top = name.split(".")[0]
            if top and top not in stdlib and top != "oq_adapters":
                offences.append(f"{path}:{node.lineno}: imports {name}")

if offences:
    print("oq_adapters must depend on the standard library only:")
    for line in offences:
        print("  " + line)
    raise SystemExit(1)

checked = [p for p in root.rglob("*.py") if p not in EXEMPT]
print(f"oq_adapters: standard library only ({len(checked)} files checked, "
      f"{len(EXEMPT)} exempt)")
PY
