"""Introspect an OpenQuanter 1.x checkout and print JSON on stdout.

This file is never imported by the server. It is copied to a temporary
path and executed **by the runtime's own interpreter**, with the runtime's
directory as the working directory. Two reasons, both load-bearing:

1. **Version independence.** A deck on Python 3.12 can manage a runtime
   pinned to 3.9. Importing the runtime in-process would weld the two
   together and make the deck un-installable next to a production box.
2. **Isolation.** Discovering a strategy class means importing the module
   that defines it, and that is user code — it may open sockets, read
   config, or fail. A subprocess that crashes costs a request; an
   in-process import that crashes costs the console.

The result is written to the file named by the second argument, not to
stdout. Importing a 1.x runtime prints a configuration banner before any
of this code runs, and a caller parsing stdout would break on it — as it
did, the first time this ran against a real checkout. stdout belongs to
whatever the runtime wants to say; the answer goes somewhere the runtime
does not write.

File contents on success:  {"ok": true, "classes": [...]}
File contents on failure:  {"ok": false, "error": "..."}
Exit code is 0 in both cases; the caller reads `ok`, so a partial failure
still carries a message rather than an empty file.

Standard library only — it runs under whatever the runtime happens to
have installed, which may be very little.
"""

import json
import os
import sys
import traceback


def _iter_strategy_modules(root):
    """Directories 1.x keeps strategy classes in, deepest last."""
    candidates = [
        os.path.join(root, "core", "app", "cta_strategy", "strategies"),
        os.path.join(root, "examples", "strategies"),
    ]
    for directory in candidates:
        if not os.path.isdir(directory):
            continue
        for name in sorted(os.listdir(directory)):
            if not name.endswith(".py") or name.startswith("_"):
                continue
            yield directory, name[:-3]


def _describe(cls, module_name):
    """Pull the declared parameters out of a CtaTemplate subclass.

    `get_class_parameters()` is part of the 1.x template, so the defaults
    the deck renders are the defaults the engine will use. Nothing is
    re-declared here; a parameter added to a strategy shows up in the
    form without touching this file.
    """
    try:
        defaults = cls.get_class_parameters()
    except Exception:
        defaults = {}

    # Known gap: the type here is inferred from the *class* default, and
    # a 1.x strategy may declare a scalar default that every instance
    # overrides with a list — `cover_gap = 500.0` in the class, a
    # seven-element ladder in the config. A form built from this alone
    # renders one number where seven belong. The instance's own value has
    # to win when there is one; the deck joins the two in the API layer
    # and this stays the declaration, not the truth.
    fields = []
    for name in getattr(cls, "parameters", []):
        value = defaults.get(name, getattr(cls, name, None))
        fields.append(
            {
                "name": name,
                "type": type(value).__name__ if value is not None else "str",
                "default": value,
                "description": "",
            }
        )

    return {
        "class_name": cls.__name__,
        "module": module_name,
        "author": getattr(cls, "author", "") or "",
        "parameters": fields,
        "variables": [
            v for v in getattr(cls, "variables", []) if isinstance(v, str)
        ],
    }


def _emit(out_path, payload):
    with open(out_path, "w") as handle:
        json.dump(payload, handle)


def main():
    root = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else ".")
    out_path = sys.argv[2]
    if root not in sys.path:
        sys.path.insert(0, root)

    try:
        from core.app.cta_strategy.template import CtaTemplate
    except Exception:
        _emit(
            out_path,
            {
                "ok": False,
                "error": "not an OpenQuanter 1.x checkout: cannot import "
                "core.app.cta_strategy.template",
                "detail": traceback.format_exc(limit=3),
            },
        )
        return

    import importlib.util

    classes = []
    errors = {}
    for directory, stem in _iter_strategy_modules(root):
        path = os.path.join(directory, stem + ".py")
        spec = importlib.util.spec_from_file_location(
            "oq_probe_" + stem, path
        )
        if spec is None or spec.loader is None:
            continue
        module = importlib.util.module_from_spec(spec)
        try:
            spec.loader.exec_module(module)
        except Exception as exc:
            # A strategy that will not import is a finding, not a crash.
            # The deck shows it next to the ones that did, so the reason a
            # class is missing is visible rather than inferred.
            errors[os.path.relpath(path, root)] = f"{type(exc).__name__}: {exc}"
            continue

        for attr in vars(module).values():
            if not isinstance(attr, type):
                continue
            if not issubclass(attr, CtaTemplate) or attr is CtaTemplate:
                continue
            if attr.__module__ != module.__name__:
                continue  # imported, not defined here
            classes.append(_describe(attr, os.path.relpath(path, root)))

    _emit(out_path, {"ok": True, "classes": classes, "errors": errors})


if __name__ == "__main__":
    main()
