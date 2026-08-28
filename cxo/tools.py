"""The CXO's domain tools: the cx metrics MCP (read-only CX signals) plus a local
`pending_escalations` tool that re-grounds each personal-manager escalation."""
from __future__ import annotations
from langchain_core.tools import tool

from .config import Settings
from . import escalations


def mcp_client(settings: Settings):
    from langchain_mcp_adapters.client import MultiServerMCPClient
    return MultiServerMCPClient({
        "cx": {"url": settings.cx_mcp_url, "transport": "streamable_http"}})


async def get_tools(settings: Settings) -> list:
    cx_tools = await mcp_client(settings).get_tools()
    detail = next((t for t in cx_tools if t.name == "issue_detail"), None)

    @tool
    async def pending_escalations() -> list:
        """Personal-manager escalations awaiting the CXO's attention. Each is
        RE-GROUNDED by reading its cx_issue from the cx service (never trust the
        ping payload). Returns [{cx_issue_id, severity, issue}]."""
        out = []
        for e in escalations.pending():
            grounded: dict = {}
            if detail is not None and e.get("cx_issue_id"):
                try:
                    grounded = await detail.ainvoke({"issue_id": e["cx_issue_id"]})
                except Exception:  # noqa: BLE001
                    grounded = {}
            out.append({"cx_issue_id": e.get("cx_issue_id"),
                        "severity": e.get("severity"), "issue": grounded})
        return out

    return cx_tools + [pending_escalations]
