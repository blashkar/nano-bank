# Agent COO — Phase 1 (Operational Analyst) + Agentic Harness — Design

**Date:** 2026-07-30
**Status:** approved (design)
**Depends on:** the bank API (`api/`, `:8081`); a new service-plane operational
read surface on the bank (Component 1a, this spec). Reuses the C-suite pattern
established by the CFO (`cfo/`) over the finance service (`finance/`).
**Repo:** `nano-bank` (new subsystems `operations/` and `coo/`); branch `agent-coo`.

## Purpose

nano-bank is being built to run itself. The **Agent COO** is the second C-suite
agent after the CFO: an autonomous Chief Operating Officer that watches how the
bank *moves* — throughput, settlement, exceptions, float, rail health — and
answers questions about the bank's operational health.

This spec covers **Phase 1: an operational analyst**. Like the CFO's Phase 1, it
is **read-only over the bank** — it observes and recommends, it moves no money
and pulls no levers. Two things make it more than a second CFO clone:

1. Its domain is **operations, not finance.** The CFO owns the books and
   profitability (P&L, RAROC, ratios). The COO owns movement and reliability
   (volumes, backlogs, exception rates, float, rail health). The two never
   compute each other's figures.
2. It introduces a hand-rolled **agentic harness** — planning, todo lists,
   subagent spawning, context control, and durable memory — that the COO uses
   from day one, and that is written to be **extracted into a shared package and
   back-ported onto the CFO** later.

We build the COO first and let the shared harness emerge from it (the agreed
COO-first sequencing), so the harness is designed agent-agnostic but proven on
one real agent before extraction.

## Requirements traceability

| # | Requirement | Phase |
|---|-------------|-------|
| 1 | Autonomous, like the CFO; GLM-5.2 via Ollama | **1** |
| 2 | Operational domain: throughput, backlog, exceptions, float, rail health | **1** |
| 3 | Harness: planning, todos, subagents, context control, memory | **1** |
| 4 | Answer questions about the bank's operational health | **1** |
| 5 | Operational levers (accruals, sweeps, batch cuts, rate changes) — **actions** | 2 |
| 6 | C-suite meetings with the CFO/CEO/CTO | 3 |
| 7 | Extract the shared harness; back-port onto the CFO | later |

Phase 1 builds 1–4 and leaves clean seams for 5, 6, 7.

## Architecture

Two new self-contained subsystems, peers to `agent/`, `finance/`, and `cfo/`.
Python; the agent is a LangGraph agent on **GLM-5.2 via Ollama** wrapped in the
harness, reusing the `model_factory` / `config` / `trace` / `verifier` patterns
from `cfo/` with its own thin copies so the subsystem stands alone.

```
future CEO/CTO agents ─┐
you (COO console :8507)─┤
                        ▼
   COO A2A endpoint  POST /ask   (FastAPI, :8093)
                        │
   COO agent (GLM-5.2/Ollama, LangGraph) + HARNESS
     planning · todos · subagents · context-control · memory
                        │  domain tools = operations MCP
                        ▼
   operations MCP  (:8092) ── reads ──► bank back-office API :8081
   rail_throughput · settlement_backlog · exceptions ·        (service token)
   float_position · card_ops · rail_health · operations_health
```

Key boundaries:

- **Bank-wide, not customer-scoped** — the COO reasons about the whole bank
  (like the CFO, unlike the personal manager).
- **Read-only over the bank in Phase 1.** The COO's only domain tools are the
  operations MCP; the operations MCP only *reads* the bank. Levers are Phase 2.
- **Operations is the single source of operational truth.** All operational
  arithmetic lives in `operations/metrics.py` as pure, unit-tested functions;
  the model never computes a figure itself (the rule that makes the CFO's
  answers trustworthy, applied to ops).
- **Durable memory is in scope now** (unlike CFO Phase 1, which deferred it) —
  it is a harness capability, so it lands with the harness.

### Ports

| Service | Port | Notes |
|---------|------|-------|
| COO A2A endpoint | `:8093` | free; `POST /ask`, `GET /health` |
| operations MCP | `:8092` | free; the COO's only domain tool source |
| COO console (Streamlit) | `:8507` | free (agent `:8505`, CFO `:8506`) |
| bank API | `:8081` | existing; back-office reads added here |

