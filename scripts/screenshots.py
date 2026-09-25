#!/usr/bin/env python3
"""Take the README's screenshots from a running deck.

    OQ_DECK_SCREENSHOT_PASSWORD=… scripts/screenshots.py \\
        --url http://127.0.0.1:8899 --redact ../private/screenshot-redact.json

Signs in, visits each screen in SCREENS, and writes
docs/screenshots/<lang>/<name>.png at 1440x900. Rerun it after a UI change and
commit the pictures that moved.

The pictures are taken in the language of the README they illustrate, so each
README shows a console reading in its own — run it twice, once per language,
and the two sets are kept apart rather than one standing in for both.

Pictures of a real deployment would carry its names — the strategy, the host,
the order prefix, a key's fingerprint. `--redact` names a JSON file of
[pattern, replacement] pairs (regular expressions, applied in order) that is
kept outside this repository, because the patterns themselves are the private
part. They are applied to every API response before the page sees it, so
names drawn on a chart canvas are replaced too, which rewriting the page's
text afterwards could not reach. Requests go out with the original names, so a
page that asks for what it was shown still finds it. The configuration screen
is not taken: its values are a strategy's parameters.

Needs Playwright for Python (`pip install playwright && playwright install
chromium`). The password comes from the environment, not the command line,
so it stays out of shell history; a deck with a second factor also needs
OQ_DECK_SCREENSHOT_TOTP, a current code.
"""

import argparse
import json
import os
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path

from playwright.sync_api import Route, sync_playwright

# (file name, route, what to do once it has loaded)
SCREENS = [
    ("overview", "/", None),
    ("live", "/live", None),
    ("reconcile", "/reconcile", None),
    ("attribution", "/reconcile?tab=attribution", None),
    ("blackbox", "/blackbox", None),
    ("blackbox-moment", "/blackbox", "open-moment"),
    ("host", "/host", None),
    ("deploy", "/deploy", None),
    ("runs", "/runs", None),
]

OUT = Path(__file__).resolve().parent.parent / "docs" / "screenshots"

# The locale name kept in `oq-deck-lang`, and the directory its pictures go in.
LANGS = {"zh-CN": "zh", "en": "en"}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--url", default="http://127.0.0.1:8899", help="the deck, as the browser reaches it")
    ap.add_argument("--redact", type=Path, help="JSON file: {\"replace\": [[pattern, replacement], …]}")
    ap.add_argument("--out", type=Path, help="where the pictures go; defaults to docs/screenshots/<lang>")
    ap.add_argument("--only", nargs="*", help="take only these screens, by name")
    ap.add_argument("--theme", choices=["light", "dark"], default="light")
    ap.add_argument("--lang", choices=sorted(LANGS), default="zh-CN", help="the language the console is read in")
    args = ap.parse_args()
    if args.out is None:
        args.out = OUT / args.lang

    password = os.environ.get("OQ_DECK_SCREENSHOT_PASSWORD")
    if not password:
        print("set OQ_DECK_SCREENSHOT_PASSWORD", file=sys.stderr)
        return 2
    rules = []
    if args.redact:
        rules = [(re.compile(p), r) for p, r in json.loads(args.redact.read_text())["replace"]]
    else:
        print("no --redact: the pictures will show this deployment's names as they are", file=sys.stderr)

    # The page asks for things by the names it was shown — a journal id
    # that now reads "trader-…" — so a request's URL is put back before it
    # goes out. Only literal patterns can be reversed, longest replacement
    # first so a short one cannot claim part of a long one.
    literal = [(p.pattern, r) for p, r in rules if not re.search(r"[\\^$.|?*+()\[\]{}]", p.pattern)]
    literal.sort(key=lambda pr: len(pr[1]), reverse=True)

    def original(url: str) -> str:
        for pattern, replacement in literal:
            url = url.replace(replacement, pattern)
        return url

    def redact(route: Route) -> None:
        # Sent from here rather than with route.fetch: the overview asks
        # for a dozen things at once, and fetches issued from inside the
        # browser's own event loop waited on each other until they all
        # timed out.
        req = route.request
        headers = {k: v for k, v in req.headers.items() if k.lower() not in ("host", "content-length", "accept-encoding")}
        request = urllib.request.Request(original(req.url), data=req.post_data_buffer, headers=headers, method=req.method)
        try:
            with urllib.request.urlopen(request, timeout=30) as r:
                status, got, raw = r.status, dict(r.headers), r.read()
        except urllib.error.HTTPError as e:
            status, got, raw = e.code, dict(e.headers), e.read()
        body = raw.decode("utf-8", "replace")
        for pattern, replacement in rules:
            body = pattern.sub(replacement, body)
        got.pop("Content-Length", None)
        route.fulfill(status=status, headers=got, body=body)

    args.out.mkdir(parents=True, exist_ok=True)
    screens = [s for s in SCREENS if not args.only or s[0] in args.only]
    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(viewport={"width": 1440, "height": 900})
        # The theme and the language the pictures are taken in; light is the
        # console's default, and the language is stored before the first
        # render so the page and its requests are both in it from the start.
        page.add_init_script(
            f"try {{ localStorage.setItem('oq-deck-theme', '{args.theme}');"
            f" localStorage.setItem('oq-deck-lang', '{LANGS[args.lang]}'); }} catch (e) {{}}"
        )
        errors: list[str] = []
        page.on("pageerror", lambda e: errors.append(str(e)))
        if rules:
            page.route("**/api/v1/**", redact)

        page.goto(args.url + "/login")
        page.fill("input[type=password]", password)
        totp = os.environ.get("OQ_DECK_SCREENSHOT_TOTP")
        if totp:
            page.fill("input[inputmode=numeric]", totp)
        page.keyboard.press("Enter")
        page.wait_for_url(lambda u: "/login" not in u, timeout=10_000)

        for name, path, action in screens:
            page.goto(args.url + path)
            page.wait_for_load_state("networkidle")
            page.wait_for_timeout(5000)
            if action == "open-moment":
                # A click on the first chart, near its right edge, opens
                # that moment in the drawer.
                box = page.locator("canvas").first.bounding_box()
                if box:
                    page.mouse.click(box["x"] + box["width"] * 0.9, box["y"] + box["height"] * 0.5)
                    page.wait_for_load_state("networkidle")
                    page.wait_for_timeout(2500)
            target = args.out / f"{name}.png"
            page.screenshot(path=str(target))
            print(f"{target.relative_to(Path.cwd()) if target.is_relative_to(Path.cwd()) else target}")
        browser.close()
    if errors:
        print("page errors:", *errors, sep="\n  ", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
