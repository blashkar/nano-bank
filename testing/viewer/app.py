"""nano-bank activity viewer.

The *output* side of the test harness: a live dashboard that taps the Postgres
database directly and shows what the bank is doing. Today that's customer
creation; accounts and transactions get their own tabs as those handlers land.

Reads from Postgres (not the API) on purpose — it's an observability tool that
watches the source of truth, independent of which API endpoints exist yet.

Config via env vars (defaults match nano-bank's local dev port-forward):
  DB_HOST  (default 127.0.0.1)   DB_PORT (default 5432)
  DB_NAME  (default nano_bank_db) DB_USER (default nanobank_user)
  DB_PASSWORD (default secure_nano_password_2024!)
  REFRESH_SECONDS (default 3)
"""
from __future__ import annotations

import os

import pandas as pd
import psycopg2
import streamlit as st
from streamlit_autorefresh import st_autorefresh

DB = dict(
    host=os.getenv("DB_HOST", "::1"),
    port=int(os.getenv("DB_PORT", "5432")),
    dbname=os.getenv("DB_NAME", "nano_bank_db"),
    user=os.getenv("DB_USER", "nanobank_user"),
    password=os.getenv("DB_PASSWORD", "secure_nano_password_2024!"),
)
REFRESH_SECONDS = int(os.getenv("REFRESH_SECONDS", "3"))


def query(sql: str) -> pd.DataFrame:
    """Run one query on a fresh short-lived connection (dashboard is low-traffic)."""
    conn = psycopg2.connect(connect_timeout=5, **DB)
    try:
        return pd.read_sql_query(sql, conn)
    finally:
        conn.close()


def render_customers() -> None:
    total = int(query("SELECT count(*) AS n FROM customers")["n"][0])
    last_hour = int(query(
        "SELECT count(*) AS n FROM customers "
        "WHERE created_at >= now() - interval '1 hour'")["n"][0])
    by_kyc = query(
        "SELECT kyc_status::text AS kyc_status, count(*) AS n "
        "FROM customers GROUP BY kyc_status ORDER BY n DESC")

    c1, c2, c3 = st.columns(3)
    c1.metric("Total customers", f"{total:,}")
    c2.metric("Created (last hour)", f"{last_hour:,}")
    c3.metric("KYC statuses", len(by_kyc))

    # Creation rate over the last hour, per minute.
    rate = query(
        "SELECT date_trunc('minute', created_at) AS minute, count(*) AS customers "
        "FROM customers WHERE created_at >= now() - interval '1 hour' "
        "GROUP BY 1 ORDER BY 1")
    if not rate.empty:
        st.caption("Customers created per minute (last hour)")
        st.bar_chart(rate.set_index("minute")["customers"], height=180)

    st.subheader("🧾 Recent customer registrations")
    feed = query(
        "SELECT created_at, first_name, last_name, email, phone_number, "
        "kyc_status::text AS kyc_status, customer_id "
        "FROM customers ORDER BY created_at DESC LIMIT 200")
    st.dataframe(feed, use_container_width=True, hide_index=True,
                 column_config={"created_at": st.column_config.DatetimeColumn(
                     "created", format="HH:mm:ss")})


def render_accounts() -> None:
    total = int(query("SELECT count(*) AS n FROM accounts")["n"][0])
    last_hour = int(query(
        "SELECT count(*) AS n FROM accounts "
        "WHERE created_at >= now() - interval '1 hour'")["n"][0])
    by_type = query(
        "SELECT account_type::text AS account_type, count(*) AS n, "
        "       avg(interest_rate) AS avg_rate "
        "FROM accounts GROUP BY account_type ORDER BY n DESC")

    c1, c2, c3 = st.columns(3)
    c1.metric("Total accounts", f"{total:,}")
    c2.metric("Opened (last hour)", f"{last_hour:,}")
    chequing_rate = next(
        (float(r.avg_rate) for r in by_type.itertuples() if r.account_type == "checking"),
        None)
    c3.metric("Chequing rate", f"{chequing_rate:.2%}" if chequing_rate is not None else "—")

    if not by_type.empty:
        st.caption("Accounts by type")
        st.bar_chart(by_type.set_index("account_type")["n"], height=180)

    # Opening rate over the last hour, per minute.
    rate = query(
        "SELECT date_trunc('minute', created_at) AS minute, count(*) AS accounts "
        "FROM accounts WHERE created_at >= now() - interval '1 hour' "
        "GROUP BY 1 ORDER BY 1")
    if not rate.empty:
        st.caption("Accounts opened per minute (last hour)")
        st.bar_chart(rate.set_index("minute")["accounts"], height=180)

    st.subheader("🧾 Recent accounts")
    feed = query(
        "SELECT a.created_at, a.account_number, a.account_type::text AS account_type, "
        "       a.status::text AS status, (a.interest_rate * 100) AS rate_pct, "
        "       a.balance, a.currency, "
        "       c.first_name || ' ' || c.last_name AS customer "
        "FROM accounts a JOIN customers c USING (customer_id) "
        "ORDER BY a.created_at DESC LIMIT 200")
    st.dataframe(feed, use_container_width=True, hide_index=True,
                 column_config={
                     "created_at": st.column_config.DatetimeColumn("opened", format="HH:mm:ss"),
                     "rate_pct": st.column_config.NumberColumn("rate", format="%.2f%%"),
                 })


def main() -> None:
    st.set_page_config(page_title="nano-bank viewer", page_icon="🏦", layout="wide")
    st.title("🏦 nano-bank · live activity")
    st.caption(f"Postgres `{DB['host']}:{DB['port']}/{DB['dbname']}` · "
               f"refresh every {REFRESH_SECONDS}s")
    st_autorefresh(interval=REFRESH_SECONDS * 1000, key="refresh")

    tab_customers, tab_accounts, tab_tx = st.tabs(
        ["👤 Customers", "💳 Accounts", "💸 Transactions (soon)"])
    with tab_customers:
        try:
            render_customers()
        except Exception as e:  # show the failure instead of a blank screen
            st.error(f"Database error: {e}")
            st.info("Is the nano-bank Postgres port-forward up? "
                    "`kubectl port-forward -n nano-bank svc/postgres-service 5432:5432`. "
                    "Note the port-forward binds IPv6 loopback `::1` here (the IPv4 "
                    "`0.0.0.0:5432` is a dead kind/docker-proxy mapping), so DB_HOST "
                    "defaults to `::1`.")
    with tab_accounts:
        try:
            render_accounts()
        except Exception as e:
            st.error(f"Database error: {e}")
    with tab_tx:
        st.info("Deposits / withdrawals / transfers will appear here once the "
                "transaction handlers are implemented.")


if __name__ == "__main__":
    main()
