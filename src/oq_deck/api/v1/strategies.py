from dataclasses import asdict

from fastapi import APIRouter, Depends, HTTPException, status

from oq_adapters import RuntimeAdapter
from oq_adapters.protocol import AdapterError, NotSupported

from ..deps import get_adapter

router = APIRouter(tags=["strategies"])


@router.get("/strategies/classes")
def list_classes(adapter: RuntimeAdapter = Depends(get_adapter)) -> list[dict]:
    """Strategy classes and the parameters they declare.

    The parameter list comes from the runtime, so the form the operator
    fills in is generated from the code that will read it. Adding a
    parameter to a strategy is enough; there is no schema here to keep in
    step, and therefore none to fall out of step.
    """
    try:
        return [asdict(c) for c in adapter.list_strategy_classes()]
    except NotSupported as exc:
        raise HTTPException(status.HTTP_501_NOT_IMPLEMENTED, str(exc)) from exc
    except AdapterError as exc:
        raise HTTPException(status.HTTP_502_BAD_GATEWAY, str(exc)) from exc


@router.get("/strategies/instances")
def list_instances(adapter: RuntimeAdapter = Depends(get_adapter)) -> list[dict]:
    try:
        return [asdict(i) for i in adapter.list_strategy_instances()]
    except NotSupported as exc:
        raise HTTPException(status.HTTP_501_NOT_IMPLEMENTED, str(exc)) from exc
    except AdapterError as exc:
        raise HTTPException(status.HTTP_502_BAD_GATEWAY, str(exc)) from exc
