from __future__ import annotations
from dataclasses import dataclass

from mcp.server.fastmcp import FastMCP
from mcp.server.transport_security import TransportSecuritySettings

from .config import Settings
from .db import CxDB
from . import metrics


@dataclass
class Deps:
    db: CxDB
    window_days: int


def build_mcp(deps: Deps) -> FastMCP:
    mcp = FastMCP("nano-cx", transport_security=TransportSecuritySettings(
        enable_dns_rebinding_protection=False))
    w = deps.window_days
    db = deps.db

    @mcp.tool()
    def onboarding_funnel() -> dict:
        """Onboarding/activation funnel: customers, KYC completion, account activation."""
        return metrics.onboarding_funnel(db.customers_onboarding(), db.accounts_activation())

    @mcp.tool()
    def product_adoption(window_days: int = w) -> dict:
        """Per-product adoption: % of active customers who transacted on each product."""
        return metrics.product_adoption(db.product_activity(window_days),
                                        db.active_customer_count(window_days))

    @mcp.tool()
    def friction_metrics(window_days: int = w) -> dict:
        """Where customers hit walls: transaction failure rate + Interac outcome mix."""
        return metrics.friction_metrics(db.transaction_outcomes(window_days),
                                        db.interac_outcomes(window_days))

    @mcp.tool()
    def engagement_metrics(window_days: int = w) -> dict:
        """Active vs dormant customers over the window (retention health)."""
        return metrics.engagement_metrics(db.customer_recency(), window_days)

    @mcp.tool()
    def issue_summary() -> dict:
        """Customer-voice: open cx_issues by category & severity, resolved count, top theme."""
        return metrics.issue_summary(db.issue_rows())

    @mcp.tool()
    def notable_issues(limit: int = 5) -> list:
        """The most severe recent individual issues (with scoped customer_id for re-grounding)."""
        return metrics.notable_issues(db.issue_rows(), limit=limit)

    @mcp.tool()
    def issue_detail(issue_id: str) -> dict:
        """A single cx_issue by id — used to re-ground a personal-manager escalation."""
        return db.issue_by_id(issue_id) or {}

    @mcp.tool()
    def cx_summary() -> dict:
        """Headline CX posture: onboarding + adoption + friction + engagement + issues."""
        return {"onboarding": metrics.onboarding_funnel(db.customers_onboarding(),
                                                        db.accounts_activation()),
                "adoption": metrics.product_adoption(db.product_activity(w),
                                                     db.active_customer_count(w)),
                "friction": metrics.friction_metrics(db.transaction_outcomes(w),
                                                     db.interac_outcomes(w)),
                "engagement": metrics.engagement_metrics(db.customer_recency(), w),
                "issues": metrics.issue_summary(db.issue_rows())}

    return mcp


def main() -> None:
    import uvicorn
    s = Settings.from_env()
    mcp = build_mcp(Deps(db=CxDB(s.db), window_days=s.default_window_days))
    uvicorn.run(mcp.streamable_http_app(), host="0.0.0.0", port=s.mcp_port)


if __name__ == "__main__":
    main()
