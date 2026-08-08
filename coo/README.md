# Agent COO — nano-bank's Chief Operating Officer

The second C-suite agent after the CFO. Where the CFO owns the books
(profitability, RAROC, ratios), the **COO owns how the bank *moves*** —
throughput, settlement, exceptions, float and rail health. Phase 1 is an
**operational analyst**: read-only over the bank, it observes and recommends but
pulls no levers.

It is a thin LangGraph react agent (GLM-5.2 via Ollama) whose only domain tools
are the **operations MCP** (`operations/`, `:8092`), wrapped in a hand-rolled
**agentic harness**. All operational arithmetic lives in `operations/metrics.py`
as pure, tested functions — the model never computes a figure, and a
deterministic verifier grounds every number against a tool result (one revise
pass if something is ungrounded).

## The harness (`coo/harness/`)

Agent-agnostic — composed by one entry point, `assemble(model, domain_tools,
prompt, memory) -> (agent, log)` — so it can later be lifted into a shared
package and back-ported onto the CFO.

- **planning** (`write_plan` / `update_plan`) — an ordered plan held in graph
  state, surfaced in the trace, preserved across compaction.
- **todos** (`write_todos`) — a status-tracked checklist, likewise preserved.
- **subagents** (`spawn_subagent(task, tools)`) — a focused deep-dive with its
  own thread and a scoped tool subset; returns only a summary, so the child's
  tool chatter never enters the parent context. Depth-guarded.
- **context control** — a `pre_model_hook` that, past a token threshold,
  summarizes older messages into a rolling summary and drops them (plan, todos
  and summary always survive); dropped detail is parked in memory.
- **memory** — durable, per-agent semantic memory over Qdrant (namespaced), the
  agent recalls before answering and records durable notes after. **Best-effort:**
  if Qdrant is down the agent still answers from live tools.

## `ask()` contract

`ask(settings, message, thread_id?) -> {answer, thread_id, trace, verification}`.
`trace` merges tool/model steps with harness events (compaction, subagent
spawn/return, memory writes) into one ordered, auditable list.

## Run locally

Each subsystem has its own uv venv.

```bash
cd coo && uv venv --python 3.12 && . .venv/bin/activate && uv pip install -r requirements.txt

# bring up the data path (separate shells):
#   1. bank on :8081  (see repo README / CLAUDE.md; needs kind Postgres + a core)
#   2. operations MCP: NANO_BANK_API=http://localhost:8081 python -m operations.mcp_server
#   3. (optional) a Qdrant for memory

# the COO:
OLLAMA_API_KEY=… OPERATIONS_MCP_URL=http://localhost:8092/mcp python -m coo.api_main   # :8093
streamlit run coo/console.py                                                            # :8507
```

Tests run fully offline (fake LLM + fake MCP + in-memory Qdrant):

```bash
cd /path/to/nano-bank && python -m pytest coo/tests -q
```

Cross-backend live smoke (once per `CORE_BACKEND`): `./coo/verify-coo.sh`.

## Scope

- **Phase 1 (this):** read-only operational analyst + the full harness.
- **Phase 2 (later):** operational levers (accruals, sweeps, batch cuts, rate
  changes) as a separate confirm-gated, bounded writable surface.
- **Phase 3 (later):** C-suite meetings — `POST /ask` is the seam; the COO calls
  the CFO's `:8089/ask` and vice-versa.
- **Phase 7 (later):** extract the harness into a shared package; back-port onto
  the CFO.

No external product's names appear in this subsystem — the COO, the operations
MCP and the harness are nano-bank's own.
