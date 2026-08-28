# agent/cx_issue_action.py — file a customer complaint (cx_issue) and, for
# high/urgent, best-effort escalate to the CXO. The durable record is the row;
# escalation is a fire-and-forget pointer whose failure never fails the filing.
from __future__ import annotations
from typing import Callable, Optional

_ESCALATE = {"high", "urgent"}
_CATEGORIES = {"onboarding", "declines_friction", "fees", "rail_experience", "app_ux",
              "feature_request", "other"}
_SEVERITIES = {"low", "medium", "high", "urgent"}


class CxIssueError(Exception):
    """A bad category/severity, or a DB failure while filing the issue."""


def _default_post(url: str, json: dict) -> None:
    import httpx
    # a short connect timeout, separate from the overall budget: if the CXO
    # pod is down or unreachable, filing latency shouldn't wait out the full
    # 5s before the caller (who only cares that the durable row landed) gets
    # control back.
    httpx.post(url, json=json, timeout=httpx.Timeout(5.0, connect=1.0))


def file_and_maybe_escalate(db, customer_id: str, cxo_url: str, category: str,
                            severity: str, summary: str, detail: str,
                            http_post: Optional[Callable] = None) -> dict:
    if category not in _CATEGORIES:
        raise CxIssueError(f"unknown category: {category!r}")
    if severity not in _SEVERITIES:
        raise CxIssueError(f"unknown severity: {severity!r}")
    try:
        issue_id = db.insert_cx_issue(customer_id, category, severity, summary, detail)
    except Exception as e:
        raise CxIssueError(f"failed to file cx issue: {e}") from e
    if severity in _ESCALATE:
        post = http_post or _default_post
        try:
            post(f"{cxo_url.rstrip('/')}/escalations",
                 {"cx_issue_id": issue_id, "customer_id": customer_id,
                  "severity": severity, "category": category, "summary": summary})
        except Exception:  # noqa: BLE001 — escalation is best-effort
            pass
    return {"cx_issue_id": issue_id, "escalated": severity in _ESCALATE}
