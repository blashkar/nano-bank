# Agent CXO — Phase 1: customer-experience analyst + a metrics-&-surveys agent

**Status:** design approved 2026-08-15 (brainstorming). Adds a fifth C-suite seat
to the estate (CFO/COO/CTO already exist), plus a subordinate standing agent.

## Goal

Give the bank a **Chief Experience Officer** agent that speaks for the overall
**customer experience** and produces a **ranked, grounded feature backlog**. The
CXO is an *analyst* (like the CFO): it reads, synthesises, and recommends — it does
not act on bank state. Its customer-experience picture is fed by a **standing
subordinate agent that owns the metrics and surveys**, which the CXO calls over
HTTP, and by **complaints/issues the per-customer personal managers file**.

Two narratives:
1. **CX posture** — a grounded review of onboarding, product adoption, friction,
   and engagement, plus the customer voice (issues/escalations).
2. **The feature backlog** — a prioritised list of features/opportunities, each
   justified by the grounded signal that motivates it.

## Non-goals (Phase 1)

- The CXO has **no acting levers**. It does not build, merge, launch campaigns, or
  mutate bank state. It recommends; humans / other seats act. (If asked to ship a
  feature it produces a backlog item and notes that implementation would go through
  the CTO's gated coder — it does **not** delegate.)
- **Surveys/NPS/CSAT are Phase 2**, not this spec. Phase 1's customer-experience
  signal is *behavioural* (existing bank data) + *complaints* (personal managers).
- No live personal-manager deployment in the demo. The PM write contract is built
  and unit-tested; the demo drives it via a seeder + one scripted escalation.

## Decisions (locked in brainstorming)

| Question | Decision |
|---|---|
| CX foundation | Behavioural signals now (Phase 1) + surveys/NPS later (Phase 2). |
| Metrics-&-surveys agent | A **standing separate service** (its own agent + data plane + API); the CXO calls it over HTTP. |
| CXO mandate | **Analyst-only** → a ranked, grounded feature backlog. No acting levers. |
| Complaint channel | A `cx_issues` table (personal managers **write**) **+** a best-effort **A2A escalation** ping for urgent issues; the CXO **reads**. |
| CXO↔metrics interface | **Structured** metric endpoints, so every figure the CXO quotes is a real tool result the grounding verifier can check. |
| PM scope (Phase 1) | Build & unit-test the PM `file_cx_issue` + escalate path and the CXO intake; **seed** the demo. |

## Architecture

```
                          ┌─────────────────────────────────────────────┐
 personal managers ───────┤ (write) cx_issues  ◄── new table in bank PG   │
   (agent/, per-customer) │ (A2A, urgent) escalation ───────────┐        │
                          └─────────────────────────────────────┼────────┘
                                                                ▼
 ┌───────────────────────────┐  structured /metrics/* (HTTP)  ┌──────────────┐
 │ metrics-&-surveys agent    │ ◄───────────────────────────  │  CXO agent   │
 │  cxm/  :8097               │  cx_summary, product_adoption, │  cxo/ :8098  │
 │  · CX data plane over the  │  friction_metrics, engagement, │  analyst     │
 │    bank DB + cx_issues     │  onboarding_funnel, issue_*     │  · /ask      │
 │  · /metrics/* endpoints    │ ─────────────────────────────► │  · /escalations
 │  · /ask (narrative)        │  (optional /ask deep-dive)     │    intake    │
 │  · surveys (Phase 2)       │                                │  ranked      │
 └───────────────────────────┘                                │  feature     │
                                                              │  backlog     │
                                                              └──────┬───────┘
                          every answer → csuite grounding verifier   │
                          durable CX notes → Qdrant memory  ◄─────────┘
```

Two new deployables (matching the "two pods, two APIs" decision). Both are thin
`csuite` agents, reusing the shared runtime + harness (planning/todos/memory/
subagents), the grounding verifier, Qdrant memory, and the seat file layout used
by `cfo/`, `coo/`, `cto/` (`agent.py`, `config.py`, `model_factory.py`, `tools.py`,
`claims.py`, `api.py`/`api_main.py`, `console.py`, `Dockerfile`, `k8s/`, `tests/`).

### 1. The metrics-&-surveys agent — `cxm/` (:8097)

The *measurement specialist*. It hosts the **CX data plane** (metric computations
over the bank DB + `cx_issues` aggregates) as its tool/MCP module, and exposes it
two ways: **structured `/metrics/*` HTTP endpoints** the CXO calls (deterministic,
pre-computed numbers so figures stay grounded), and an **`/ask`** LLM agent for a
narrative deep-dive. It owns surveys in Phase 2. Kept in one deployable now; it can
split its data plane into a separate MCP pod later if reuse demands it (YAGNI).

Endpoints (all read-only; windowed ones take `as_of` / `window_days`):

- **`GET /metrics/cx_summary`** — a small headline bundle of the top figures for
  the CXO's opening posture.
- **`GET /metrics/onboarding_funnel`** — customers created; KYC pending vs
  completed; accounts `pending_activation` vs `active`. → activation friction.
- **`GET /metrics/product_adoption`** — per product (deposit, card, Interac, AFT,
  Lynx): count/% of active customers who transacted on it (`transactions.product`
  + rail tables); multi-product customers. → what's used vs ignored.
- **`GET /metrics/friction_metrics`** — transaction failure rate
  (`status='failed'` / total, by product) and the Interac outcome mix
  (declined/expired/failed vs `deposit_completed`). → where customers hit walls.
- **`GET /metrics/engagement_metrics`** — active vs dormant customers (last
  transaction within/after `window_days`), recency distribution, transaction
  frequency. → retention/health.
- **`GET /metrics/issue_summary`** — from `cx_issues`: open counts by category &
  severity, new-vs-resolved trend, top themes.
- **`GET /metrics/notable_issues?limit=N`** — recent high-severity individual
  issues (customer-scoped, redacted), and a **by-id lookup** the CXO uses to
  re-ground an escalation.
- `GET /livez`, `GET /health` (like the other seats).

Pure, unit-testable metric functions sit behind the endpoints (each returns a
structured dict of grounded numbers); the HTTP layer is a thin wrapper.

### 2. The CXO agent — `cxo/` (:8098)

The *strategist*. A thin `csuite` analyst agent whose tools are HTTP clients to
`cxm`'s structured endpoints, plus:

- **`pending_escalations()`** — the in-session escalation queue populated by the
  `/escalations` intake; each entry is **re-grounded** by reading its `cx_issue`
  details from `cxm` (never trusting the ping payload's numbers).
- **`compute`** — the shared tool, for any derived ratio the CXO forms itself.
- harness: `write_plan`/`write_todos`, `recall_memory`/`record_memory` (durable CX
  notes), and an optional subagent for a focused deep-dive (e.g. one product's
  friction).

**Prompt / lane.** The CXO answers only from `cxm`'s tool outputs; never fabricates
a figure; calls `compute` for derived numbers. Lane = CX posture (onboarding,
adoption, friction, engagement) + the customer voice (issues/escalations) + the
feature backlog. Stays in lane: books → CFO; platform reliability → CTO; rail/ops
throughput → COO; fraud/AML → not visible. **Analyst-only** (no acting levers).

**Signature output — the ranked feature backlog.** From the CX signals, a
prioritised list where each item cites the grounded signal that motivates it and
its magnitude, e.g.:

> **#1 — Interac autodeposit onboarding.** 38% of e-Transfers expire unclaimed
> (`friction_metrics`); 12 open `rail_experience` issues, the top complaint theme
> (`issue_summary`). *Why:* attacks the largest friction point and the #1
> customer-voice theme at once.

A CX review runs as: plan → pull `cx_summary` + the metric endpoints +
`issue_summary` + `pending_escalations` → synthesise posture → rank the backlog →
record a durable CX note.

### 3. The complaint channel

**3a. PM write path.** The personal manager (`agent/`) gains one MCP tool:

```
file_cx_issue(category, severity, summary, detail)
   → inserts a cx_issues row, customer-scoped (the PM's bound customer),
     source='personal_manager', status='open'
```

It is a benign, non-money write, so — unlike the PM's money-movement tools — it is
**not** confirm-gated; it is recorded through the PM's normal action log. This is
the durable record of a complaint.

**3b. A2A escalation (urgent voice).** When a filed issue is `high`/`urgent`, the
PM additionally fires a best-effort ping to the CXO:

```
POST {cxo_url}/escalations  {cx_issue_id, customer_id, category, severity, summary}
```

The CXO's `POST /escalations` records the pending pointer (in-memory queue + a
durable memory note) and re-reads that issue's details from `cxm` to ground it. If
the CXO is down, nothing is lost — the issue is already in `cx_issues`. The CXO
stays read-only on bank data (it never writes `cx_issues`; it is pinged and reads).

**3c. Seeding for the demo.** The PM isn't in the demo cluster, and a realistic
*spread* of complaints is what makes the metrics meaningful — so `cxm/seed_cx_issues.py`
writes a believable distribution (varied customers, categories, severities,
timestamps) as-if PM-filed, and the demo fires **one scripted urgent escalation**
to the CXO intake to exercise 3b. The real `file_cx_issue` + escalate path is built
and unit-tested even though the demo drives it via the seeder.

### 4. The `cx_issues` data model

New DDL (`src/core/tables/10_cx.sql`), written by personal managers, read by `cxm`:

```
enums: cx_issue_category (onboarding | declines_friction | fees |
         rail_experience | app_ux | feature_request | other)
       cx_issue_severity (low | medium | high | urgent)
       cx_issue_status   (open | acknowledged | resolved)

cx_issues(
  id           uuid primary key default gen_random_uuid(),
  customer_id  uuid not null references customers(id),
  category     cx_issue_category not null,
  severity     cx_issue_severity not null,
  summary      text not null,
  detail       text,
  status       cx_issue_status not null default 'open',
  source       text not null default 'personal_manager',
  created_at   timestamptz not null default now(),
  resolved_at  timestamptz
)
+ indexes on (status), (severity), (category), (created_at)
```

## Demo — `demos/09-cxo/`

A narrated CX arc (reusing the `csuite` console pattern), driven over HTTP against
the deployed CXO. Because behavioural metrics need bank activity, demo setup reuses
the existing `testing/` generator + rail simulators so adoption/friction/engagement
have signal, alongside the `cx_issues` seeder.

Beats:
1. **Grounded CX posture** — `cx_summary` + adoption + friction + engagement; every
   figure grounded.
2. **Derived figure (compute)** — e.g. the unclaimed-e-Transfer rate as a %.
3. **The customer voice** — `issue_summary` + `notable_issues`; top complaint themes.
4. **Urgent escalation** — a scripted PM ping hits `/escalations`; the CXO surfaces
   it, re-grounded from `cx_issues`.
5. **Ranked feature backlog** — the signature grounded output.
6. **Scope discipline + memory** — a P&L / reliability question → defers to
   CFO / CTO; plus a durable CX note recorded and recalled in a fresh thread.

The CXO has no acting levers, so — unlike the CTO demo — the tamper-evident
`agent_action_ledger` is not central here; the demo emphasises grounding, the
customer voice, the escalation, and the backlog.

## Testing

- **Metric functions** — grounded numbers over a seeded test DB: each endpoint's
  pure function returns the correct aggregates for a known fixture.
- **Seeder** — deterministic; produces a known `cx_issues` distribution that
  `issue_summary` / `notable_issues` assert against.
- **PM `file_cx_issue` + escalate** — the write path, and the best-effort A2A
  escalate (the HTTP call stubbed) fires only for `high`/`urgent`.
- **CXO `/escalations` intake + re-grounding** — the ping is recorded and the CXO
  reads the issue's grounded details rather than the payload.
- **CXO agent (offline, fake metrics client)** — grounding (every quoted figure
  matches a tool output), lane discipline (defers P&L/reliability), backlog shape.
- **One live smoke** — seed → CX review → grounded posture + backlog; fire an
  escalation → surfaced.

## Ports & names

| Component | Dir | Port |
|---|---|---|
| metrics-&-surveys agent | `cxm/` | 8097 |
| CXO agent | `cxo/` | 8098 |

(Next free after `platform_mcp` 8094, `cto` 8095, `coder` 8096.)

## Phase line

- **Phase 1 (this spec):** `cxm` (behavioural metrics + `cx_issues` aggregates +
  structured endpoints + `/ask`), `cxo` (analyst + backlog + escalation intake),
  the `cx_issues` table + seeder + the PM write contract, `demos/09-cxo`, and tests.
- **Phase 2 (next spec):** surveys/NPS/CSAT tables + a campaign runner + simulated
  respondents in `cxm`; survey metric endpoints; the CXO folds survey signal into
  the backlog. Optional: live personal-manager end-to-end (deploy the PM and drive
  a real complaint through `file_cx_issue` → escalate → CXO).

## Open provisioning steps (one-time)

1. Load `src/core/tables/10_cx.sql` into the bank DB (the Kind init Job / migration).
2. Add `cxm` + `cxo` to the deploy scripts and the agent namespace secrets
   (reuse the shared Ollama key the other seats use; no new API keys).
3. Wire the PM's `cxo_url` for the escalation A2A (only needed for live-PM Phase 2).
