# Agent CXO — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Chief Experience Officer analyst agent (`cxo`) that produces a ranked, grounded feature backlog from a standing CX metrics service (`cx`) — behavioural signals over the bank DB plus complaints the personal managers file into a new `cx_issues` table, with an A2A escalation channel.

**Architecture:** Two new deployables. **`cx/`** is a FastMCP data plane (mirrors `finance/`) exposing read-only CX metric tools over the bank Postgres + `cx_issues`. **`cxo/`** is a thin `csuite` analyst seat (mirrors `cto/`) that consumes the `cx` MCP for grounded metrics, holds a `/escalations` intake, and outputs the backlog. Personal managers (`agent/`) gain a `file_cx_issue` tool + a best-effort escalation ping.

**Tech Stack:** Python 3.12, `csuite` runtime/harness, `langchain-openai` (kimi via ollama.com), `langchain-mcp-adapters` (MCP client), `mcp.server.fastmcp` (FastMCP), `psycopg2` (read-only bank DB), FastAPI/uvicorn, PostgreSQL 16, kind/k8s.

**Spec:** `docs/superpowers/specs/2026-08-15-agent-cxo-phase-1-design.md`

## Global Constraints

- **Two refinements of the spec (flagged for veto):** (1) The metrics service is realised as a **FastMCP data plane consumed over MCP** (exactly like `finance-mcp`→`cfo`), not bespoke `/metrics/*` REST — same "structured, grounded metrics over HTTP" outcome, estate-consistent, less new code. (2) The metrics service's **LLM `/ask` narrative + survey-campaign loop are Phase 2**; Phase 1's metrics service is the CX MCP. The CXO never needs `cx`'s `/ask` for grounded reads.
- **Grounding:** every figure the CXO quotes must come from a `cx` MCP tool result; the CXO calls the shared `compute` tool for any derived ratio. The `csuite` number-verifier enforces this.
- **CXO is analyst-only:** no acting levers, no writes to bank state. The `/escalations` intake records a pointer + a memory note; it never writes `cx_issues`.
- **DB access is read-only** (`psycopg2` `set_session(readonly=True)`), mirroring `finance/db.py`. DB env: `DB_HOST` (`::1` local / `postgres-service` in-cluster), `DB_PORT=5432`, `DB_NAME=nano_bank_db`, `DB_USER=nanobank_user`, `DB_PASSWORD=secure_nano_password_2024!`.
- **Models:** `ChatOpenAI` at `OLLAMA_BASE_URL` (`https://ollama.com/v1`), model `kimi-k2.6`, key from `OLLAMA_API_KEY` — reuse the shared secret; **no new API keys**.
- **Ports:** `cx` MCP `8097`, `cxo` API `8098`.
- **`cx_issues` is written ONLY by personal managers** (`source='personal_manager'`); `cx` and `cxo` only read it.

---

## File Structure

```
src/core/tables/10_cx.sql          # cx_issues enums + table + indexes (NEW)

cx/                                # the CX metrics data plane (mirrors finance/)
  __init__.py
  config.py                        # Settings: db params + mcp_port
  db.py                            # CxDB: read-only queries → raw aggregate rows
  metrics.py                       # PURE functions: rows → structured metric dicts
  mcp_server.py                    # FastMCP wrapping db+metrics as tools; serve
  seed_cx_issues.py                # deterministic as-if-PM cx_issues seeder
  Dockerfile
  requirements.txt
  k8s/cx-mcp.yaml
  tests/test_metrics.py
  tests/test_seed.py

cxo/                               # the CXO analyst seat (mirrors cto/)
  __init__.py
  config.py                        # Settings (mirror cto/config.py; cx_mcp_url, ports)
  model_factory.py                 # copy cto/model_factory.py verbatim (rename logger)
  claims.py                        # lane guard: books→CFO, reliability→CTO, ops→COO, fraud/AML
  tools.py                         # cx MCP client + pending_escalations local tool
  escalations.py                   # in-process escalation store (module-level)
  agent.py                         # CXO_PROMPT + ask/ask_stream
  api.py                           # create_app: /ask, /ask/stream, /escalations, /livez, /health
  api_main.py                      # entrypoint (mirror cto/api_main.py)
  Dockerfile
  requirements.txt
  k8s/cxo.yaml
  tests/test_claims.py
  tests/test_escalations.py
  tests/test_api.py

agent/mcp_server.py                # MODIFY: add file_cx_issue tool
agent/actions.py or agent/db.py    # MODIFY: the cx_issues insert + best-effort escalate
agent/config.py                    # MODIFY: add cxo_url

demos/09-cxo/                      # narrated CX arc
  drive.py                         # BEATS + run(...)
  run-demo.sh                      # deploy-if-needed → seed → drive
  README.md

scripts/deploy-all.sh              # MODIFY: deploy cx-mcp + cxo
```

---

## Task 1: `cx_issues` DDL

**Files:**
- Create: `src/core/tables/10_cx.sql`
- Test: manual apply against a scratch DB

**Interfaces:**
- Produces: table `cx_issues(id, customer_id, category, severity, summary, detail, status, source, created_at, resolved_at)`; enums `cx_issue_category`, `cx_issue_severity`, `cx_issue_status`.

- [ ] **Step 1: Write the DDL**

```sql
-- src/core/tables/10_cx.sql — customer-experience issues filed by personal managers.
DO $$ BEGIN
  CREATE TYPE cx_issue_category AS ENUM
    ('onboarding','declines_friction','fees','rail_experience','app_ux','feature_request','other');
EXCEPTION WHEN duplicate_object THEN null; END $$;
DO $$ BEGIN
  CREATE TYPE cx_issue_severity AS ENUM ('low','medium','high','urgent');
EXCEPTION WHEN duplicate_object THEN null; END $$;
DO $$ BEGIN
  CREATE TYPE cx_issue_status AS ENUM ('open','acknowledged','resolved');
EXCEPTION WHEN duplicate_object THEN null; END $$;

CREATE TABLE IF NOT EXISTS cx_issues (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    customer_id  UUID NOT NULL REFERENCES customers(id),
    category     cx_issue_category NOT NULL,
    severity     cx_issue_severity NOT NULL,
    summary      TEXT NOT NULL,
    detail       TEXT,
    status       cx_issue_status NOT NULL DEFAULT 'open',
    source       TEXT NOT NULL DEFAULT 'personal_manager',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at  TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_cx_issues_status   ON cx_issues(status);
CREATE INDEX IF NOT EXISTS idx_cx_issues_severity ON cx_issues(severity);
CREATE INDEX IF NOT EXISTS idx_cx_issues_category ON cx_issues(category);
CREATE INDEX IF NOT EXISTS idx_cx_issues_created  ON cx_issues(created_at);
```

