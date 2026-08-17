# cx/metrics.py — pure CX metric functions over CxDB row-dicts. No DB, no I/O.
from __future__ import annotations
import datetime as dt

_SEV_RANK = {"urgent": 3, "high": 2, "medium": 1, "low": 0}


def pct(numer, denom) -> float:
    return round(100.0 * numer / denom, 2) if denom else 0.0


def onboarding_funnel(cust_rows: list[dict], acct_rows: list[dict]) -> dict:
    c = cust_rows[0] if cust_rows else {"total": 0, "kyc_completed": 0, "kyc_pending": 0}
    a = acct_rows[0] if acct_rows else {"total": 0, "active": 0, "pending_activation": 0}
    return {"customers": c["total"], "kyc_completed": c["kyc_completed"],
            "kyc_pending": c["kyc_pending"],
            "kyc_completion_rate": pct(c["kyc_completed"], c["total"]),
            "accounts": a["total"], "accounts_active": a["active"],
            "accounts_pending_activation": a["pending_activation"],
            "account_activation_rate": pct(a["active"], a["total"])}


def product_adoption(activity_rows: list[dict], active_rows: list[dict]) -> dict:
    active = active_rows[0]["active_customers"] if active_rows else 0
    products = [{"product": r["product"], "customers": r["customers"],
                 "adoption_rate": pct(r["customers"], active)} for r in activity_rows]
    products.sort(key=lambda d: d["customers"], reverse=True)
    return {"active_customers": active, "products": products}


def friction_metrics(txn_rows: list[dict], interac_rows: list[dict]) -> dict:
    txn = [{"product": r["product"], "total": r["total"], "failed": r["failed"],
            "failure_rate": pct(r["failed"], r["total"])} for r in txn_rows]
    by = {r["status"]: r["n"] for r in interac_rows}
    total = sum(by.values())
    # Interac's terminal 'completed' status is `deposited` (funds claimed/auto-deposited).
    completed = by.get("deposited", 0)
    return {"transaction_failure": txn,
            "interac": {"total": total, "completed": completed,
                        "completed_rate": pct(completed, total),
                        "expired": by.get("expired", 0),
                        "expired_rate": pct(by.get("expired", 0), total),
                        "declined": by.get("declined", 0),
                        "failed": by.get("failed", 0)}}


def engagement_metrics(recency_rows: list[dict], window_days: int, now=None) -> dict:
    now = now or dt.datetime.now(dt.timezone.utc)
    cutoff = now - dt.timedelta(days=window_days)
    active = dormant = 0
    for r in recency_rows:
        last = r.get("last_txn")
        if last is not None and last >= cutoff:
            active += 1
        else:
            dormant += 1
    total = active + dormant
    return {"window_days": window_days, "customers": total, "active": active,
            "dormant": dormant, "active_rate": pct(active, total),
            "dormant_rate": pct(dormant, total)}


def issue_summary(issue_rows: list[dict], now=None) -> dict:
    by_cat: dict[str, int] = {}
    by_sev: dict[str, int] = {}
    open_ct = resolved_ct = 0
    for r in issue_rows:
        if r["status"] != "resolved":
            open_ct += 1
            by_cat[r["category"]] = by_cat.get(r["category"], 0) + 1
            by_sev[r["severity"]] = by_sev.get(r["severity"], 0) + 1
        else:
            resolved_ct += 1
    top = max(by_cat.items(), key=lambda kv: kv[1])[0] if by_cat else None
    return {"open": open_ct, "resolved": resolved_ct, "by_category": by_cat,
            "by_severity": by_sev, "top_theme": top}


def notable_issues(issue_rows: list[dict], limit: int = 5) -> list[dict]:
    ordered = sorted(issue_rows, key=lambda r: (_SEV_RANK.get(r["severity"], 0),
                                                str(r.get("created_at", ""))), reverse=True)
    return [{"id": r["id"], "customer_id": r["customer_id"], "category": r["category"],
             "severity": r["severity"], "summary": r["summary"]} for r in ordered[:limit]]
