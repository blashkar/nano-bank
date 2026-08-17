# agent/cx_issue_action.py — file a customer complaint (cx_issue) and, for
# high/urgent, best-effort escalate to the CXO. The durable record is the row;
# escalation is a fire-and-forget pointer whose failure never fails the filing.
from __future__ import annotations
from typing import Callable, Optional

_ESCALATE = {"high", "urgent"}


def _default_post(url: str, json: dict) -> None:
    import httpx
    httpx.post(url, json=json, timeout=5)


def file_and_maybe_escalate(db, customer_id: str, cxo_url: str, category: str,
                            severity: str, summary: str, detail: str,
                            http_post: Optional[Callable] = None) -> dict:
    issue_id = db.insert_cx_issue(customer_id, category, severity, summary, detail)
    if severity in _ESCALATE:
        post = http_post or _default_post
        try:
            post(f"{cxo_url.rstrip('/')}/escalations",
                 {"cx_issue_id": issue_id, "customer_id": customer_id,
                  "severity": severity, "category": category, "summary": summary})
        except Exception:  # noqa: BLE001 — escalation is best-effort
            pass
    return {"cx_issue_id": issue_id, "escalated": severity in _ESCALATE}
