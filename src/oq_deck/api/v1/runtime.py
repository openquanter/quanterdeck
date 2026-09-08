from dataclasses import asdict

from fastapi import APIRouter, Depends

from oq_adapters import RuntimeAdapter

from ..deps import get_adapter

router = APIRouter(tags=["runtime"])


@router.get("/runtime/capabilities")
def capabilities(adapter: RuntimeAdapter = Depends(get_adapter)) -> dict:
    """What this deck's runtime can do.

    The web app renders its navigation from this. A capability that is
    off produces no control at all, rather than a control that fails when
    pressed — the difference between a console that is honest about an
    unfinished runtime and one that looks broken.
    """
    return asdict(adapter.capabilities())
