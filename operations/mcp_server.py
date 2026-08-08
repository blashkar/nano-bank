"""The operations MCP: the COO's back-office perception surface. Each tool reads
the bank's service-plane back-office endpoints via BankClient and returns a pure
metrics aggregation. Money Decimals are stringified for JSON transport."""
from __future__ import annotations
from decimal import Decimal

from mcp.server.fastmcp import FastMCP
from mcp.server.transport_security import TransportSecuritySettings

from .config import Settings
from .bank_client import BankClient
from . import metrics


def _stringify(obj):
    if isinstance(obj, Decimal):
        return str(obj)
    if isinstance(obj, dict):
        return {k: _stringify(v) for k, v in obj.items()}
    if isinstance(obj, list):
        return [_stringify(v) for v in obj]
    return obj


def build_mcp(bank: BankClient) -> FastMCP:
    mcp = FastMCP(
        "nano-operations",
        transport_security=TransportSecuritySettings(enable_dns_rebinding_protection=False),
    )

    @mcp.tool()
    def float_position() -> dict:
        """Current clearing/settlement float across the rails, totalled by system."""
        return _stringify(metrics.float_summary(bank.float_()))

    @mcp.tool()
    def transactions(window: str = "24h") -> dict:
        """Transaction volume/count rolled up by type over a window (24h|7d|30d)."""
        return _stringify(metrics.transactions_summary(bank.transactions(window)))

    @mcp.tool()
    def rails(window: str = "24h") -> dict:
        """Per-rail (Interac/AFT/Lynx) activity by status over a window."""
        return _stringify(metrics.rails_summary(bank.rails(window)))

    @mcp.tool()
    def exceptions(window: str = "24h") -> dict:
        """Recorded operational exceptions (failed txns, reversals, AFT returns/rejects, recalls)."""
        return _stringify(metrics.exceptions_summary(bank.exceptions(window)))

    @mcp.tool()
    def cards(window: str = "24h") -> dict:
        """Open authorization holds (now) + captured card transactions over a window."""
        return _stringify(metrics.cards_summary(bank.cards(window)))

    @mcp.tool()
    def operations_health(window: str = "24h") -> dict:
        """One-shot bundle: float, transactions, rails, exceptions and cards for a window."""
        return _stringify(
            {
                "float": metrics.float_summary(bank.float_()),
                "transactions": metrics.transactions_summary(bank.transactions(window)),
                "rails": metrics.rails_summary(bank.rails(window)),
                "exceptions": metrics.exceptions_summary(bank.exceptions(window)),
                "cards": metrics.cards_summary(bank.cards(window)),
            }
        )

    return mcp


def main():
    settings = Settings.from_env()
    mcp = build_mcp(BankClient(settings))
    import uvicorn

    uvicorn.run(mcp.streamable_http_app(), host="0.0.0.0", port=settings.mcp_port)


if __name__ == "__main__":
    main()
