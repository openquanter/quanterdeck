from dataclasses import asdict
from typing import Any

from fastapi import APIRouter, Body, Depends, HTTPException, status

from oq_adapters import RuntimeAdapter
from oq_adapters.protocol import AdapterError, ConflictError, NotSupported

from ..deps import get_adapter, require_writes

router = APIRouter(tags=["configs"])


@router.get("/configs")
def list_configs(adapter: RuntimeAdapter = Depends(get_adapter)) -> list[str]:
    try:
        return adapter.list_configs()
    except NotSupported as exc:
        raise HTTPException(status.HTTP_501_NOT_IMPLEMENTED, str(exc)) from exc
    except AdapterError as exc:
        raise HTTPException(status.HTTP_502_BAD_GATEWAY, str(exc)) from exc


@router.get("/configs/{key}")
def read_config(
    key: str, adapter: RuntimeAdapter = Depends(get_adapter)
) -> dict:
    try:
        return asdict(adapter.read_config(key))
    except AdapterError as exc:
        raise HTTPException(status.HTTP_404_NOT_FOUND, str(exc)) from exc


@router.put("/configs/{key}", dependencies=[Depends(require_writes)])
def write_config(
    key: str,
    content: dict[str, Any] = Body(...),
    etag: str = Body(...),
    adapter: RuntimeAdapter = Depends(get_adapter),
) -> dict:
    """Write a config file, after backing up what was there.

    The backup is not optional at this layer and there is no parameter to
    turn it off. `etag` is the one the caller read; a mismatch means the
    file moved underneath them and the write is refused rather than
    merged, because nothing here knows which of the two versions the
    operator meant.
    """
    try:
        return asdict(adapter.write_config(key, content, etag=etag))
    except ConflictError as exc:
        raise HTTPException(status.HTTP_412_PRECONDITION_FAILED, str(exc)) from exc
    except AdapterError as exc:
        raise HTTPException(status.HTTP_400_BAD_REQUEST, str(exc)) from exc
