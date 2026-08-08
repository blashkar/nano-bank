from __future__ import annotations
from typing import Callable, Optional

from fastapi import FastAPI
from pydantic import BaseModel

from .config import Settings
from .agent import ask as default_ask


class AskRequest(BaseModel):
    message: str
    thread_id: Optional[str] = None


def create_app(settings: Settings, ask_fn: Optional[Callable] = None) -> FastAPI:
    ask_fn = ask_fn or default_ask
    app = FastAPI(title="nano-bank CFO")

    @app.get("/health")
    def health():
        return {"status": "ok", "service": "cfo"}

    @app.post("/ask")
    async def ask_endpoint(req: AskRequest):
        return await ask_fn(settings, req.message, req.thread_id)

    return app
