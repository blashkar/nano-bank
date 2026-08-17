from __future__ import annotations
import json
from typing import Callable, Optional

from fastapi import FastAPI
from fastapi.responses import StreamingResponse
from pydantic import BaseModel

from .config import Settings
from .agent import ask as default_ask
from .agent import ask_stream as default_ask_stream
from . import escalations as _esc


class AskRequest(BaseModel):
    message: str
    thread_id: Optional[str] = None


class Escalation(BaseModel):
    cx_issue_id: str
    customer_id: Optional[str] = None
    severity: Optional[str] = None
    category: Optional[str] = None
    summary: Optional[str] = None


def _default_probes(settings: Settings) -> dict:
    """Best-effort dependency probes for /health. Each returns a bool and never
    raises; a down dependency degrades the report, it does not 500 the endpoint."""
    def ollama() -> bool:
        from . import model_factory as mf
        return mf.backend_healthcheck(settings)

    def cx_mcp() -> bool:
        import anyio
        from .tools import get_tools
        try:
            return len(anyio.run(get_tools, settings)) > 0
        except Exception:  # noqa: BLE001
            return False

    def qdrant() -> bool:
        try:
            from qdrant_client import QdrantClient
            QdrantClient(url=settings.qdrant_url).get_collections()
            return True
        except Exception:  # noqa: BLE001
            return False

    return {"ollama": ollama, "cx_mcp": cx_mcp, "qdrant": qdrant}


def create_app(settings: Settings, ask_fn: Optional[Callable] = None,
               probes: Optional[dict] = None,
               ask_stream_fn: Optional[Callable] = None) -> FastAPI:
    ask_fn = ask_fn or default_ask
    ask_stream_fn = ask_stream_fn or default_ask_stream
    probes = probes if probes is not None else _default_probes(settings)
    app = FastAPI(title="nano-bank CXO")

    @app.get("/livez")
    def livez():
        # Liveness only: is the process up? No dependency probes / model round-trip.
        return {"status": "ok", "service": "cxo"}

    @app.get("/health")
    def health():
        checks = {}
        for name, probe in probes.items():
            try:
                checks[name] = bool(probe())
            except Exception:  # noqa: BLE001
                checks[name] = False
        return {"status": "ok", "service": "cxo", "checks": checks}

    @app.post("/ask")
    async def ask_endpoint(req: AskRequest):
        return await ask_fn(settings, req.message, req.thread_id)

    @app.post("/ask/stream")
    async def ask_stream_endpoint(req: AskRequest):
        async def gen():
            async for chunk in ask_stream_fn(settings, req.message, req.thread_id):
                yield json.dumps(chunk) + "\n"

        return StreamingResponse(gen(), media_type="application/x-ndjson")

    @app.post("/escalations")
    def escalations_intake(item: Escalation):
        # Record the pointer only; the durable record is the cx_issues row the PM
        # already wrote. The CXO re-grounds it via the cx service when it reports.
        _esc.record(item.model_dump())
        return {"recorded": True}

    return app
