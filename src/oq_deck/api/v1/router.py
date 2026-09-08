"""Route table for /api/v1."""

from fastapi import APIRouter

from . import configs, health, runtime, services, strategies

router = APIRouter(prefix="/api/v1")
router.include_router(health.router)
router.include_router(runtime.router)
router.include_router(services.router)
router.include_router(configs.router)
router.include_router(strategies.router)