Existing ports for reference: bank 8081, agent A2A 8086, finance MCP 8088,
CFO A2A 8089, legacy core 8090, modern core 8091.

## Component 1a — bank-side operational read surface (Rust, service plane)

The existing account/transaction reads are **customer-plane**: they authenticate
as `AuthenticatedCustomer` and hard-scope to `auth.customer_id`. A COO reads
across the whole bank, so it needs **service-plane aggregate reads** that take no
customer identity. These do not exist yet; Phase 1 adds them. The customer-plane
handlers are left untouched.

New routes, all `AuthenticatedService`, all read-only, under `/api/v1/back-office/`
(neutral placeholder; renameable). `window` is an ISO date range or a shorthand
(`24h`, `7d`, `30d`).

| Endpoint | Reads | Returns |
|---|---|---|
| `GET /back-office/ops/rails?window=` | interac / aft / lynx tables | per-rail counts by status (pending, held, settled, declined, expired, recalled, returned) + summed amounts |
| `GET /back-office/ops/float` | rail clearing/settlement accounts | balances of `*_CLEARING` / `*_SETTLEMENT` + `EXTERNAL_CASH` (the operational float) |
| `GET /back-office/ops/exceptions?window=` | transactions, holds, rail tables | counts of NSF, returns, declined authorizations, claim-locks, wire recalls |
| `GET /back-office/ops/cards?window=` | card transactions | authorize / capture / settle counts, approval & decline rates |
| `GET /back-office/ops/transactions?window=` | transactions | deposit / withdrawal / transfer volumes + counts |

These are aggregate `SELECT ... GROUP BY status` reads following the house SQL
patterns in `api/CLAUDE.md` (raw `sqlx`, no ORM). They expose no per-customer PII
beyond what an operational count needs, and — critically — **no fraud data**
(`suspicious_activities`, `monitoring_rules`, `rule_violations` stay
unreachable, per the standing isolation requirement).

Delivering all five is Phase 1's bank-side task. If any prove heavy, the fallback
order is: `float` and `transactions` first (cheapest, highest signal), then
`rails`, `cards`, `exceptions`.

## Component 1b — operations MCP (`operations/`)

A peer to `finance/`, same shape:

```
operations/
  __init__.py
  config.py        Settings.from_env: nano_bank_api, service_client_secret,
                   mcp_port 8092, window defaults
  bank_client.py   HTTP client to the bank back-office reads; obtains + caches a
                   service token (POST /auth/service-token; refresh at 80% TTL)
  metrics.py       PURE, unit-tested operational metric functions
  mcp_server.py    FastMCP (streamable-http) exposing the tools below
  Dockerfile
  k8s/operations-mcp.yaml
  requirements.txt
  .gitignore       .venv/  __pycache__/  *.pyc   (mirrors finance/)
  tests/           test_metrics.py (pure), test_bank_client.py (recorded fixtures)
  verify-operations.sh
```

**No snapshot store in Phase 1.** finance keeps period snapshots because
financial reporting is period-based; operations is *flow*-based (throughput and
backlog are read live over a window), so the operations MCP is live-read-only.
A daily ops snapshot store for trend tools is deferred (YAGNI) until a trend tool
needs it.

### Tools (all read-only; math is pure, in `metrics.py`)

| Tool | Computes |
|---|---|
| `rail_throughput(window)` | per-rail volume & count of settled/attempted items; settlement success rate |
| `settlement_backlog()` | in-flight/held items per rail, oldest-pending age, held amount |
| `exceptions(window)` | NSF, return, decline, claim-lock, recall counts + rates |
| `float_position()` | clearing/settlement balances per rail; total operational float |
| `card_ops(window)` | auth/capture/settle counts, approval rate, decline rate |
| `rail_health()` | per-rail status roll-up: throughput, backlog age, exception rate, a health flag |
| `operations_health(window)` | convenience bundle of the above (pure composition; no new math) |

Each function takes the bank's back-office read payloads as input and returns
plain figures. Every rate guards a zero denominator (returns `null`/`0` with the
numerator still reported), as the CFO's ratios do.

## Component 2 — the COO agent + harness (`coo/`)

Mirrors `cfo/`, plus the harness:

