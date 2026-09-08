"""FastAPI application assembly."""

from __future__ import annotations

from pathlib import Path

from fastapi import FastAPI, HTTPException
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles

from . import __version__
from .api.v1 import router as v1_router

#: The built web app, shipped inside the wheel. Absent in a source
#: checkout until `npm run build` has run, and the server is expected to
#: come up anyway so the API can be developed without Node.
WEB_DIST = Path(__file__).parent / "web"


def create_app() -> FastAPI:
    app = FastAPI(
        title="Quanterdeck",
        version=__version__,
        description="The OpenQuanter console. Self-hosted; keys stay local.",
    )
    app.include_router(v1_router)

    if WEB_DIST.is_dir():
        app.mount(
            "/assets",
            StaticFiles(directory=WEB_DIST / "assets"),
            name="assets",
        )

        @app.get("/{path:path}", include_in_schema=False)
        def spa(path: str) -> FileResponse:
            """Serve the single-page app for any path it owns.

            The router is history-based, so `/services` is a real URL a
            user can reload or bookmark, and there is no file behind it.
            A static mount alone answers those with 404 — which it did,
            the first time anyone opened a deep link.

            Paths under /api are matched by the routers above and never
            reach here; anything else that looks like a file (a stale
            asset request after a rebuild, say) is a genuine 404 rather
            than an HTML page delivered under a .js name.
            """
            candidate = WEB_DIST / path
            if path and candidate.is_file():
                return FileResponse(candidate)
            if path and "." in path.rsplit("/", 1)[-1]:
                raise HTTPException(status_code=404, detail="not found")
            return FileResponse(WEB_DIST / "index.html")

    return app


app = create_app()
