from __future__ import annotations
from typing import Optional


class CxDB:
    """Read-only access to nano-bank's Postgres for CX metrics + cx_issues.

    Schema notes (verified against the live DB): customers PK is `customer_id`;
    `transactions` link to a customer via `initiated_by` (there is no account_id on
    the header); KYC-complete is `kyc_completed_at IS NOT NULL`; Interac's completed
    status is `deposited`.
    """

    def __init__(self, db_params: Optional[dict] = None):
        self._db = db_params

    def rows(self, sql: str, params: tuple = ()) -> list[dict]:
        import psycopg2
        import psycopg2.extras
        conn = psycopg2.connect(**self._db)
        try:
            conn.set_session(readonly=True, autocommit=True)
            with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as cur:
                cur.execute(sql, params)
                return [dict(r) for r in cur.fetchall()]
        finally:
            conn.close()

    def customers_onboarding(self) -> list[dict]:
        return self.rows(
            "SELECT count(*) AS total,"
            " count(*) FILTER (WHERE kyc_completed_at IS NOT NULL) AS kyc_completed,"
            " count(*) FILTER (WHERE kyc_completed_at IS NULL) AS kyc_pending"
            " FROM customers")

    def accounts_activation(self) -> list[dict]:
        return self.rows(
            "SELECT count(*) AS total,"
            " count(*) FILTER (WHERE status = 'active') AS active,"
            " count(*) FILTER (WHERE status = 'pending_activation') AS pending_activation"
            " FROM accounts")

    def product_activity(self, window_days: int) -> list[dict]:
        # distinct customers who initiated a transaction tagged with each product
        return self.rows(
            "SELECT product, count(DISTINCT initiated_by) AS customers FROM transactions"
            " WHERE product IS NOT NULL AND initiated_by IS NOT NULL"
            " AND created_at >= now() - (%s || ' days')::interval"
            " GROUP BY product", (window_days,))

    def active_customer_count(self, window_days: int) -> list[dict]:
        return self.rows(
            "SELECT count(DISTINCT initiated_by) AS active_customers FROM transactions"
            " WHERE initiated_by IS NOT NULL"
            " AND created_at >= now() - (%s || ' days')::interval", (window_days,))

    def transaction_outcomes(self, window_days: int) -> list[dict]:
        return self.rows(
            "SELECT coalesce(product,'unknown') AS product,"
            " count(*) AS total, count(*) FILTER (WHERE status = 'failed') AS failed"
            " FROM transactions"
            " WHERE created_at >= now() - (%s || ' days')::interval"
            " GROUP BY product", (window_days,))

    def interac_outcomes(self, window_days: int) -> list[dict]:
        return self.rows(
            "SELECT status::text AS status, count(*) AS n FROM interac_etransfers"
            " WHERE created_at >= now() - (%s || ' days')::interval"
            " GROUP BY status", (window_days,))

    def customer_recency(self) -> list[dict]:
        return self.rows(
            "SELECT c.customer_id AS customer_id, max(t.created_at) AS last_txn"
            " FROM customers c LEFT JOIN transactions t ON t.initiated_by = c.customer_id"
            " GROUP BY c.customer_id")

    def total_customers(self) -> int:
        return self.rows("SELECT count(*) AS n FROM customers")[0]["n"]

    def issue_rows(self) -> list[dict]:
        return self.rows(
            "SELECT id::text, customer_id::text, category::text, severity::text,"
            " summary, detail, status::text, created_at, resolved_at FROM cx_issues"
            " ORDER BY created_at DESC")

    def issue_by_id(self, issue_id: str) -> Optional[dict]:
        r = self.rows(
            "SELECT id::text, customer_id::text, category::text, severity::text,"
            " summary, detail, status::text, created_at FROM cx_issues WHERE id = %s",
            (issue_id,))
        return r[0] if r else None