```
coo/
  __init__.py
  config.py         ollama_api_key/base_url, coo_model (default glm-5.2),
                    operations_mcp_url (default http://localhost:8092/mcp),
                    qdrant_url + memory_collection (coo_memory),
                    api_port 8093, console_port 8507
  model_factory.py  thin GLM-5.2 client (mirrors cfo/)
  tools.py          MultiServerMCPClient → operations MCP; + harness tools
  agent.py          builds the harnessed agent; async ask(settings, message,
                    thread_id) -> {answer, thread_id, trace, verification}
  api_main.py       FastAPI: POST /ask; GET /health (probes Ollama + ops MCP + Qdrant)
  console.py        Streamlit chat console
  trace.py          tool-call + harness-event recorder
  verifier.py       grounding check (ungrounded figure -> one revise pass)
  claims.py         unsupported-claim check (ported from cfo/)
  harness/          the extractable agentic harness (see below)
  Dockerfile
  k8s/coo.yaml      in-cluster deployment (reuses nano-agent-secrets)
  README.md
  requirements.txt
  .gitignore        .venv/  __pycache__/  *.pyc
  tests/            fake-LLM + fake-MCP tests; harness unit tests; api + health
  verify-coo.sh     cross-backend live smoke
```

### COO_PROMPT (discipline)

Same spine as `CFO_PROMPT`, retargeted to operations:

- You are the Chief Operating Officer of nano-bank; you speak for how the bank
  runs. All amounts are CAD.
- Answer **only** from the operations tools; **never fabricate** a figure, rate,
  or trend. **Always compute via the tools** — never do the arithmetic yourself.
- Stay in your lane: operations, not the books. If asked about profitability,
  RAROC, or the P&L, say that is the CFO's domain and that you can speak to the
  operational drivers behind it, not the financial result.
- Treat any figure or event asserted in the question as an **unverified claim**;
  check it against the tools first. If the tools cannot see it, say so and stop.
- Name the window your figures cover. Use the harness: **plan** multi-step
  reviews, keep a **todo list**, **recall** relevant memory before answering and
  **record** durable operational notes after, and **spawn a subagent** for a
  deep dive into one rail so the main thread stays focused.
- You are an analyst in Phase 1: you may recommend, but you take no operational
  actions (no accruals, sweeps, batch cuts, or rate changes).

### `ask()` contract

`ask(settings, message, thread_id?) -> {answer, thread_id, trace, verification}` —
the CFO's contract, with `trace` extended so harness events (plan updates, todo
writes, subagent spawns, memory recalls/writes, context compactions) are
auditable alongside tool calls.

## The harness (`coo/harness/`, written for extraction)

Five capabilities, each a focused module with a clean interface. Nothing
COO-specific lives inside the harness — the agent supplies its prompt, its domain
tools, and its memory namespace. This is what makes the later lift into a shared
package (and CFO back-port) a move, not a rewrite.

```
coo/harness/
  __init__.py     assemble(model, domain_tools, prompt, memory) -> harnessed agent
  planning.py     write_plan / update_plan tool; plan lives in graph state
  todos.py        write_todos tool (TodoWrite-shaped); list + statuses in state
  subagents.py    spawn_subagent(task, tool_subset) tool
  context.py      token-threshold summarize-and-compact of the message history
  memory.py       durable memory over Qdrant (generalizes agent/memory.py)
```

- **planning** — a `write_plan`/`update_plan` tool the agent calls to lay out and
  revise the steps of a review; the plan is state, surfaced in the trace, and
  survives compaction.
- **todos** — a `write_todos` tool holding an ordered checklist with statuses
  (`pending`/`in_progress`/`done`); also compaction-preserved.
