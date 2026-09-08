from fastapi import APIRouter

from ... import __version__

router = APIRouter(tags=["health"])


@router.get("/health")
def health() -> dict[str, str]:
    """Liveness. Deliberately says nothing about the runtime.

    A probe that reported the runtime's health here would go red when the
    thing being managed went down, and a restart loop on the console is
    the last thing an operator needs at that moment.
    """
    return {"status": "ok", "version": __version__}
