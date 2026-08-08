"""The CFO's only tools: the finance MCP server (bank-wide, read-only)."""
from __future__ import annotations
from .config import Settings


def mcp_client(settings: Settings):
    from langchain_mcp_adapters.client import MultiServerMCPClient
    return MultiServerMCPClient({
        "finance": {
            "url": settings.finance_mcp_url,
            "transport": "streamable_http",
        }
    })


async def get_tools(settings: Settings) -> list:
    return await mcp_client(settings).get_tools()
