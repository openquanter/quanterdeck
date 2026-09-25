#!/usr/bin/env python3
"""Fail if interface copy exists in one language only.

Every piece of Chinese in web/src must be the first argument of
`tr("中文", "English")`: a string literal directly after `tr(`. Chinese
anywhere else — JSX text, a plain string, a template outside `tr` — is
copy an English reader would see untranslated, and is reported with its
file and line.

Comments are ignored. A line carrying `i18n-ok` is exempt, for the rare
string that is Chinese in both languages (the language switch's own
label, "中").

    scripts/check-i18n.py            # exits 1 and lists what is missing
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "web" / "src"
CJK = re.compile(r"[　-〿一-鿿＀-￯]")


def scan(text: str):
    """Yield (index, in_string_start or None) for each CJK character outside comments."""
    i, n = 0, len(text)
    # Stack of open string literals: (quote, start); templates can nest via ${ }.
    stack: list[tuple[str, int, int]] = []  # (quote, start, brace depth inside ${})
    while i < n:
        c = text[i]
        top = stack[-1] if stack else None
        if top and top[0] in "\"'":
            if c == "\\":
                i += 2
                continue
            if c == top[0] or c == "\n":
                stack.pop()
            elif CJK.match(c):
                yield i, top[1]
            i += 1
            continue
        if top and top[0] == "`" and top[2] == 0:
            if c == "\\":
                i += 2
                continue
            if c == "`":
                stack.pop()
            elif c == "$" and i + 1 < n and text[i + 1] == "{":
                stack[-1] = ("`", top[1], 1)
                i += 2
                continue
            elif CJK.match(c):
                yield i, top[1]
            i += 1
            continue
        # Code: inside ${ } of a template, or top level.
        if top and top[0] == "`":
            if c == "{":
                stack[-1] = ("`", top[1], top[2] + 1)
            elif c == "}":
                stack[-1] = ("`", top[1], top[2] - 1)
                i += 1
                continue
        if c == "/" and i + 1 < n and text[i + 1] == "/":
            j = text.find("\n", i)
            i = n if j < 0 else j
            continue
        if c == "/" and i + 1 < n and text[i + 1] == "*":
            j = text.find("*/", i + 2)
            i = n if j < 0 else j + 2
            continue
        if c in "\"'`":
            stack.append((c, i, 0))
            i += 1
            continue
        if CJK.match(c):
            yield i, None
        i += 1


def main() -> int:
    problems = []
    for path in sorted(list(ROOT.rglob("*.tsx")) + list(ROOT.rglob("*.ts"))):
        text = path.read_text(encoding="utf-8")
        lines = text.split("\n")
        seen_lines = set()
        for index, start in scan(text):
            line_no = text.count("\n", 0, index) + 1
            if line_no in seen_lines or "i18n-ok" in lines[line_no - 1]:
                continue
            ok = start is not None and re.search(r"\btr\(\s*$", text[:start]) is not None
            if not ok:
                seen_lines.add(line_no)
                where = "JSX text or code" if start is None else "a string outside tr()"
                problems.append(f"{path.relative_to(ROOT.parent.parent)}:{line_no}: {where}: {lines[line_no - 1].strip()[:100]}")
    if problems:
        print("\n".join(problems))
        print(f"\n{len(problems)} line(s) with Chinese outside tr(\"中文\", \"English\")")
        return 1
    print("every piece of Chinese copy has its English beside it")
    return 0


if __name__ == "__main__":
    sys.exit(main())
