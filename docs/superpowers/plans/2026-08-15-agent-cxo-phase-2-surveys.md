# Agent CXO — Phase 2 (Surveys) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add NPS + CSAT survey campaigns to the CXO's grounded signal — a deterministic campaign runner with behaviour-correlated simulated responses, pure aggregators + cx MCP tools, and CXO prompt integration.

**Architecture:** Extends the existing `cx` metrics service (new tables, a runner, new tools) and the `cxo` seat (prompt only — it gets the tools via its MCP client). No new deployables.

**Tech Stack:** Python 3.12, `psycopg2`, `mcp.server.fastmcp`, the Phase-1 `cx`/`cxo` packages, PostgreSQL 16, kind/k8s.

**Spec:** `docs/superpowers/specs/2026-08-15-agent-cxo-phase-2-surveys-design.md`

## Global Constraints

- **Builds on Phase 1** (branch `agent-cxo`, PR #77): `cx/` (config, db, metrics, mcp_server, seed_cx_issues) and `cxo/` already exist. Follow their exact patterns.
- **Deterministic simulation:** `simulate_score` is pure and takes a seeded `random.Random`; `create_campaign` seeds it reproducibly. No wall-clock randomness.
- **CXO stays analyst-only / read-only:** it reads survey results; it never creates campaigns.
- **DB access read-only for reads** (`CxDB.rows`, `set_session(readonly=True)`); campaign writes open a separate write connection (mirror `agent/db.py insert_cx_issue` / `cx/seed_cx_issues.py`).
- **NPS buckets:** promoter 9–10, passive 7–8, detractor 0–6; `score = round(%promoters − %detractors)`. **CSAT satisfied = score ≥ 4.**
- **Schema facts (Phase 1):** customers PK `customer_id`; transactions link via `initiated_by`; `cx_issues.status <> 'resolved'` = open.
- **DB env / apply:** apply DDL via `kubectl -n nano-bank exec -i deploy/postgres -- psql -U nanobank_user -d nano_bank_db < file`. Live checks use a port-forward `kubectl -n nano-bank port-forward --address ::1 svc/postgres-service 5432:5432` with `DB_HOST=::1`.

---

## File Structure

```
src/core/tables/11_cx_surveys.sql   # survey_campaigns + survey_responses + enum (NEW)
cx/campaigns.py                     # runner: resolve_segment, customer_sentiment, simulate_score, create_campaign (NEW)
cx/seed_surveys.py                  # demo seeder: 2 campaigns + simulate (NEW)
cx/metrics.py                       # ADD nps(scores), csat(scores)
cx/db.py                            # ADD segment/sentiment/survey read + campaign write helpers
cx/mcp_server.py                    # ADD list_campaigns, nps_score, csat_score, survey_results; surveys in cx_summary
cx/tests/test_metrics.py            # ADD nps/csat tests
cx/tests/test_campaigns.py          # simulate_score determinism + correlation (NEW)
cx/tests/test_seed_surveys.py       # seeder builds 2 campaign specs (NEW)
cxo/agent.py                        # extend CXO_PROMPT (surveys)
cxo/tests/test_prompt.py            # asserts CXO_PROMPT mentions NPS/CSAT/survey (NEW)
demos/09-cxo/drive.py               # ADD the NPS beat
demos/09-cxo/run-demo.sh            # ADD survey-seed step
```

---

## Task 1: survey DDL

**Files:**
- Create: `src/core/tables/11_cx_surveys.sql`

**Interfaces:**
- Produces: enum `survey_instrument`; tables `survey_campaigns`, `survey_responses`.

- [ ] **Step 1: Write the DDL**

```sql
-- src/core/tables/11_cx_surveys.sql — NPS/CSAT survey campaigns + responses.
DO $$ BEGIN
  CREATE TYPE survey_instrument AS ENUM ('nps','csat');
EXCEPTION WHEN duplicate_object THEN null; END $$;

CREATE TABLE IF NOT EXISTS survey_campaigns (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    instrument  survey_instrument NOT NULL,
    segment     TEXT NOT NULL,
    question    TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'open',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS survey_responses (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    campaign_id UUID NOT NULL REFERENCES survey_campaigns(id),
    customer_id UUID NOT NULL REFERENCES customers(customer_id),
    score       INT NOT NULL,
    comment     TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_survey_responses_campaign ON survey_responses(campaign_id);
CREATE INDEX IF NOT EXISTS idx_survey_campaigns_instrument ON survey_campaigns(instrument);
```

- [ ] **Step 2: Apply + verify**

```bash
export XDG_RUNTIME_DIR=/run/user/1000 XDG_DATA_HOME=/home/bmartins/.local/share
kubectl --context kind-nano-bank -n nano-bank exec -i deploy/postgres -- \
  psql -U nanobank_user -d nano_bank_db < src/core/tables/11_cx_surveys.sql
kubectl --context kind-nano-bank -n nano-bank exec -i deploy/postgres -- \
  psql -U nanobank_user -d nano_bank_db -c "\d survey_responses"
```
Expected: both tables + indexes created; re-running is idempotent.

- [ ] **Step 3: Commit**

```bash
git add src/core/tables/11_cx_surveys.sql
git commit -m "feat(cx): survey_campaigns + survey_responses tables (NPS/CSAT)"
```

---

## Task 2: NPS + CSAT aggregators (pure)

**Files:**
- Modify: `cx/metrics.py`
- Modify: `cx/tests/test_metrics.py`

**Interfaces:**
- Produces: `nps(scores: list[int]) -> dict`, `csat(scores: list[int]) -> dict`.

- [ ] **Step 1: Add the failing tests** to `cx/tests/test_metrics.py`:

```python
def test_nps_score_and_buckets():
    # 5 promoters(9,10,9,10,9), 2 passives(7,8), 3 detractors(0,3,6)
    scores = [9, 10, 9, 10, 9, 7, 8, 0, 3, 6]
    r = m.nps(scores)
    assert r["responses"] == 10
    assert r["promoters"] == 5 and r["passives"] == 2 and r["detractors"] == 3
    assert r["score"] == 20   # round(50% - 30%)


def test_nps_empty_is_zero():
    r = m.nps([])
    assert r["responses"] == 0 and r["score"] == 0


def test_csat_rate_and_mean():
    scores = [5, 4, 3, 4, 1]           # satisfied(>=4) = 3 of 5
    r = m.csat(scores)
    assert r["responses"] == 5 and r["satisfied"] == 3
    assert r["csat_rate"] == 60.0
    assert r["mean"] == 3.4
```

- [ ] **Step 2: Run to verify FAIL** — `python -m pytest cx/tests/test_metrics.py -q` → FAIL.

- [ ] **Step 3: Add to `cx/metrics.py`** (after `csat`? no — after `notable_issues`; reuse the existing `pct`):

```python
def nps(scores: list[int]) -> dict:
    n = len(scores)
    promoters = sum(1 for s in scores if s >= 9)
    detractors = sum(1 for s in scores if s <= 6)
    passives = n - promoters - detractors
    return {"responses": n, "promoters": promoters, "passives": passives,
            "detractors": detractors,
            "score": round(pct(promoters, n) - pct(detractors, n))}


def csat(scores: list[int]) -> dict:
    n = len(scores)
    satisfied = sum(1 for s in scores if s >= 4)
    mean = round(sum(scores) / n, 2) if n else 0.0
    return {"responses": n, "satisfied": satisfied,
            "csat_rate": pct(satisfied, n), "mean": mean}
```

- [ ] **Step 4: Run to verify PASS** — `python -m pytest cx/tests/test_metrics.py -q` → PASS.

- [ ] **Step 5: Commit**

```bash
git add cx/metrics.py cx/tests/test_metrics.py
git commit -m "feat(cx): pure nps() + csat() aggregators + tests"
```

---

## Task 3: `simulate_score` (pure, correlated)

**Files:**
- Create: `cx/campaigns.py` (this task adds only `simulate_score`)
- Create: `cx/tests/test_campaigns.py`

**Interfaces:**
- Produces: `simulate_score(instrument: str, sentiment: int, rng) -> int`.

- [ ] **Step 1: Write the failing test** `cx/tests/test_campaigns.py`:

```python
import random
from cx import campaigns as c


def test_simulate_score_is_deterministic():
    a = [c.simulate_score("nps", 0, random.Random(1)) for _ in range(20)]
    b = [c.simulate_score("nps", 0, random.Random(1)) for _ in range(20)]
    assert a == b


def test_nps_negative_sentiment_scores_lower_than_positive():
    rng = random.Random(3)
    neg = [c.simulate_score("nps", -1, rng) for _ in range(200)]
    pos = [c.simulate_score("nps", 1, rng) for _ in range(200)]
    assert sum(neg) / 200 < sum(pos) / 200
    assert all(0 <= s <= 10 for s in neg + pos)


def test_csat_ranges_and_correlation():
    rng = random.Random(5)
    neg = [c.simulate_score("csat", -1, rng) for _ in range(200)]
    pos = [c.simulate_score("csat", 1, rng) for _ in range(200)]
    assert all(1 <= s <= 5 for s in neg + pos)
    assert sum(neg) / 200 < sum(pos) / 200
```

- [ ] **Step 2: Run to verify FAIL** — `python -m pytest cx/tests/test_campaigns.py -q` → FAIL (no `cx.campaigns`).

- [ ] **Step 3: Write `cx/campaigns.py`** (this step: imports + `simulate_score` only):

```python
# cx/campaigns.py — the deterministic survey campaign runner. simulate_score is
# pure (seeded rng); create_campaign does the DB IO.
from __future__ import annotations

# Weighted score buckets by (instrument, sentiment sign). Sentiment: -1 negative
# (open issue / dormant), +1 positive (active, no issue), 0 neutral.
_NPS_BUCKETS = {
    -1: [0, 1, 2, 3, 4, 5, 6, 6, 7],          # skew detractor
     0: [6, 7, 7, 8, 8, 9],                    # mixed
     1: [7, 8, 9, 9, 10, 10],                  # skew promoter
}
_CSAT_BUCKETS = {
    -1: [1, 2, 2, 3],
     0: [3, 3, 4],
     1: [4, 5, 5],
}


def simulate_score(instrument: str, sentiment: int, rng) -> int:
    sign = -1 if sentiment < 0 else (1 if sentiment > 0 else 0)
    buckets = _NPS_BUCKETS if instrument == "nps" else _CSAT_BUCKETS
    return rng.choice(buckets[sign])
```

- [ ] **Step 4: Run to verify PASS** — `python -m pytest cx/tests/test_campaigns.py -q` → PASS.

- [ ] **Step 5: Commit**

```bash
git add cx/campaigns.py cx/tests/test_campaigns.py
git commit -m "feat(cx): pure correlated simulate_score + determinism/correlation tests"
```

---

## Task 4: `CxDB` survey + segment + sentiment reads/writes

**Files:**
- Modify: `cx/db.py`

**Interfaces:**
- Produces on `CxDB`: `resolve_segment(segment, window_days) -> list[str]`, `open_issue_customers() -> set[str]`, `dormant_customers(window_days) -> set[str]`, `insert_campaign(instrument, segment, question) -> str`, `insert_responses(campaign_id, rows) -> None`, `campaigns() -> list[dict]`, `survey_scores(campaign_id=None, instrument=None) -> list[int]`.

- [ ] **Step 1: Add the read/write methods to `cx/db.py`** (after `issue_by_id`):

```python
    # --- surveys / segments ---------------------------------------------------
    def resolve_segment(self, segment: str, window_days: int) -> list[str]:
        if segment == "all_active":
            rows = self.rows(
                "SELECT DISTINCT initiated_by::text AS c FROM transactions"
                " WHERE initiated_by IS NOT NULL"
                " AND created_at >= now() - (%s || ' days')::interval", (window_days,))
        elif segment.startswith("product:"):
            rows = self.rows(
                "SELECT DISTINCT initiated_by::text AS c FROM transactions"
                " WHERE product = %s AND initiated_by IS NOT NULL"
                " AND created_at >= now() - (%s || ' days')::interval",
                (segment.split(":", 1)[1], window_days))
        elif segment == "has_open_issue":
            rows = self.rows(
                "SELECT DISTINCT customer_id::text AS c FROM cx_issues WHERE status <> 'resolved'")
        elif segment == "dormant":
            rows = self.rows(
                "SELECT c.customer_id::text AS c FROM customers c"
                " WHERE NOT EXISTS (SELECT 1 FROM transactions t WHERE t.initiated_by = c.customer_id"
                " AND t.created_at >= now() - (%s || ' days')::interval)", (window_days,))
        else:
            return []
        return [r["c"] for r in rows]

    def open_issue_customers(self) -> set:
        return {r["c"] for r in self.rows(
            "SELECT DISTINCT customer_id::text AS c FROM cx_issues WHERE status <> 'resolved'")}

    def dormant_customers(self, window_days: int) -> set:
        return {r["c"] for r in self.rows(
            "SELECT c.customer_id::text AS c FROM customers c"
            " WHERE NOT EXISTS (SELECT 1 FROM transactions t WHERE t.initiated_by = c.customer_id"
            " AND t.created_at >= now() - (%s || ' days')::interval)", (window_days,))}

    def insert_campaign(self, instrument: str, segment: str, question: str) -> str:
        import psycopg2
        conn = psycopg2.connect(**self._db)
        try:
            with conn, conn.cursor() as cur:
                cur.execute(
                    "INSERT INTO survey_campaigns (instrument, segment, question)"
                    " VALUES (%s,%s,%s) RETURNING id::text", (instrument, segment, question))
                return cur.fetchone()[0]
        finally:
            conn.close()

    def insert_responses(self, campaign_id: str, rows: list[tuple]) -> None:
        # rows: [(customer_id, score), ...]
        import psycopg2
        import psycopg2.extras
        conn = psycopg2.connect(**self._db)
        try:
            with conn, conn.cursor() as cur:
                psycopg2.extras.execute_values(
                    cur, "INSERT INTO survey_responses (campaign_id, customer_id, score)"
                    " VALUES %s", [(campaign_id, c, s) for c, s in rows])
        finally:
            conn.close()

    def campaigns(self) -> list[dict]:
        return self.rows(
            "SELECT sc.id::text, sc.instrument::text, sc.segment, sc.question, sc.status,"
            " count(sr.id) AS responses FROM survey_campaigns sc"
            " LEFT JOIN survey_responses sr ON sr.campaign_id = sc.id"
            " GROUP BY sc.id ORDER BY sc.created_at DESC")

    def survey_scores(self, campaign_id: str = None, instrument: str = None) -> list[int]:
        if campaign_id:
            rows = self.rows("SELECT score FROM survey_responses WHERE campaign_id = %s",
                             (campaign_id,))
        elif instrument:
            rows = self.rows(
                "SELECT sr.score FROM survey_responses sr JOIN survey_campaigns sc"
                " ON sc.id = sr.campaign_id WHERE sc.instrument = %s", (instrument,))
        else:
            rows = self.rows("SELECT score FROM survey_responses")
        return [r["score"] for r in rows]
```

- [ ] **Step 2: Syntax check** — `python -c "import cx.db; print('ok')"` → `ok`.

- [ ] **Step 3: Commit**

```bash
git add cx/db.py
git commit -m "feat(cx): CxDB survey reads/writes + segment/sentiment resolution"
```

---

## Task 5: `create_campaign` runner + live verify

**Files:**
- Modify: `cx/campaigns.py` (add the runner)

**Interfaces:**
- Consumes: `CxDB` (Task 4), `simulate_score` (Task 3).
- Produces: `customer_sentiment(db, targets, window_days) -> dict[str,int]`, `create_campaign(db, instrument, segment, question, seed=7, window_days=30) -> dict`.

- [ ] **Step 1: Add to `cx/campaigns.py`**

```python
import random


def customer_sentiment(db, targets: list[str], window_days: int) -> dict:
    """-1 for customers with an open issue or dormant, +1 for active-no-issue, else 0."""
    issue = db.open_issue_customers()
    dormant = db.dormant_customers(window_days)
    out = {}
    for c in targets:
        if c in issue or c in dormant:
            out[c] = -1
        else:
            out[c] = 1
    return out


def create_campaign(db, instrument: str, segment: str, question: str,
                    seed: int = 7, window_days: int = 30) -> dict:
    if instrument not in ("nps", "csat"):
        raise ValueError(f"unknown instrument: {instrument}")
    targets = db.resolve_segment(segment, window_days)
    campaign_id = db.insert_campaign(instrument, segment, question)
    sentiment = customer_sentiment(db, targets, window_days)
    rng = random.Random(f"{seed}:{campaign_id}")
    rows = [(c, simulate_score(instrument, sentiment[c], rng)) for c in targets]
    if rows:
        db.insert_responses(campaign_id, rows)
    return {"campaign_id": campaign_id, "instrument": instrument, "segment": segment,
            "responses": len(rows)}
```

- [ ] **Step 2: Live verify** (bank DB port-forwarded on `::1`, `cx_issues` seeded):

```bash
export XDG_RUNTIME_DIR=/run/user/1000 XDG_DATA_HOME=/home/bmartins/.local/share
kubectl --context kind-nano-bank -n nano-bank port-forward --address ::1 svc/postgres-service 5432:5432 >/tmp/pf.log 2>&1 &
sleep 3
DB_HOST=::1 python -c "
from cx.db import CxDB; from cx.config import Settings; from cx import campaigns, metrics
db = CxDB(Settings.from_env().db)
r = campaigns.create_campaign(db, 'nps', 'all_active', 'How likely are you to recommend us?')
print('campaign:', r)
print('nps:', metrics.nps(db.survey_scores(instrument='nps')))
"
```
Expected: `responses` > 0 and an `nps` dict with a score in [-100, 100].

- [ ] **Step 3: Commit**

```bash
git add cx/campaigns.py
git commit -m "feat(cx): create_campaign runner (resolve->sentiment->simulate->persist)"
```

---

## Task 6: cx MCP survey tools

**Files:**
- Modify: `cx/mcp_server.py`

**Interfaces:**
- Consumes: `CxDB.campaigns/survey_scores` (Task 4), `metrics.nps/csat` (Task 2).
- Produces: MCP tools `list_campaigns`, `nps_score`, `csat_score`, `survey_results`; a `surveys` block on `cx_summary`.

- [ ] **Step 1: Add the tools in `build_mcp`** (before `cx_summary`):

```python
    @mcp.tool()
    def list_campaigns() -> list:
        """Survey campaigns with instrument, segment, status, and response count."""
        return db.campaigns()

    @mcp.tool()
    def nps_score(campaign_id: str = "") -> dict:
        """Net Promoter Score for one campaign (by id) or across all NPS campaigns."""
        return metrics.nps(db.survey_scores(campaign_id=campaign_id or None,
                                            instrument=None if campaign_id else "nps"))

    @mcp.tool()
    def csat_score(campaign_id: str = "") -> dict:
        """CSAT for one campaign (by id) or across all CSAT campaigns."""
        return metrics.csat(db.survey_scores(campaign_id=campaign_id or None,
                                             instrument=None if campaign_id else "csat"))

    @mcp.tool()
    def survey_results(campaign_id: str = "") -> list:
        """Per-campaign summary: instrument, segment, responses, and headline score."""
        out = []
        for c in db.campaigns():
            if campaign_id and c["id"] != campaign_id:
                continue
            scores = db.survey_scores(campaign_id=c["id"])
            agg = metrics.nps(scores) if c["instrument"] == "nps" else metrics.csat(scores)
            out.append({"campaign_id": c["id"], "instrument": c["instrument"],
                        "segment": c["segment"], "responses": c["responses"],
                        "score": agg.get("score", agg.get("csat_rate"))})
        return out
```

- [ ] **Step 2: Add `surveys` to `cx_summary`** — inside the `cx_summary` tool's returned dict add:

```python
                "surveys": {"nps": metrics.nps(db.survey_scores(instrument="nps")),
                            "csat": metrics.csat(db.survey_scores(instrument="csat"))},
```

- [ ] **Step 3: Tool-list smoke**

```bash
python -c "
import anyio
from cx.mcp_server import build_mcp, Deps
from cx.db import CxDB
m = build_mcp(Deps(CxDB({}), 30))
names = sorted(t.name for t in anyio.run(m.list_tools))
print(names)
assert {'list_campaigns','nps_score','csat_score','survey_results'} <= set(names)
print('survey tools registered')
"
```
Expected: `survey tools registered`.

- [ ] **Step 4: Commit**

```bash
git add cx/mcp_server.py
git commit -m "feat(cx): nps_score/csat_score/survey_results/list_campaigns MCP tools + cx_summary surveys"
```

---

## Task 7: survey seeder

**Files:**
- Create: `cx/seed_surveys.py`, `cx/tests/test_seed_surveys.py`

**Interfaces:**
- Produces: `campaign_specs() -> list[dict]` (pure: the demo campaigns), `seed(db_params) -> list[dict]`.

- [ ] **Step 1: Write the failing test** `cx/tests/test_seed_surveys.py`:

```python
from cx import seed_surveys as s


def test_campaign_specs_are_nps_and_csat():
    specs = s.campaign_specs()
    insts = {sp["instrument"] for sp in specs}
    assert insts == {"nps", "csat"}
    assert any(sp["segment"] == "has_open_issue" for sp in specs)
    assert all(sp["question"] for sp in specs)
```

- [ ] **Step 2: Run to verify FAIL** — `python -m pytest cx/tests/test_seed_surveys.py -q` → FAIL.

- [ ] **Step 3: Write `cx/seed_surveys.py`**

```python
# cx/seed_surveys.py — create the demo survey campaigns + simulate responses.
from __future__ import annotations
from . import campaigns as _campaigns
from .db import CxDB


def campaign_specs() -> list[dict]:
    return [
        {"instrument": "nps", "segment": "all_active",
         "question": "How likely are you to recommend nano-bank to a friend?"},
        {"instrument": "csat", "segment": "has_open_issue",
         "question": "How satisfied were you with your recent Interac e-Transfer?"},
    ]


def seed(db_params: dict) -> list[dict]:
    db = CxDB(db_params)
    # clear prior demo campaigns so the seed is reproducible
    import psycopg2
    conn = psycopg2.connect(**db_params)
    try:
        with conn, conn.cursor() as cur:
            cur.execute("DELETE FROM survey_responses")
            cur.execute("DELETE FROM survey_campaigns")
    finally:
        conn.close()
    return [_campaigns.create_campaign(db, sp["instrument"], sp["segment"], sp["question"])
            for sp in campaign_specs()]


if __name__ == "__main__":
    from .config import Settings
    for r in seed(Settings.from_env().db):
        print("seeded campaign", r)
```

- [ ] **Step 4: Run to verify PASS** — `python -m pytest cx/tests/test_seed_surveys.py -q` → PASS.

- [ ] **Step 5: Live run** (DB port-forwarded, `cx_issues` seeded):

```bash
DB_HOST=::1 python -m cx.seed_surveys
```
Expected: two `seeded campaign ...` lines with `responses` > 0.

- [ ] **Step 6: Commit**

```bash
git add cx/seed_surveys.py cx/tests/test_seed_surveys.py
git commit -m "feat(cx): survey seeder (NPS all_active + CSAT has_open_issue) + test"
```

---

## Task 8: CXO prompt integration

**Files:**
- Modify: `cxo/agent.py`
- Create: `cxo/tests/test_prompt.py`

**Interfaces:**
- Produces: an extended `CXO_PROMPT` that folds NPS/CSAT into posture + backlog.

- [ ] **Step 1: Write the failing test** `cxo/tests/test_prompt.py`:

```python
from cxo.agent import CXO_PROMPT


def test_prompt_mentions_surveys():
    low = CXO_PROMPT.lower()
    assert "nps" in low and "csat" in low and "survey" in low
    assert "detractor" in low
```

- [ ] **Step 2: Run to verify FAIL** — `python -m pytest cxo/tests/test_prompt.py -q` → FAIL.

- [ ] **Step 3: Extend `CXO_PROMPT`** in `cxo/agent.py` — insert this sentence into the "customer VOICE" description (after the issues/escalations clause, before "Stay in your lane"):

```
    "You also read PROACTIVE survey signal via the cx tools — NPS "
    "(nps_score: promoters/passives/detractors and the score) and CSAT "
    "(csat_score), per campaign or overall (list_campaigns / survey_results). "
    "Fold the survey signal into your posture and backlog: call out the NPS score "
    "and where DETRACTORS cluster (which segment/campaign), and use it to justify "
    "backlog items alongside the behavioural and complaint signals. "
```

- [ ] **Step 4: Run to verify PASS** — `python -m pytest cxo/tests/test_prompt.py -q` → PASS.

- [ ] **Step 5: Commit**

```bash
git add cxo/agent.py cxo/tests/test_prompt.py
git commit -m "feat(cxo): fold NPS/CSAT survey signal into the CXO prompt + test"
```

---

## Task 9: demo — survey beat + seed step

**Files:**
- Modify: `demos/09-cxo/drive.py`, `demos/09-cxo/run-demo.sh`

**Interfaces:**
- Produces: a new NPS beat; a survey-seed step in the runner.

- [ ] **Step 1: Add the beat to `demos/09-cxo/drive.py`** — insert BETWEEN the "The customer voice" beat and the "Ranked feature backlog" beat:

```python
    {
        "title": "Survey signal — NPS + CSAT",
        "shows": "proactive VoC: the CXO reads NPS/CSAT and says where detractors cluster",
        "message": "What is our current NPS, and how does CSAT look — especially for "
                   "the Interac e-Transfer experience? Tell me where the detractors "
                   "cluster.",
        "thread": "new",
    },
```

- [ ] **Step 2: Add the seed step to `demos/09-cxo/run-demo.sh`** — in the `if [ "$DO_SEED" = "1" ]` block that seeds `cx_issues` (after `python -m cx.seed_cx_issues`), add:

```bash
  # Ensure the survey tables exist, then seed the demo campaigns (NPS + CSAT).
  kubectl --context "$CTX" -n "$NS" exec -i deploy/postgres -- \
    psql -U nanobank_user -d nano_bank_db < src/core/tables/11_cx_surveys.sql >/dev/null 2>&1 || true
  DB_HOST=::1 python -m cx.seed_surveys || echo "⚠ survey seed skipped"
```

- [ ] **Step 3: Syntax check**

```bash
python -c "import ast; ast.parse(open('demos/09-cxo/drive.py').read()); print('drive ok')"
bash -n demos/09-cxo/run-demo.sh && echo "run-demo ok"
```
Expected: both ok.

- [ ] **Step 4: Commit**

```bash
git add demos/09-cxo/drive.py demos/09-cxo/run-demo.sh
git commit -m "feat(demo): NPS/CSAT survey beat + survey-seed step in demos/09-cxo"
```

---

## Task 10: live smoke (verification)

**Files:** none. Requires docker+kind+kubectl and a port-forwarded bank DB.

- [ ] **Step 1: Apply DDL + rebuild/deploy** — apply `11_cx_surveys.sql` (Task 1 Step 2), then `export XDG_RUNTIME_DIR=/run/user/1000 XDG_DATA_HOME=/home/bmartins/.local/share; ./cxo/k8s/deploy.sh` (rebuilds `nano-cx` with the new tools + `nano-cxo` with the new prompt).

- [ ] **Step 2: Seed** — `cx_issues` (`DB_HOST=::1 python -m cx.seed_cx_issues`) then surveys (`DB_HOST=::1 python -m cx.seed_surveys`). Expected: two campaigns with responses > 0.

- [ ] **Step 3: Grounded NPS posture** — port-forward `svc/cxo 8098`; then:
```bash
curl -s -X POST localhost:8098/ask -H 'content-type: application/json' \
  -d '{"message":"What is our NPS and CSAT, and where do detractors cluster?"}' \
  --max-time 180 | python -c "import sys,json; print(json.load(sys.stdin)['answer'][:1200])"
```
Expected: a grounded NPS score + CSAT rate (traceable to `nps_score`/`csat_score`), naming the detractor cluster (e.g. the Interac CSAT campaign); the verifier reports figures grounded.

- [ ] **Step 4: Full demo** — `demos/09-cxo/run-demo.sh --no-up`. Expected: 7 beats render; the new survey beat is grounded; the backlog folds the survey signal.

- [ ] **Step 5: Commit** any fixes; the phase is complete.

---

## Self-Review

**Spec coverage:** NPS+CSAT instruments → Tasks 2,3. Deterministic runner + correlated simulation → Tasks 3–5. Tables → Task 1. Segments → Task 4. Aggregators + tools → Tasks 2,6. `cx_summary` surveys → Task 6. Seeder → Task 7. CXO prompt integration → Task 8. Demo beat + seed → Task 9. Testing + live smoke → each task's tests + Task 10. Phase-3 items (autonomous agent, live-PM) correctly excluded.

**Placeholder scan:** No TBD/TODO. Every code step has real code; boilerplate-free (all new logic is small and given in full).

**Type consistency:** `simulate_score(instrument, sentiment, rng)` signature identical across Tasks 3/5. `CxDB` methods used in Tasks 5/6 (`resolve_segment`, `open_issue_customers`, `dormant_customers`, `insert_campaign`, `insert_responses`, `campaigns`, `survey_scores`) all defined in Task 4. `metrics.nps`/`metrics.csat` used in Task 6 defined in Task 2. `campaigns.create_campaign` used in Task 7 defined in Task 5. `db.survey_scores(campaign_id=, instrument=)` keyword usage consistent between Task 4 (def) and Task 6 (calls).
