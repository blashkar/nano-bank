"""The CEO's tools: the shared consult/direct primitive, wired to the four officer
seats. Consult all four; direct the two acting seats (COO, CTO). The CEO holds no
domain MCP of its own — its knowledge comes from the officers it consults."""
from __future__ import annotations

from csuite import collab

from .config import Settings
from .audit import Audit


def get_tools(settings: Settings, *, audit=None, client=None) -> list:
    audit = audit if audit is not None else Audit(settings.db)
    return collab.build_tools(settings.peer_registry(), audit, client=client)
