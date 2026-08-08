from __future__ import annotations
import os
from dataclasses import dataclass
from typing import Mapping, Optional


@dataclass
class Settings:
    nano_bank_api: str
    service_client_secret: str
    mcp_port: int
    timeout: float

    @classmethod
    def from_env(cls, env: Optional[Mapping[str, str]] = None) -> "Settings":
        e = os.environ if env is None else env
        # The service secret is a shared credential with the bank's service plane
        # — there is no safe default. Fail loudly rather than silently minting
        # tokens with the well-known dev secret (which would work against a bank
        # left on its own default and never surface the misconfiguration).
        secret = e.get("SERVICE_CLIENT_SECRET")
        if not secret:
            raise RuntimeError(
                "SERVICE_CLIENT_SECRET is not set. It must match the bank's "
                "NANO_BANK__SECURITY__SERVICE_CLIENT_SECRET; refusing to fall "
                "back to the well-known dev default."
            )
        return cls(
            nano_bank_api=e.get("NANO_BANK_API", "http://localhost:8081"),
            service_client_secret=secret,
            mcp_port=int(e.get("MCP_PORT", "8092")),
            timeout=float(e.get("REQUEST_TIMEOUT", "10.0")),
        )