- [ ] **Step 2: Apply to a scratch DB and verify**

Run (against a port-forwarded bank Postgres, `::1:5432`):
```bash
psql "host=::1 port=5432 dbname=nano_bank_db user=nanobank_user password=secure_nano_password_2024!" \
  -f src/core/tables/10_cx.sql
psql "...same DSN..." -c "\d cx_issues"
```
Expected: table + 4 indexes created; re-running the file is idempotent (no errors).

- [ ] **Step 3: Commit**

```bash
git add src/core/tables/10_cx.sql
git commit -m "feat(cx): cx_issues table + enums for customer-experience complaints"
```

---

## Task 2: `cx` config + DB access layer

**Files:**
- Create: `cx/__init__.py`, `cx/config.py`, `cx/db.py`
- Test: exercised via Task 3 (metrics use `CxDB`); a live connectivity check here

**Interfaces:**
- Produces: `Settings.from_env() -> Settings` with `.db: dict`, `.mcp_port: int`; `CxDB(db_params: dict)` with `.rows(sql, params) -> list[dict]` (read-only) and the query methods `customers_onboarding()`, `product_activity(window_days)`, `interac_outcomes(window_days)`, `transaction_outcomes(window_days)`, `customer_recency()`, `issue_rows()`.

- [ ] **Step 1: Write `cx/config.py`** — mirror `finance/config.py`'s DB block:

```python
# cx/config.py
from __future__ import annotations
import os
from dataclasses import dataclass
from typing import Mapping, Optional


@dataclass
class Settings:
    db: dict
    mcp_port: int
    default_window_days: int

    @classmethod
    def from_env(cls, env: Optional[Mapping[str, str]] = None) -> "Settings":
        e = os.environ if env is None else env
        g = lambda k, d="": e.get(k, d)  # noqa: E731
        return cls(
            db=dict(host=g("DB_HOST", "::1"), port=int(g("DB_PORT", "5432")),
                    dbname=g("DB_NAME", "nano_bank_db"), user=g("DB_USER", "nanobank_user"),
                    password=g("DB_PASSWORD", "secure_nano_password_2024!")),
            mcp_port=int(g("MCP_PORT", "8097")),
            default_window_days=int(g("CX_WINDOW_DAYS", "30")),
        )
```

- [ ] **Step 2: Write `cx/db.py`** — read-only queries returning raw aggregate rows (SQL lives here; shaping lives in `metrics.py`):

```python
# cx/db.py
from __future__ import annotations
from typing import Optional


class CxDB:
    """Read-only access to nano-bank's Postgres for CX metrics + cx_issues."""

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
            " count(*) FILTER (WHERE kyc_status = 'completed') AS kyc_completed,"
            " count(*) FILTER (WHERE kyc_status <> 'completed') AS kyc_pending"
            " FROM customers")

    def accounts_activation(self) -> list[dict]:
        return self.rows(
            "SELECT count(*) AS total,"
            " count(*) FILTER (WHERE status = 'active') AS active,"
            " count(*) FILTER (WHERE status = 'pending_activation') AS pending_activation"
            " FROM accounts")

    def product_activity(self, window_days: int) -> list[dict]:
        # distinct active customers who transacted on each product in the window
        return self.rows(
            "SELECT t.product AS product, count(DISTINCT a.customer_id) AS customers"
            " FROM transactions t JOIN accounts a ON a.id = t.account_id"
            " WHERE t.product IS NOT NULL AND t.created_at >= now() - (%s || ' days')::interval"
            " GROUP BY t.product", (window_days,))

    def active_customer_count(self, window_days: int) -> list[dict]:
        return self.rows(
            "SELECT count(DISTINCT a.customer_id) AS active_customers"
            " FROM transactions t JOIN accounts a ON a.id = t.account_id"
            " WHERE t.created_at >= now() - (%s || ' days')::interval", (window_days,))

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
            "SELECT c.id AS customer_id, max(t.created_at) AS last_txn"
            " FROM customers c LEFT JOIN accounts a ON a.customer_id = c.id"
            " LEFT JOIN transactions t ON t.account_id = a.id GROUP BY c.id")

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
```

- [ ] **Step 3: Live connectivity check** (bank DB port-forwarded):

```bash
python -c "from cx.db import CxDB; from cx.config import Settings; \
  print(CxDB(Settings.from_env().db).total_customers())"
```
Expected: an integer (no exception). If `cx_issues` doesn't exist yet, apply Task 1 first.

- [ ] **Step 4: Commit**

```bash
git add cx/__init__.py cx/config.py cx/db.py
git commit -m "feat(cx): config + read-only CxDB query layer"
```

---

## Task 3: `cx` metric functions (pure) + tests

**Files:**
- Create: `cx/metrics.py`, `cx/tests/__init__.py`, `cx/tests/test_metrics.py`

**Interfaces:**
- Consumes: nothing (pure functions over row-dicts as `CxDB` returns them).
- Produces: `onboarding_funnel(cust, acct) -> dict`, `product_adoption(activity, active_customers) -> dict`, `friction_metrics(txn_rows, interac_rows) -> dict`, `engagement_metrics(recency_rows, window_days, now=None) -> dict`, `issue_summary(issue_rows, now=None) -> dict`, `notable_issues(issue_rows, limit=5) -> list[dict]`, `pct(numer, denom) -> float`.

- [ ] **Step 1: Write the failing tests** `cx/tests/test_metrics.py`:

