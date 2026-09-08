from dataclasses import asdict

from fastapi import APIRouter, Body, Depends, HTTPException, status

from oq_adapters import RuntimeAdapter, ServiceAction
from oq_adapters.legacy_py.services import DESTRUCTIVE_STOP
from oq_adapters.protocol import AdapterError, NotSupported, RefusedError

from ..deps import get_adapter, require_writes

router = APIRouter(tags=["services"])


@router.get("/services")
def list_services(adapter: RuntimeAdapter = Depends(get_adapter)) -> list[dict]:
    try:
        return [asdict(s) for s in adapter.list_services()]
    except NotSupported as exc:
        raise HTTPException(status.HTTP_501_NOT_IMPLEMENTED, str(exc)) from exc
    except AdapterError as exc:
        raise HTTPException(status.HTTP_502_BAD_GATEWAY, str(exc)) from exc


@router.post("/services/{name}/{action}", dependencies=[Depends(require_writes)])
def act(
    name: str,
    action: ServiceAction,
    confirm: str = Body(default="", embed=True),
    adapter: RuntimeAdapter = Depends(get_adapter),
) -> dict:
    """Perform one action on one service.

    `confirm` must equal the service's own name for the actions that
    carry a consequence. Typing the name is the point: it is the step
    that makes an operator read which service they are about to stop,
    which a yes/no dialog does not.
    """
    confirmed = confirm.strip() == name
    try:
        result = adapter.service_action(name, action, confirmed=confirmed)
    except RefusedError as exc:
        raise HTTPException(
            status.HTTP_409_CONFLICT,
            detail={
                "reason": exc.reason,
                "confirmation_required": name,
                "consequence": DESTRUCTIVE_STOP.get(name, ""),
            },
        ) from exc
    except NotSupported as exc:
        raise HTTPException(status.HTTP_501_NOT_IMPLEMENTED, str(exc)) from exc
    except AdapterError as exc:
        raise HTTPException(status.HTTP_502_BAD_GATEWAY, str(exc)) from exc
    return asdict(result)
