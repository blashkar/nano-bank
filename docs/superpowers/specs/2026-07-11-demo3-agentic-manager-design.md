# Design: Demo 3 — agentic manager console (multi-box chat + LangSmith-like traces)

**Date:** 2026-07-11
**Status:** approved
**Repo:** `nano-bank` (branch `agent-k8s-e2e` / PR #22)

Third of four demos. A Streamlit console that talks to the **personal manager**
(the Branch API, `:8086`) through several themed chat boxes — one per action type
— each with a LangSmith-like trace panel showing the manager's run (tool calls,
inputs/outputs, durations).

## Backend (agent) — two additions

### 1. `open_account` manager tool
The manager currently cannot open accounts (no such tool). Add an **immediate**
(not confirm-gated) MCP tool, mirroring `register_interac_recipient`:

```
open_account(account_type: str) -> dict   # "chequing" | "savings" | "credit_card"
  -> deps.bank.create_account(current_token(), {"account_type": account_type})
```
- Added to `LLM_TOOL_NAMES` so it's bound to the manager.
- Immediate, because opening an account is not money movement (same posture as
  registering a payee). Money movement stays two-phase confirm-gated.
- `agent/bank.py::create_account(token, payload)` already exists.

### 2. Run-trace surfacing (the LangSmith source)
A LangChain `BaseCallbackHandler` records ordered events during `assist()`:

```
class TraceRecorder(BaseCallbackHandler):
  on_chat_model_start -> event {kind: "model", name: <model>, ...}
  on_tool_start(serialized, input_str) -> open a {kind:"tool", name, input, t0}
  on_tool_end(output) -> close it with output + elapsed_ms
  on_tool_error(error) -> close it with error + elapsed_ms (ok=False)
  events() -> list[ {seq, kind, name, ok, elapsed_ms, input?, output?, error?} ]
```
- `assist()` creates one recorder per call and passes it via
  `config={"callbacks": [rec], "configurable": {"thread_id": ...}, ...}`; the
  existing final-answer + pending-action extraction is unchanged.
- `assist()`'s return dict gains **`trace: rec.events()`** alongside
  `answer` / `thread_id` / `pending_action`.
- The Branch API `POST /branch/clients/{cid}/message` already returns the
  `assist_fn` output verbatim, so `trace` flows through with **no API change**.
- Confirm (`/actions/{aid}/confirm`) stays as-is (returns the execute result); the
  demo renders that as a single trailing "executed" event client-side. No trace
  change on the confirm path.

Trace is best-effort/telemetry only — it never affects the answer or the
confirm-gate.

## Frontend — `demos/03-agentic-manager/`

Streamlit app pointed at the **Branch API** (`:8086`, bearer `BRANCH_SERVICE_TOKEN`).
Config via env: `DEMO_BRANCH_BASE` (default `http://localhost:8086`) and
`DEMO_BRANCH_TOKEN`.

- **Top bar — pick a client.** "Seed a demo client" (`POST /branch/seed`) →
  selectbox of `customer_id`s; show a profile + accounts snapshot
  (`GET /branch/clients/{cid}/{profile,accounts}`). Create-customer is **out of
  scope** — it precedes agent access.
- **Three action chat boxes, each with its own `thread_id`** (independent
  conversations, stored per box in session state):
  1. **Open account** — e.g. "open a savings account" (uses `open_account`).
  2. **Register Interac payee** — e.g. "register sam@example.ca as Sam".
  3. **Perform transactions** — deposit / transfer / Interac send; shows
     **Confirm / Cancel** when the manager returns a `pending_action`.
  Each box POSTs `{message, thread_id}`, stores the returned `thread_id` back,
  appends the turn to that box's transcript.
- **Per box: a LangSmith-like trace panel** rendering the last response's `trace`:
  each event as a card/row — icon by kind (🧠 model / 🔧 tool), tool name,
  collapsible **input** and **output** JSON, `elapsed_ms`, green/red by `ok` —
  then the final **answer** and any **pending action**. Reads like a run tree.

Layout: boxes stacked top-to-bottom (trace panels are wide); each box's trace in
an expander under its transcript.

## Deploy
Rebuild `agent-mcp` (new tool) + `agent-api` (assist trace); `kind load` + rollout.
Run the demo on `:8512` (LAN), pointed at a `svc/agent-api 8086` port-forward.

## Testing
- **Unit** (`agent/tests/`): `open_account` is in `LLM_TOOL_NAMES` and calls
  `bank.create_account` with the account_type (fake bank); `TraceRecorder`
  records a tool start→end as one event with `elapsed_ms` and `ok`, and a
  tool-error as `ok=False`.
- **Live**: seed a client → "open a savings account" (trace shows the
  `open_account` tool) → "register sam@example.ca" → a transaction propose +
  confirm — all three boxes render answers + traces.

## Out of scope
- Create-customer (precedes agent access; use seed / demos 1–2).
- Persisting traces across sessions, multi-operator use, streaming/token-level
  traces (event-level is enough for the panel).

## Relates to
- `docs/superpowers/specs/2026-07-07-personal-manager-design.md` (the manager +
  two-phase confirm reused here).
- `demos/README.md` (demo index; this is demo 3 of 4).