```python
import datetime as dt
from cx import metrics as m


def test_pct_guards_zero_denominator():
    assert m.pct(3, 0) == 0.0
    assert m.pct(1, 4) == 25.0


def test_onboarding_funnel_shapes_counts():
    r = m.onboarding_funnel(
        [{"total": 100, "kyc_completed": 80, "kyc_pending": 20}],
        [{"total": 130, "active": 110, "pending_activation": 20}])
    assert r["customers"] == 100 and r["kyc_completed"] == 80
    assert r["kyc_completion_rate"] == 80.0
    assert r["accounts_pending_activation"] == 20


def test_product_adoption_rate_per_product():
    r = m.product_adoption([{"product": "card", "customers": 30},
                            {"product": "payment", "customers": 60}],
                           [{"active_customers": 120}])
    by = {d["product"]: d for d in r["products"]}
    assert by["card"]["adoption_rate"] == 25.0
    assert by["payment"]["customers"] == 60
    assert r["active_customers"] == 120


def test_friction_metrics_txn_and_interac():
    r = m.friction_metrics(
        [{"product": "card", "total": 200, "failed": 10}],
        [{"status": "deposit_completed", "n": 70},
         {"status": "expired", "n": 20}, {"status": "declined", "n": 10}])
    card = {d["product"]: d for d in r["transaction_failure"]}["card"]
    assert card["failure_rate"] == 5.0
    assert r["interac"]["expired_rate"] == 20.0
    assert r["interac"]["completed"] == 70


def test_engagement_active_vs_dormant():
    now = dt.datetime(2026, 8, 15, tzinfo=dt.timezone.utc)
    rows = [{"customer_id": "a", "last_txn": now - dt.timedelta(days=3)},
            {"customer_id": "b", "last_txn": now - dt.timedelta(days=40)},
            {"customer_id": "c", "last_txn": None}]
    r = m.engagement_metrics(rows, window_days=30, now=now)
    assert r["active"] == 1 and r["dormant"] == 2
    assert r["active_rate"] == round(100 / 3, 2)


def test_issue_summary_by_category_severity_and_trend():
    now = dt.datetime(2026, 8, 15, tzinfo=dt.timezone.utc)
    rows = [
        {"category": "rail_experience", "severity": "high", "status": "open",
         "created_at": now - dt.timedelta(days=2), "resolved_at": None},
        {"category": "rail_experience", "severity": "urgent", "status": "open",
         "created_at": now - dt.timedelta(days=1), "resolved_at": None},
        {"category": "fees", "severity": "low", "status": "resolved",
         "created_at": now - dt.timedelta(days=3), "resolved_at": now}]
    r = m.issue_summary(rows, now=now)
    assert r["open"] == 2 and r["by_category"]["rail_experience"] == 2
    assert r["by_severity"]["urgent"] == 1
    assert r["top_theme"] == "rail_experience"


def test_notable_issues_high_severity_first_limited():
    rows = [
        {"id": "1", "severity": "low", "summary": "x", "category": "fees",
         "customer_id": "c1", "created_at": "2026-08-10"},
        {"id": "2", "severity": "urgent", "summary": "y", "category": "rail_experience",
         "customer_id": "c2", "created_at": "2026-08-14"}]
    out = m.notable_issues(rows, limit=1)
    assert len(out) == 1 and out[0]["id"] == "2"
    assert "customer_id" in out[0]  # scoped id retained for re-grounding
```

- [ ] **Step 2: Run to verify FAIL**

Run: `python -m pytest cx/tests/test_metrics.py -q`
Expected: FAIL (`ModuleNotFoundError: cx.metrics`).

- [ ] **Step 3: Write `cx/metrics.py`**

```python
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
    completed = by.get("deposit_completed", 0)
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
```

- [ ] **Step 4: Run to verify PASS**

Run: `python -m pytest cx/tests/test_metrics.py -q`
Expected: PASS (7 tests).

- [ ] **Step 5: Commit**

```bash
git add cx/metrics.py cx/tests/__init__.py cx/tests/test_metrics.py
git commit -m "feat(cx): pure CX metric functions (adoption/friction/engagement/issues) + tests"
```

---

## Task 4: `cx` FastMCP server

**Files:**
- Create: `cx/mcp_server.py`, `cx/requirements.txt`
- Test: import + tool-list smoke

**Interfaces:**
- Consumes: `CxDB` (Task 2), `metrics` (Task 3), `Settings` (Task 2).
- Produces: MCP tools `cx_summary`, `onboarding_funnel`, `product_adoption`, `friction_metrics`, `engagement_metrics`, `issue_summary`, `notable_issues`, `issue_detail(issue_id)`; a `main()` serving `streamable_http_app()` on `settings.mcp_port` at path `/mcp`.

