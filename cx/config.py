from __future__ import annotations
import os
from dataclasses import dataclass
from typing import Mapping, Optional


@dataclass
class Settings:
    db: dict
    mcp_port: int
    default_window_days: int

    @classmethod
    def from_env(cls, env: Optional[Mapping[str, str]] = None) -> "Settings":
        e = os.environ if env is None else env
        g = lambda k, d="": e.get(k, d)  # noqa: E731
        return cls(
            db=dict(host=g("DB_HOST", "::1"), port=int(g("DB_PORT", "5432")),
                    dbname=g("DB_NAME", "nano_bank_db"), user=g("DB_USER", "nanobank_user"),
                    password=g("DB_PASSWORD", "secure_nano_password_2024!")),
            mcp_port=int(g("MCP_PORT", "8097")),
            default_window_days=int(g("CX_WINDOW_DAYS", "30")),
        )