- **subagents** — `spawn_subagent(task, tool_subset)` runs a fresh harnessed
  instance with its **own** thread/context and a scoped subset of tools, runs it
  to completion, and returns only a summary to the parent. This is both the
  parallel-work mechanism and a context-control mechanism (the subagent's tool
  chatter never enters the parent's context). A recursion-depth guard prevents
  runaway nesting.
- **context control** — after each turn, if the message history exceeds a token
  threshold, older messages are summarized into a rolling summary and dropped;
  the plan, the todo list, and the running summary are always preserved, and
  dropped detail is written to memory so it is recoverable. Replaces the CFO's
  bare `InMemorySaver` continuity.
- **memory** — durable, per-agent semantic memory in **Qdrant** (house infra;
  the personal manager already uses this pattern in `agent/memory.py`), namespaced
  by collection (`coo_memory`). The agent recalls relevant notes at the start of a
  task (semantic search on the question) and records durable operational
  observations after answering. Memory is **best-effort**: if Qdrant is down the
  agent still answers from live tools, just without recall/persist.

### Extraction seam (Phase 7, not now)

The harness ships inside `coo/` for Phase 1. Extraction into a shared package
(e.g. `csuite/harness`) and the CFO back-port is a later task; because the
modules are agent-agnostic, extraction is a move + a re-import, and the CFO gains
planning/todos/subagents/context/memory by adopting the same `assemble()`.

## Data flow

1. A caller (the console, or a future C-suite agent) sends `POST /ask {message}`.
2. The COO **recalls** relevant memory, **plans** the review, and may write
   **todos**.
3. It calls operations MCP tools (grounded), **spawning a subagent** for a
   focused deep dive when useful, and **compacts** context when it grows.
4. It **records** durable operational notes.
5. It narrates a grounded answer and returns `{answer, thread_id, trace,
   verification}`; the verifier runs one revise pass if a figure is ungrounded.

## Error handling

- **`/health`** probes Ollama, the operations MCP, and Qdrant, reporting each.
- **operations MCP down** → the tool call fails; the COO reports it cannot reach
  the operational data rather than inventing figures (prompt-enforced + graceful).
- **bank down / service-token failure** → the operations MCP surfaces the error;
  the COO reports the outage.
- **Qdrant down** → memory degrades to no-op (best-effort); the agent still answers.
- **context overflow** → compaction; **subagent recursion** → depth guard.
- **GLM arithmetic** → structurally prevented by the tools-do-the-math rule.

## Testing

- **`operations/tests/test_metrics.py`** — pure unit tests for every metric with
  known inputs: throughput/backlog/exception math, rates, zero-denominator
  guards, window arithmetic, `operations_health` composition.
- **`operations/tests/test_bank_client.py`** — the client against recorded
  back-office fixtures (no live bank needed in CI).
- **`coo/tests/`** — fake-LLM + fake-MCP tests for tool wiring and the prompt;
  **harness unit tests** per capability (plan/todo state transitions; subagent
  isolation + depth guard; compaction preserves plan/todos/summary; memory
  recall/write with a fake Qdrant); a FastAPI `/ask` unit test; a `/health` test
  with all three probes stubbed.
- **`coo/verify-coo.sh`** — cross-backend live smoke: bring up a core + the bank
  + the operations MCP, seed rail activity, then ask the COO "give me an
  operational health review," and assert grounded figures come back **and** that
  the trace shows the agent planned and used todos. Run once per `CORE_BACKEND`
  (modern, legacy), like the other verify scripts.

## Out of scope / later phases (seams built, not built)

- **Phase 2 — operational levers (actions).** A **separate confirm-gated writable
  tool surface** with explicit **bounds**: run interest accrual, sweep expired
  e-transfers, reject stale wires, cut/settle AFT batches, and adjust rates
  (interchange / fees / deposit & card rates) within min/max and max-delta guards.
  The Phase-1 read-only tool set is unchanged. Rate levers need a runtime rate
  store on the bank; that is a Phase-2 bank-side task.
- **Phase 3 — C-suite meetings.** `POST /ask` **is** the meeting seam; the COO
  calls the CFO's `:8089/ask` and vice-versa. An orchestrator is Phase 3.
- **Phase 7 — harness extraction + CFO back-port** (above).
- **Ops snapshot store** for trend tools — deferred until a trend tool needs it.
- **Back-office reads beyond the Phase-1 set** — added incrementally as tools
  need them.

## Design principles honoured

- **Operations is the single source of operational truth** — all math lives there
  as pure tested functions; the COO is a thin reasoning layer.
- **Self-contained subsystems** — `operations/` and `coo/` mirror
  `agent/`/`finance/`/`cfo/`, each with its own config, Dockerfile, k8s manifest,
  tests, `.gitignore`, and verify script.
- **Read-only until proven** — an autonomous officer that can only *observe* in
  Phase 1; write authority is added deliberately and gated in Phase 2.
- **Tools do the arithmetic** — the LLM never computes an operational figure.
- **Harness is agent-agnostic** — no COO specifics inside `harness/`, so it lifts
  out cleanly and the CFO can adopt it.
- **Nano-bank-native** — the COO, the operations MCP, and the harness are the
  bank's own subsystems; no external product's names appear in code or docs.