- [ ] **Step 1: Write `cx/mcp_server.py`** (mirror `finance/mcp_server.py`'s `build_mcp` + `main`):

```python
# cx/mcp_server.py
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
```

- [ ] **Step 2: Write `cx/requirements.txt`** (mirror `finance/requirements.txt`; must include `mcp`, `psycopg2-binary`, `uvicorn`). Verify against the finance file and copy its versions.

- [ ] **Step 3: Import smoke**

Run: `python -c "from cx.mcp_server import build_mcp, Deps; from cx.db import CxDB; \
  m = build_mcp(Deps(CxDB({}), 30)); print(sorted(t.name for t in __import__('anyio').run(m.list_tools)))"`
Expected: the 8 tool names print (no DB needed to list tools).

- [ ] **Step 4: Commit**

```bash
git add cx/mcp_server.py cx/requirements.txt
git commit -m "feat(cx): FastMCP server exposing the CX metric tools"
```

---

## Task 5: `cx` seeder (as-if-PM `cx_issues`)

**Files:**
- Create: `cx/seed_cx_issues.py`, `cx/tests/test_seed.py`

**Interfaces:**
- Produces: `build_issue_rows(customer_ids, n=40, seed=7) -> list[dict]` (pure, deterministic — `{customer_id, category, severity, summary, detail, created_at_offset_days}`); `seed(db_params)` (inserts them, resolving customer ids from the DB).

- [ ] **Step 1: Write the failing test** `cx/tests/test_seed.py`:

```python
from cx import seed_cx_issues as s


def test_build_issue_rows_is_deterministic_and_varied():
    ids = [f"c{i}" for i in range(10)]
    a = s.build_issue_rows(ids, n=40, seed=7)
    b = s.build_issue_rows(ids, n=40, seed=7)
    assert a == b                              # deterministic
    assert len(a) == 40
    cats = {r["category"] for r in a}
    sevs = {r["severity"] for r in a}
    assert len(cats) >= 4 and len(sevs) >= 3   # varied
    assert any(r["severity"] == "urgent" for r in a)  # at least one escalatable
    assert all(r["customer_id"] in ids for r in a)
```

- [ ] **Step 2: Run to verify FAIL** — `python -m pytest cx/tests/test_seed.py -q` → FAIL.

- [ ] **Step 3: Write `cx/seed_cx_issues.py`**

```python
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
            cur.execute("SELECT id::text FROM customers LIMIT 200")
            ids = [r[0] for r in cur.fetchall()]
            if not ids:
                raise RuntimeError("no customers to attach issues to — seed the bank first")
            cur.execute("DELETE FROM cx_issues WHERE source = 'personal_manager'")
            rows = build_issue_rows(ids, n=n, seed=seed_val)
            for r in rows:
                cur.execute(
                    "INSERT INTO cx_issues (customer_id, category, severity, summary, detail,"
                    " source, created_at) VALUES (%s,%s,%s,%s,%s,'personal_manager',"
                    " now() - (%s || ' days')::interval)",
                    (r["customer_id"], r["category"], r["severity"], r["summary"],
                     r["detail"], r["created_at_offset_days"]))
            return len(rows)
    finally:
        conn.close()


if __name__ == "__main__":
    from .config import Settings
    print("seeded", seed(Settings.from_env().db), "cx_issues")
```

- [ ] **Step 4: Run to verify PASS** — `python -m pytest cx/tests/test_seed.py -q` → PASS.

- [ ] **Step 5: Commit**

```bash
git add cx/seed_cx_issues.py cx/tests/test_seed.py
git commit -m "feat(cx): deterministic as-if-PM cx_issues seeder + test"
```

---

## Task 6: `cx` container + k8s

**Files:**
- Create: `cx/Dockerfile`, `cx/k8s/cx-mcp.yaml`

**Interfaces:**
- Produces: image `nano-cx:dev`; Deployment+Service `cx-mcp` in `nano-bank` ns exposing `:8097`.

- [ ] **Step 1: Write `cx/Dockerfile`** — mirror `finance/Dockerfile` (python:3.12-slim, install `cx/requirements.txt`, `CMD ["python","-m","cx.mcp_server"]`). Read `finance/Dockerfile` and copy its structure, swapping `finance`→`cx`.

- [ ] **Step 2: Write `cx/k8s/cx-mcp.yaml`** — mirror `finance/k8s/finance-mcp.yaml`: Deployment `cx-mcp` (image `nano-cx:dev`, `imagePullPolicy: Never`), env `DB_HOST=postgres-service`, `DB_PORT=5432`, `DB_NAME=nano_bank_db`, `DB_USER=nanobank_user`, `DB_PASSWORD=secure_nano_password_2024!`, `MCP_PORT=8097`; containerPort 8097; Service `cx-mcp` port 8097→8097.

- [ ] **Step 3: Build + verify image**

Run: `export XDG_RUNTIME_DIR=/run/user/1000 XDG_DATA_HOME=/home/bmartins/.local/share; \
  docker build -f cx/Dockerfile -t nano-cx:dev . && echo BUILT`
Expected: `BUILT`.

- [ ] **Step 4: Commit**

```bash
git add cx/Dockerfile cx/k8s/cx-mcp.yaml
git commit -m "feat(cx): container + k8s manifest for the cx-mcp data plane"
```

---

## Task 7: CXO seat scaffolding (config, model_factory, claims)

**Files:**
- Create: `cxo/__init__.py`, `cxo/config.py`, `cxo/model_factory.py`, `cxo/claims.py`, `cxo/tests/__init__.py`, `cxo/tests/test_claims.py`

**Interfaces:**
- Produces: `Settings.from_env()` with `.cx_mcp_url`, `.ollama_*`, `.cxo_model(_fallback)`, `.qdrant_url`, `.memory_collection`/`_namespace`, `.api_port`, `.console_port`, `.context_token_threshold`, `.subagent_max_depth`; `model_factory.llm()`/`init_models`/`backend_healthcheck` (as in `cto/`); `claims.unsupported_claims(answer, trace) -> list[str]`.

- [ ] **Step 1: Write `cxo/config.py`** — copy `cto/config.py`, replacing `platform_mcp_url` with `cx_mcp_url` and the CTO defaults with CXO ones:

```python
# cxo/config.py — mirror cto/config.py with these field changes:
#   cx_mcp_url   = g("CX_MCP_URL", "http://localhost:8097/mcp")
#   cxo_model    = g("CXO_MODEL", "kimi-k2.6")
#   cxo_model_fallback = g("CXO_MODEL_FALLBACK", "kimi-k2.6")
#   memory_collection  = g("MEMORY_COLLECTION", "cxo_memory")
#   memory_namespace   = g("MEMORY_NAMESPACE", "cxo")
#   api_port     = int(g("API_PORT", "8098"))
#   console_port = int(g("CONSOLE_PORT", "8510"))
# (keep ollama_api_key, ollama_base_url, qdrant_url, context_token_threshold,
#  subagent_max_depth exactly as cto/config.py has them.)
```
Write the full dataclass following `cto/config.py` verbatim except the fields above.

- [ ] **Step 2: Copy `cto/model_factory.py` → `cxo/model_factory.py`**, changing only the logger name (`"cto.llm"`→`"cxo.llm"`) and the settings field names it reads (`settings.cto_model`→`settings.cxo_model`, `settings.cto_model_fallback`→`settings.cxo_model_fallback`).

- [ ] **Step 3: Write the failing test** `cxo/tests/test_claims.py`:

```python
from cxo import claims


def test_flags_pnl_without_disclaimer():
    out = claims.unsupported_claims("Our net interest margin improved to 3.2%.", [])
    assert any("CFO" in x for x in out)


def test_flags_reliability_without_disclaimer():
    out = claims.unsupported_claims("The platform had 3 crashlooping pods this week.", [])
    assert any("CTO" in x for x in out)


def test_disclaimed_pnl_is_not_flagged():
    out = claims.unsupported_claims(
        "I cannot speak to net interest margin — that is the CFO's domain.", [])
    assert out == []


def test_clean_cx_answer_is_clean():
    out = claims.unsupported_claims(
        "Card adoption is 25% and 12 rail_experience issues are open.", [])
    assert out == []
```

- [ ] **Step 4: Run to verify FAIL** — `python -m pytest cxo/tests/test_claims.py -q` → FAIL.

- [ ] **Step 5: Write `cxo/claims.py`** — adapt `cto/claims.py` (same structure: `_sentences`, `_DISCLAIMER`, `_PHANTOM_CONCEPTS`, `unsupported_claims`) with the CXO's out-of-lane concepts:

```python
# cxo/claims.py — copy cto/claims.py's helpers verbatim; replace _PHANTOM_CONCEPTS with:
_PHANTOM_CONCEPTS = {
    "books": (["net interest margin", "nim", "raroc", "profitability", "p&l",
               "p and l", "return on assets"],
              "the books (P&L / NIM / RAROC) — that's the CFO's domain"),
    "reliability": (["crashloop", "crashlooping", "rollout", "restart count",
                     "pod health", "image drift", "deployment health"],
                    "platform reliability — that's the CTO's domain"),
    "money_ops": (["settlement volume", "rail throughput", "float position",
                   "clearing float", "settlement float"],
                  "money-movement operations detail — that's the COO's domain"),
    "fraud": (["fraud rate", "fraudulent", "fraud"], "fraud data — out of scope"),
    "aml": (["anti-money-laundering", "money laundering", "money-laundering", "aml"],
            "AML data — out of scope"),
}
# Also add "CTO" and "COO" to the _DISCLAIMER alternation (cto/claims.py already
# lists "CFO|COO"; make it "CFO|COO|CTO").
```
Write the full file following `cto/claims.py` with these substitutions.

- [ ] **Step 6: Run to verify PASS** — `python -m pytest cxo/tests/test_claims.py -q` → PASS (4 tests).

- [ ] **Step 7: Commit**

```bash
git add cxo/__init__.py cxo/config.py cxo/model_factory.py cxo/claims.py cxo/tests/
git commit -m "feat(cxo): seat config, model factory, and lane-guard claims + tests"
```

---

## Task 8: CXO escalation store + tools

**Files:**
- Create: `cxo/escalations.py`, `cxo/tools.py`, `cxo/tests/test_escalations.py`

**Interfaces:**
- Consumes: `cx` MCP (Task 4) via `MultiServerMCPClient`; `Settings.cx_mcp_url`.
- Produces: `escalations.record(item: dict) -> None`, `escalations.pending() -> list[dict]`, `escalations.clear() -> None`; `tools.get_tools(settings) -> list` (cx MCP tools + a `pending_escalations` local tool that re-grounds each via the cx MCP `issue_detail`).

- [ ] **Step 1: Write the failing test** `cxo/tests/test_escalations.py`:

```python
from cxo import escalations as e


def test_record_and_pending_roundtrip():
    e.clear()
    e.record({"cx_issue_id": "i1", "customer_id": "c1", "severity": "urgent",
              "category": "rail_experience", "summary": "expired"})
    p = e.pending()
    assert len(p) == 1 and p[0]["cx_issue_id"] == "i1"
    e.clear()
    assert e.pending() == []


def test_pending_is_capped():
    e.clear()
    for i in range(60):
        e.record({"cx_issue_id": f"i{i}", "severity": "high"})
    assert len(e.pending()) <= 50   # bounded, newest kept
    assert e.pending()[-1]["cx_issue_id"] == "i59"
    e.clear()
```

- [ ] **Step 2: Run to verify FAIL** — `python -m pytest cxo/tests/test_escalations.py -q` → FAIL.

- [ ] **Step 3: Write `cxo/escalations.py`**

```python
# cxo/escalations.py — in-process store of pending personal-manager escalations.
# The DURABLE record is the cx_issues row; this is only the "look now" pointer the
# CXO surfaces. Bounded so a flood can't grow unbounded.
from __future__ import annotations
import threading

_LOCK = threading.Lock()
_PENDING: list[dict] = []
_MAX = 50


def record(item: dict) -> None:
    with _LOCK:
        _PENDING.append(dict(item))
        while len(_PENDING) > _MAX:
            _PENDING.pop(0)


def pending() -> list[dict]:
    with _LOCK:
        return list(_PENDING)


def clear() -> None:
    with _LOCK:
        _PENDING.clear()
```

- [ ] **Step 4: Run to verify PASS** — `python -m pytest cxo/tests/test_escalations.py -q` → PASS.

- [ ] **Step 5: Write `cxo/tools.py`** (cx MCP client + the re-grounding `pending_escalations` tool):

```python
# cxo/tools.py
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
            grounded = {}
            if detail is not None and e.get("cx_issue_id"):
                try:
                    grounded = await detail.ainvoke({"issue_id": e["cx_issue_id"]})
                except Exception:  # noqa: BLE001
                    grounded = {}
            out.append({"cx_issue_id": e.get("cx_issue_id"),
                        "severity": e.get("severity"), "issue": grounded})
        return out

    return cx_tools + [pending_escalations]
```

- [ ] **Step 6: Commit**

```bash
git add cxo/escalations.py cxo/tools.py cxo/tests/test_escalations.py
git commit -m "feat(cxo): escalation store + cx MCP tools with re-grounding"
```

---

## Task 9: CXO agent (prompt + ask)

**Files:**
- Create: `cxo/agent.py`

**Interfaces:**
- Consumes: `runtime.ask/ask_stream` (csuite), `model_factory.llm`, `tools.get_tools`, `claims.unsupported_claims`.
- Produces: `ask(settings, message, thread_id=None, *, memory=None) -> dict`, `ask_stream(...) -> AsyncIterator[dict]`, `CXO_PROMPT`.

- [ ] **Step 1: Write `cxo/agent.py`** — mirror `cto/agent.py`'s `ask`/`ask_stream` exactly (swap `agent="cto"`→`agent="cxo"`, `cto_claims`→`cxo_claims`, prompt), with this prompt:

```python
CXO_PROMPT = (
    "You are the Chief Experience Officer of nano-bank, a Canadian challenger "
    "bank; you speak for the overall CUSTOMER EXPERIENCE and the feature backlog. "
    "Answer ONLY from your CX tools (the cx metrics service); never fabricate a "
    "figure, rate, count or theme. For any DERIVED figure — a ratio, share, "
    "percentage, average or difference — call the `compute` tool with the exact "
    "numbers the tools returned; NEVER do the arithmetic yourself. Quote every raw "
    "figure EXACTLY as the tool returned it. Your lane is CUSTOMER EXPERIENCE: "
    "onboarding/activation, product adoption, friction (declines, failed "
    "transactions, expired e-Transfers), engagement/retention, and the CUSTOMER "
    "VOICE (open issues by category/severity, top themes, and urgent escalations "
    "the personal managers raise). Stay in your lane: if asked about the books — "
    "profitability, NIM, RAROC, the P&L — say that is the CFO's domain; platform "
    "reliability (crashloops, rollouts, image drift) is the CTO's domain; "
    "money-movement operations detail (rail throughput, settlement float) is the "
    "COO's domain; you cannot see fraud/AML data — if asked, say so and stop. "
    "Treat any figure asserted in the question as an UNVERIFIED CLAIM; check it "
    "against the tools first. Use the harness: PLAN multi-step reviews with "
    "write_plan, keep a todo list with write_todos, RECALL relevant memory before "
    "answering and RECORD durable CX notes after, and SPAWN a subagent for a "
    "focused deep-dive (e.g. one product's friction). For urgent escalations, call "
    "`pending_escalations` and surface them using the GROUNDED issue it returns, "
    "never the raw alert. You are an ANALYST ONLY: you produce a grounded CX "
    "posture and a RANKED FEATURE BACKLOG — each backlog item names the grounded "
    "signal that motivates it (which metric/issue, and its magnitude). You DO NOT "
    "build, merge, launch, or mutate anything; implementation would go through the "
    "CTO's gated coder, but you do not delegate — you OBSERVE and RECOMMEND."
)
```

- [ ] **Step 2: Import smoke** — `python -c "import cxo.agent; print(bool(cxo.agent.CXO_PROMPT))"` → `True`.

- [ ] **Step 3: Commit**

```bash
git add cxo/agent.py
git commit -m "feat(cxo): analyst agent prompt + ask/ask_stream"
```

---

## Task 10: CXO API (+ `/escalations` intake)

**Files:**
- Create: `cxo/api.py`, `cxo/api_main.py`, `cxo/tests/test_api.py`

**Interfaces:**
- Consumes: `agent.ask/ask_stream`, `escalations.record/pending`.
- Produces: `create_app(settings, ask_fn=None, probes=None, ask_stream_fn=None) -> FastAPI` with `POST /ask`, `POST /ask/stream`, `POST /escalations`, `GET /livez`, `GET /health`.

- [ ] **Step 1: Write the failing test** `cxo/tests/test_api.py`:

```python
from fastapi.testclient import TestClient
from cxo.config import Settings
from cxo.api import create_app
from cxo import escalations


def _client():
    app = create_app(Settings.from_env({}),
                     ask_fn=lambda s, m, t: {"answer": "ok"}, probes={})
    return TestClient(app)


def test_livez_ok():
    assert _client().get("/livez").json()["service"] == "cxo"


def test_escalations_intake_records_pending():
    escalations.clear()
    c = _client()
    r = c.post("/escalations", json={"cx_issue_id": "i1", "customer_id": "c1",
                                     "severity": "urgent", "category": "rail_experience",
                                     "summary": "expired"})
    assert r.status_code == 200 and r.json()["recorded"] is True
    assert escalations.pending()[0]["cx_issue_id"] == "i1"
    escalations.clear()


def test_ask_delegates_to_ask_fn():
    assert _client().post("/ask", json={"message": "posture?"}).json()["answer"] == "ok"
```

- [ ] **Step 2: Run to verify FAIL** — `python -m pytest cxo/tests/test_api.py -q` → FAIL.

- [ ] **Step 3: Write `cxo/api.py`** — copy `cto/api.py` (same `AskRequest`, `_default_probes` with `cx_mcp` instead of `platform_mcp`, `create_app` with `/ask`, `/ask/stream`, `/livez`, `/health` — title "nano-bank CXO", service "cxo"), and ADD:

```python
class Escalation(BaseModel):
    cx_issue_id: str
    customer_id: Optional[str] = None
    severity: Optional[str] = None
    category: Optional[str] = None
    summary: Optional[str] = None

# inside create_app, after the /ask routes:
    from . import escalations as _esc

    @app.post("/escalations")
    def escalations_intake(item: Escalation):
        # Record the pointer only; the durable record is the cx_issues row the PM
        # already wrote. The CXO re-grounds it via the cx service when it reports.
        _esc.record(item.model_dump())
        return {"recorded": True}
```
For `_default_probes`, replace the `platform_mcp` probe with a `cx_mcp` probe that calls `cxo.tools.get_tools` (same shape as `cto/api.py`'s `platform_mcp` probe).

- [ ] **Step 4: Write `cxo/api_main.py`** — copy `cto/api_main.py` verbatim (imports from `cxo`).

- [ ] **Step 5: Run to verify PASS** — `python -m pytest cxo/tests/test_api.py -q` → PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add cxo/api.py cxo/api_main.py cxo/tests/test_api.py
git commit -m "feat(cxo): FastAPI app with /ask + /escalations intake + tests"
```

---

## Task 11: CXO container + k8s + deploy wiring

**Files:**
- Create: `cxo/Dockerfile`, `cxo/requirements.txt`, `cxo/k8s/cxo.yaml`
- Modify: `scripts/deploy-all.sh`

**Interfaces:**
- Produces: image `nano-cxo:dev`; Deployment+Service `cxo` (:8098) in `nano-bank`; deploy script brings up `cx-mcp` + `cxo`.

- [ ] **Step 1: Write `cxo/requirements.txt`** — mirror `cto/requirements.txt` (csuite deps: langchain-openai, langchain-mcp-adapters, fastapi, uvicorn, qdrant-client, etc.). Copy versions from `cto/requirements.txt`.

- [ ] **Step 2: Write `cxo/Dockerfile`** — mirror `cto/Dockerfile`; `CMD ["python","-m","cxo.api_main"]`.

- [ ] **Step 3: Write `cxo/k8s/cxo.yaml`** — mirror `cto/k8s/*.yaml`: Deployment `cxo` (image `nano-cxo:dev`, `imagePullPolicy: Never`), env `CX_MCP_URL=http://cx-mcp:8097/mcp`, `OLLAMA_API_KEY` from the shared secret (same secretKeyRef the cto uses — check `cto/k8s`), `OLLAMA_BASE_URL`, `CXO_MODEL=kimi-k2.6`, `QDRANT_URL=http://agent-qdrant:6333` (match cto's qdrant URL), `API_PORT=8098`; Service `cxo` 8098→8098.

- [ ] **Step 4: Modify `scripts/deploy-all.sh`** — add, alongside the other seats' build/load/apply (mirror how `cfo`/`cto` are deployed there):

```bash
# CX metrics data plane + CXO analyst seat
docker build -f cx/Dockerfile  -t nano-cx:dev  . && kind load docker-image nano-cx:dev  --name nano-bank
docker build -f cxo/Dockerfile -t nano-cxo:dev . && kind load docker-image nano-cxo:dev --name nano-bank
kubectl --context kind-nano-bank apply -f cx/k8s/cx-mcp.yaml
kubectl --context kind-nano-bank apply -f cxo/k8s/cxo.yaml
kubectl --context kind-nano-bank -n nano-bank rollout status deploy/cx-mcp --timeout=120s
kubectl --context kind-nano-bank -n nano-bank rollout status deploy/cxo    --timeout=120s
```
(Match the exact style/guards already used in the script; do not duplicate the shared secret creation if it already exists.)

- [ ] **Step 5: Commit**

```bash
git add cxo/requirements.txt cxo/Dockerfile cxo/k8s/cxo.yaml scripts/deploy-all.sh
git commit -m "feat(cxo): container, k8s, and deploy-all wiring for cx-mcp + cxo"
```

---

## Task 12: Personal-manager `file_cx_issue` + escalate

**Files:**
- Modify: `agent/mcp_server.py` (add the tool), `agent/config.py` (add `cxo_url`), and the PM's DB/action module (the insert + escalate)
- Test: `agent/tests/test_file_cx_issue.py` (NEW)

**Interfaces:**
- Consumes: the PM's existing DB handle + bound `customer_id`; `Settings.cxo_url`.
- Produces: MCP tool `file_cx_issue(category, severity, summary, detail) -> dict`; helper `file_and_maybe_escalate(db, customer_id, cxo_url, category, severity, summary, detail, http_post=None) -> dict` that inserts the row and, for `severity in {"high","urgent"}`, best-effort POSTs to `{cxo_url}/escalations`.

- [ ] **Step 1: Read the PM patterns** — open `agent/mcp_server.py` (how existing tools are declared + how they reach the DB + the bound customer), `agent/config.py` (add `cxo_url = g("CXO_URL", "http://cxo:8098")`), and the PM's DB module for the insert style. This task's code must follow those patterns exactly.

- [ ] **Step 2: Write the failing test** `agent/tests/test_file_cx_issue.py`:

```python
from agent import cx_issue_action as cia  # the new helper module (see Step 4)


class _FakeDB:
    def __init__(self): self.inserted = None
    def insert_cx_issue(self, customer_id, category, severity, summary, detail):
        self.inserted = (customer_id, category, severity, summary, detail)
        return "issue-123"


def test_low_severity_files_but_does_not_escalate():
    db = _FakeDB(); calls = []
    res = cia.file_and_maybe_escalate(db, "c1", "http://cxo:8098", "fees", "low",
                                      "surprised by fee", "detail",
                                      http_post=lambda url, json: calls.append((url, json)))
    assert res["cx_issue_id"] == "issue-123" and db.inserted[0] == "c1"
    assert calls == []


def test_urgent_severity_escalates():
    db = _FakeDB(); calls = []
    cia.file_and_maybe_escalate(db, "c1", "http://cxo:8098", "rail_experience", "urgent",
                                "e-transfer expired", "detail",
                                http_post=lambda url, json: calls.append((url, json)))
    assert calls and calls[0][0].endswith("/escalations")
    assert calls[0][1]["cx_issue_id"] == "issue-123" and calls[0][1]["severity"] == "urgent"


def test_escalate_failure_is_swallowed():
    db = _FakeDB()
    def boom(url, json): raise RuntimeError("cxo down")
    res = cia.file_and_maybe_escalate(db, "c1", "http://cxo:8098", "app_ux", "high",
                                      "x", "y", http_post=boom)
    assert res["cx_issue_id"] == "issue-123"  # filing still succeeds
```

- [ ] **Step 3: Run to verify FAIL** — `python -m pytest agent/tests/test_file_cx_issue.py -q` → FAIL.

- [ ] **Step 4: Write `agent/cx_issue_action.py`**

```python
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
```

- [ ] **Step 5: Add `insert_cx_issue` to the PM's DB module** — following its existing insert style, add:
```sql
INSERT INTO cx_issues (customer_id, category, severity, summary, detail, source)
VALUES (%s,%s,%s,%s,%s,'personal_manager') RETURNING id::text
```
returning the new id.

- [ ] **Step 6: Register the MCP tool in `agent/mcp_server.py`** — a customer-scoped `@mcp.tool() file_cx_issue(category, severity, summary, detail)` that calls `cx_issue_action.file_and_maybe_escalate(deps.db, deps.customer_id, deps.settings.cxo_url, ...)`. Follow the exact tool-declaration + deps pattern already in that file. Add `cxo_url` to `agent/config.py`.

- [ ] **Step 7: Run to verify PASS** — `python -m pytest agent/tests/test_file_cx_issue.py -q` → PASS (3 tests).

- [ ] **Step 8: Commit**

```bash
git add agent/cx_issue_action.py agent/tests/test_file_cx_issue.py agent/mcp_server.py agent/config.py agent/db.py
git commit -m "feat(agent): personal-manager file_cx_issue tool + best-effort CXO escalation"
```

---

## Task 13: Demo — `demos/09-cxo/`

**Files:**
- Create: `demos/09-cxo/drive.py`, `demos/09-cxo/run-demo.sh`, `demos/09-cxo/README.md`

**Interfaces:**
- Consumes: `demos/_driver.py run(beats, api_url, agent_label, run_hint)`; the deployed `cxo` (:8098, port-forwarded).
- Produces: a narrated 6-beat CX arc.

- [ ] **Step 1: Write `demos/09-cxo/drive.py`** — mirror `demos/08-cto/drive.py`'s structure (`sys.path` insert, `from _driver import run`, a `BEATS` list, `run(...)` in `__main__`) with beats:

```python
BEATS = [
  {"title": "Grounded CX posture", "thread": "new",
   "shows": "every CX figure is tool-grounded: onboarding, adoption, friction, engagement",
   "message": "Give me a grounded customer-experience posture right now: onboarding/"
              "activation, product adoption, friction (failed transactions + Interac "
              "expiries), and engagement. Use the numbers; this is an assessment."},
  {"title": "Derived figure (compute)", "thread": "new",
   "shows": "a rate the raw tools don't return — the CXO calls compute",
   "message": "What share of Interac e-Transfers expired unclaimed in the window? "
              "Give me the percentage — just the number."},
  {"title": "The customer voice", "thread": "new",
   "shows": "issue_summary + notable_issues: the top complaint themes, grounded",
   "message": "What are customers complaining about? Give me the open issues by "
              "category and severity, the top theme, and the most severe individual ones."},
  {"title": "Urgent escalation", "thread": "new", "outcome_hint": "read_only",
   "shows": "a personal-manager escalation is surfaced, re-grounded from cx_issues",
   "message": "Any urgent escalations from the personal managers right now? Surface "
              "them with the grounded issue details."},
  {"title": "Ranked feature backlog", "thread": "new",
   "shows": "the signature output: a prioritised backlog, each item citing its grounded signal",
   "message": "Give me a ranked feature backlog for next quarter — top 3 — each item "
              "justified by the CX signal that motivates it and its magnitude."},
  {"title": "Scope discipline + memory", "thread": "new", "outcome_hint": "deferred",
   "shows": "defers a P&L question to the CFO; records + recalls a durable CX note",
   "message": "What was our net interest margin last month, and note the top CX risk "
              "you'd watch as a durable note."},
]
```
`__main__` calls `run(BEATS, api_url=os.environ.get("CXO_API_URL","http://localhost:8098"), agent_label="Agent CXO", run_hint="demos/09-cxo/run-demo.sh")`.

- [ ] **Step 2: Write `demos/09-cxo/run-demo.sh`** — mirror `demos/08-cto/run-demo.sh`'s skeleton but simpler (no incident staging): `--no-up` guard; bring up via `scripts/deploy-all.sh` if not `--no-up`; **seed** step `python -m cx.seed_cx_issues` (via a port-forwarded DB or `kubectl exec`), ensure demo bank data exists (reuse `testing/generator` + rail simulators if the estate is empty); port-forward `svc/cxo 8098`; wait `/livez`; create the tiny `httpx` venv; drive `demos/09-cxo/drive.py`.

- [ ] **Step 3: Write `demos/09-cxo/README.md`** — mirror `demos/08-cto/README.md`: what it shows, how to run (`demos/09-cxo/run-demo.sh`, `--no-up`), prereqs, and the honesty note (analyst-only; no ledger acting rows — grounding + the backlog are the point).

- [ ] **Step 4: Shellcheck the runner** — `bash -n demos/09-cxo/run-demo.sh` → no errors.

- [ ] **Step 5: Commit**

```bash
git add demos/09-cxo/
git commit -m "feat(demo): demos/09-cxo narrated CX arc (posture, voice, escalation, backlog)"
```

---

## Task 14: Live smoke (verification)

**Files:** none (verification only). Requires docker+kind+kubectl and a port-forwarded bank DB.

- [ ] **Step 1: Deploy** — `export XDG_RUNTIME_DIR=/run/user/1000 XDG_DATA_HOME=/home/bmartins/.local/share; scripts/deploy-all.sh` (brings up `cx-mcp` + `cxo`). Apply `10_cx.sql` if the DB predates it.

- [ ] **Step 2: Ensure bank data + seed issues** — run `testing/generator` + a rail simulator if the estate is empty, then `python -m cx.seed_cx_issues` (DB port-forwarded). Expected: `seeded 40 cx_issues`.

- [ ] **Step 3: CX review** — port-forward `svc/cxo 8098`; `curl -s localhost:8098/livez`; then:
```bash
curl -s -X POST localhost:8098/ask -H 'content-type: application/json' \
  -d '{"message":"Give me a grounded CX posture and a ranked feature backlog."}' | python -m json.tool
```
Expected: a grounded posture (figures traceable to `cx` tools) + a ranked backlog; `claims` empty; the number-verifier reports all figures grounded.

- [ ] **Step 4: Escalation** — POST a scripted escalation and confirm the CXO surfaces it:
```bash
ISSUE=$(kubectl -n nano-bank exec deploy/cx-mcp -- python -c "print('smoke')")  # or pick a real id
curl -s -X POST localhost:8098/escalations -H 'content-type: application/json' \
  -d '{"cx_issue_id":"<a real cx_issue id>","severity":"urgent","category":"rail_experience","summary":"expired"}'
curl -s -X POST localhost:8098/ask -H 'content-type: application/json' \
  -d '{"message":"Any urgent escalations right now?"}' | python -m json.tool
```
Expected: the answer surfaces the escalation, grounded via `issue_detail`.

- [ ] **Step 5: Full demo** — `demos/09-cxo/run-demo.sh --no-up`. Expected: all 6 beats render; beat 6 defers the P&L question to the CFO.

- [ ] **Step 6: Commit** any fixes found; then the phase is complete.

---

## Self-Review

**Spec coverage:**
- CX foundation (behavioural now) → Tasks 2–4. ✓
- Metrics-&-surveys standing service (grounded, structured) → `cx` MCP, Tasks 2–6. ✓ (surveys = Phase 2, per spec)
- CXO analyst-only + ranked backlog → Tasks 7–11, beat 5. ✓
- Complaint channel: `cx_issues` table (Task 1), PM write + escalate (Task 12), CXO `/escalations` intake + re-grounding (Tasks 8, 10). ✓
- Structured/grounded interface → cx MCP tools + verifier + `compute` (Global Constraints; Task 9 prompt). ✓
- Demo + testing + ports/names + phase line → Tasks 13–14; ports 8097/8098; Phase 2 out of scope. ✓

**Placeholder scan:** No TBD/TODO. Boilerplate tasks (7 config, model_factory, Dockerfiles, k8s, requirements) reference an exact existing file to mirror with the specific field diffs enumerated — the engineer reads real committed code, not a placeholder.

**Type consistency:** `CxDB` method names used by `mcp_server.py` (`customers_onboarding`, `accounts_activation`, `product_activity`, `active_customer_count`, `transaction_outcomes`, `interac_outcomes`, `customer_recency`, `issue_rows`, `issue_by_id`) all defined in Task 2. `metrics.*` names used in Task 4 all defined in Task 3. `escalations.record/pending/clear` used in Tasks 8/10 defined in Task 8. `file_and_maybe_escalate`/`insert_cx_issue` consistent across Task 12. `cx_summary`/`issue_detail` tool names consistent between Task 4 (producer) and Task 8 (consumer).
