# Agent CEO — Phase 1: synthesizer seat + direct-officers, chairing a C-suite meeting

**Status:** design approved 2026-08-17 (brainstorming). Adds the capstone sixth
C-suite seat to the estate (CFO/COO/CTO/CXO already live). Branches off
`agent-cxo` (PR #77, unmerged at spec time).

## Goal

Give nano-bank a **Chief Executive Officer** agent — the capstone seat that does
not hold domain data of its own. It **consults** the four officers, **synthesizes**
a grounded cross-functional executive brief, and can **direct** the two *acting*
officers (COO, CTO) to act through their own audited levers. Every figure in the
brief is attributed to the officer who reported it; the CEO never invents one.

The signature output is a grounded **executive brief**: a cross-functional
synthesis (finance / ops / platform / CX, each figure attributed to its officer),
the top cross-functional priorities and risks, and any directive taken with its
*verified* outcome.

This phase also introduces the shared **`csuite/collab.py`** consult/direct
primitive (board-vision item #1), wired into the CEO first and reusable by the
Phase-2 board orchestrator.

## Non-goals (Phase 1)

- **No `board/` orchestrator.** Scheduled convening and chair-led autonomous
  deliberate-and-act across all seats is **Phase 2** (resuming the parked "T3").
  Phase 1 is a single CEO seat driven by a request (or the demo script).
- **No new acting levers.** The CEO acts only by *directing* an officer whose lever
  already exists. It adds no MCP tool that mutates bank state directly.
- **No bank/Rust change.** The CEO writes only its own directive rows to the
  existing `agent_action_ledger` via `append_agent_action` (psycopg2, the
  `finance/db.py` pattern). No schema change, no core change.
- **CFO and CXO stay consult-only.** They are analyst seats with no levers; the CEO
  can consult them but cannot direct them.
- **No guardrail bypass.** A directive is an imperative to the officer's *own*
  agent, which self-verifies and may refuse. The CEO reports honestly whether a
  lever actually fired; it never reaches around an officer's judgment.

## Decisions (locked in brainstorming)

| Question | Decision |
|---|---|
| Phase 1 scope | **Synthesizer + direct officers** — a `ceo/` seat that consults all four officers, produces a grounded executive brief, and can direct the two acting seats. The `board/` orchestrator is Phase 2. |
| Direct mechanism | **Imperative via `/ask`** — `direct_<peer>` POSTs an imperative to the peer's existing `/ask`; the peer's own agent self-verifies and acts via its **existing** audited lever (officer ledger row `actor=coo|cto`). The CEO also writes a CEO-level directive row `append_agent_action('ceo','direct_<peer>',…)`. |
| Directable seats | **COO + CTO only** (the acting seats). CFO + CXO are **consult-only**. |
| Directive verification | **Read-back verify (Option B)** — `direct_<peer>` snapshots `MAX(seq) WHERE actor=<peer>` before POSTing, then reads rows with `seq > snapshot AND actor=<peer>` after. A new row ⇒ the officer acted (capture its `{seq, action, effect}`); no new row ⇒ the officer only deliberated/refused. This distinction is the honest core of the CEO directive row. |
| Safety on a directive | The **officer's** lever refuses if unwarranted; the CEO bypasses no guardrail. Directs autonomously (board vision: no human in the loop; human gate optional). |
| Grounding | CEO domain figures come **only** from consult-tool outputs, attributed; a `ceo/claims.py` phantom-concept guard flags invented finance/ops/platform/CX figures. |
| Demo | A **C-suite meeting** — the CEO chairs a board meeting: round-the-table consults → synthesis → a directive decision → verified minutes. |
| Branch | Off **`agent-cxo`** (PR #77, unmerged). |

## Architecture

```
                        ┌──────────────────────────────────────────────┐
                        │            CEO agent   ceo/  :8099            │
                        │  · /ask, /ask/stream (executive brief)        │
                        │  · harness (plan/todos/memory/subagent)       │
                        │  · claims-lane guard: relay & attribute,      │
                        │    never invent a domain figure               │
                        └───────────────┬──────────────────────────────┘
                                        │  tools from csuite/collab.py
        consult_* (relay)   ┌───────────┼───────────┐   direct_* (imperative → lever)
                            ▼           ▼           ▼
   ┌────────────┐   ┌────────────┐  ┌────────────┐  ┌────────────┐
   │  CFO :8089 │   │  COO :8093 │  │  CTO :8095 │  │  CXO :8098 │
   │  /ask      │   │  /ask +    │  │  /ask +    │  │  /ask      │
   │  analyst   │   │  ops lever │  │  coder/plat│  │  analyst   │
   └────────────┘   └─────┬──────┘  └─────┬──────┘  └────────────┘
     consult only         │ acts          │ acts       consult only
                          ▼               ▼
                 agent_action_ledger  (append-only, hash-chained)
                 actor=coo / actor=cto  ← officer's own row
                                        ▲
                 actor=ceo  direct_coo/direct_cto  ← CEO directive row
                 (params: directive+rationale; effect: read-back of officer row)
```

### Consult vs direct

- **`consult_<peer>(question)`** — POST `{"message": question}` to
  `http://<peer>:<port>/ask`; return `{"officer": "<peer>", "answer": "<grounded
  text>"}`. Pure relay: the CEO attributes the officer's figures, never
  recomputes or paraphrases them.
- **`direct_<peer>(directive)`** — POST the imperative to the same `/ask`; the
  peer's agent self-verifies and acts via its own MCP lever (COO ops-mcp, CTO
  coder/platform), producing an **officer** ledger row. `collab` then:
  1. (before POST) snapshots `before = MAX(seq) WHERE actor=<peer>`;
  2. (after POST) reads `new = rows WHERE seq > before AND actor=<peer>`;
  3. writes the **CEO** directive row via `ceo/audit.py`;
  4. returns `{"peer","directive","officer_acted": bool, "officer_row": {…}|null,
     "officer_response": "<relayed summary>"}`.

## Components

### `csuite/collab.py` (new, shared)

A tool factory — board-vision item #1 — wired into the CEO first, reusable by the
Phase-2 board.

- `consult_tool(peer, base_url)` → a LangChain `@tool consult_<peer>`.
- `direct_tool(peer, base_url, audit)` → a LangChain `@tool direct_<peer>` with the
  before/after read-back described above; `audit` is the injected
  `ceo/audit.py` writer (kept out of `collab` so the primitive stays reusable by
  seats with a different actor name).
- `build_tools(registry, audit)` where `registry = {"cfo": url, "coo": url, …,
  "directable": {"coo","cto"}}` → the full tool list (consults for all, directs for
  the directable set).

HTTP is `httpx` with a timeout; a peer being down surfaces as a tool error the
agent reports (it does not crash the brief). No peer credential — in-cluster
service-to-service, same trust model as the existing officer→MCP calls.

### `ceo/` seat (mirrors `cxo/`)

| File | Purpose |
|---|---|
| `config.py` | `Settings`: `api_port=8099`, `console_port=8511`, peer URLs (`cfo:8089`, `coo:8093`, `cto:8095`, `cxo:8098`), DB env, model, qdrant/memory namespace `ceo`. |
| `model_factory.py` | kimi-k2.6 via `nano-agent-secrets` (`OLLAMA_API_KEY`), mirroring the CXO. |
| `claims.py` | Phantom-concept guard: flags a domain **figure** in the brief that did not arrive through a consult tool (invented finance/ops/platform/CX numbers). Disclaimer-aware, cue-based, no LLM — mirrors `cxo/claims.py`. |
| `tools.py` | Builds the peer registry from `Settings` and calls `csuite.collab.build_tools(registry, audit)`. |
| `audit.py` | The **only** CEO writer. `direct(peer, params, effect) → SELECT seq, entry_hash FROM append_agent_action('ceo','direct_'||peer, %s::jsonb, %s::jsonb)`. psycopg2, `finance/db.py` pattern. |
| `agent.py` | `CEO_PROMPT` + `ask`/`ask_stream` over `csuite.runtime`, `agent="ceo"`, `claims_fn=ceo_claims.unsupported_claims`. |
| `api.py` | `/ask`, `/ask/stream`, `/livez`, `/health` (probes: ollama, peer reachability, qdrant) — mirrors `cxo/api.py`. |
| `api_main.py`, `Dockerfile`, `k8s/` | Deployment, mirroring the CXO. |
| `tests/` | Offline unit tests (below). |

### Directive row shape

```
actor  = 'ceo'
action = 'direct_coo' | 'direct_cto'
params = {"directive": "<imperative text>", "rationale": "<CEO's grounded why>"}
effect = {"officer_acted": true,
          "officer_row": {"seq": 4213, "action": "cut_aft_batch", "effect": {…}},
          "officer_response": "<relayed summary>"}
      # or {"officer_acted": false, "officer_response": "…refused / analysis-only…"}
```

### CEO prompt / lane

> *You are the CEO of nano-bank, a Canadian challenger bank. You do not hold domain
> data — you CONSULT your officers (CFO/COO/CTO/CXO via `consult_*`) and SYNTHESIZE
> a cross-functional executive brief. Every figure is attributed to the officer who
> reported it; you NEVER invent a number. You may DIRECT the two acting officers
> (`direct_coo`, `direct_cto`) with an imperative; they self-verify and act via
> their own audited levers — you bypass no guardrail, and you report honestly
> whether a lever actually fired. Consult-only seats: the CFO (the books) and the
> CXO (customer experience) have no levers; you cannot direct them. Use the
> harness: plan multi-step reviews, keep a todo list, recall memory before and
> record durable executive notes after. Your output is a grounded executive brief:
> a finance/ops/platform/CX synthesis (each figure attributed), the top
> cross-functional priorities and risks, and any directive taken with its verified
> outcome.*

## Demo — a C-suite meeting (`demos/10-ceo/`)

Stages the CEO **chairing a board meeting**, one script, rendered live in a
`present` console on **:8511** (mirroring `demos/09-cxo/present`):

1. **Call to order / agenda** — the CEO opens with the meeting's question
   (e.g. *"State of the bank — what needs attention this week?"*).
2. **Round the table** — `consult_cfo → consult_coo → consult_cto → consult_cxo`
   in turn; each officer's grounded answer streams into that officer's panel,
   attributed. The four seats reporting.
3. **CEO synthesis** — the CEO folds the four reports into the executive brief:
   top priorities/risks, each figure attributed to its officer.
4. **A decision** — from the synthesis the CEO issues one directive to an acting
   seat, `direct_coo` (e.g. *"cut the pending AFT batch"*) or `direct_cto`; the
   officer self-verifies and acts via its own lever.
5. **Verified minutes** — read-back shows the fresh `actor=coo` ledger row; the CEO
   records the meeting's decision as an `actor=ceo` `direct_coo` row citing the
   proven officer outcome. The console's final panel shows both rows side by side —
   the decision and the officer action it caused.

`run-demo.sh` seeds state (a pending AFT batch so the directive has something real
to act on), brings up the seat, and drives the beats with per-beat step-through.
A `--no-up` flag replays against an already-running estate, matching the CTO/CXO
demos.

## Testing

**Offline unit tests** (no cluster; mock peer `/ask` + a fake ledger):

- `csuite/collab.py`: `consult_<peer>` relays the mocked answer with attribution; a
  down peer surfaces as a tool error, not a crash.
- `direct_<peer>` read-back — **both** branches: (a) the mock ledger gains a
  `actor=<peer>` row with `seq > before` ⇒ `officer_acted=true` with the captured
  row; (b) no new row ⇒ `officer_acted=false`. The CEO directive row is written
  with the correct `actor='ceo'`, `action='direct_<peer>'`, and effect.
- `ceo/claims.py`: an invented finance/ops/platform/CX figure is flagged; an
  attributed figure that came through a consult is not; a disclaimer clears it.
- `ceo/audit.py`: the `append_agent_action('ceo',…)` call shape (against a stub
  cursor).

**Live e2e** (gated behind the cluster, like the CXO): driven by the demo script —
consult all four, one directive, assert a fresh `actor=coo` row and a matching
`actor=ceo` directive row via `verify_agent_ledger()` still returning NULL (chain
intact).

## Ports & estate

CEO API **:8099**, present console **:8511** — the planned slots in the estate
(cfo 8089 / ops-mcp 8092 / coo 8093 / platform-mcp 8094 / cto 8095 / coder 8096 /
cx-mcp 8097 / cxo 8098 / **ceo 8099**; consoles agent 8505 / cfo 8506 / coo 8507 /
cto 8509 / cxo 8510+8513 / **ceo 8511**). In-cluster peer URLs:
`http://cfo:8089`, `http://coo:8093`, `http://cto:8095`, `http://cxo:8098` (all
`/ask`). Reuses `nano-agent-secrets` (`OLLAMA_API_KEY`), model kimi-k2.6.

## Phase 2 (out of scope, noted)

The `board/` orchestrator — scheduled convening and chair-led autonomous
deliberate-and-act across all seats — resumes the parked "T3" (see
`csuite-board-vision`) and **reuses this exact `csuite/collab.py` primitive**. This
phase deliberately builds the CEO seat and the shared primitive first so the board
has both a chair and a table to convene.
