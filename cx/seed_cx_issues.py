# cx/seed_cx_issues.py — deterministic, as-if-personal-manager cx_issues seeder.
from __future__ import annotations
import random

_CATS = ["onboarding", "declines_friction", "fees", "rail_experience", "app_ux",
         "feature_request", "other"]
_SEVS = ["low", "low", "medium", "medium", "high", "urgent"]  # weighted toward low/medium
_SUMMARIES = {
    "onboarding": "KYC took too long to clear",
    "declines_friction": "card declined at checkout despite funds",
    "fees": "surprised by the monthly fee",
    "rail_experience": "e-Transfer expired before the payee claimed it",
    "app_ux": "couldn't find the autodeposit setting",
    "feature_request": "wants recurring e-Transfers",
    "other": "general dissatisfaction with support wait time"}


def build_issue_rows(customer_ids: list[str], n: int = 40, seed: int = 7) -> list[dict]:
    rng = random.Random(seed)
    out = []
    for i in range(n):
        cat = rng.choice(_CATS)
        sev = rng.choice(_SEVS)
        out.append({"customer_id": rng.choice(customer_ids), "category": cat,
                    "severity": sev, "summary": _SUMMARIES[cat],
                    "detail": f"{_SUMMARIES[cat]} (case {i}).",
                    "created_at_offset_days": rng.randint(0, 29)})
    return out


def seed(db_params: dict, n: int = 40, seed_val: int = 7) -> int:
    import psycopg2
    conn = psycopg2.connect(**db_params)
    try:
        with conn, conn.cursor() as cur:
            cur.execute("SELECT customer_id::text FROM customers LIMIT 200")
            ids = [r[0] for r in cur.fetchall()]
            if not ids:
                raise RuntimeError("no customers to attach issues to — seed the bank first")
            # 'demo_seed' — never 'personal_manager', which is the tag the
            # production write path (agent/db.py::insert_cx_issue) stamps on
            # every genuine customer filing. Deleting by that tag would wipe
            # real complaints in any environment that has taken live filings.
            cur.execute("DELETE FROM cx_issues WHERE source = 'demo_seed'")
            rows = build_issue_rows(ids, n=n, seed=seed_val)
            for r in rows:
                cur.execute(
                    "INSERT INTO cx_issues (customer_id, category, severity, summary, detail,"
                    " source, created_at) VALUES (%s,%s,%s,%s,%s,'demo_seed',"
                    " now() - (%s || ' days')::interval)",
                    (r["customer_id"], r["category"], r["severity"], r["summary"],
                     r["detail"], r["created_at_offset_days"]))
            return len(rows)
    finally:
        conn.close()


if __name__ == "__main__":
    from .config import Settings
    print("seeded", seed(Settings.from_env().db), "cx_issues")
