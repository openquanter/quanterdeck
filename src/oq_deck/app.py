"""FastAPI application assembly."""

from __future__ import annotations

from pathlib import Path

from fastapi import FastAPI
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
            "/", StaticFiles(directory=WEB_DIST, html=True), name="web"
        )

    return app


app = create_app()
