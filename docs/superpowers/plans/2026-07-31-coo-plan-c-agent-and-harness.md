# COO Plan C — the COO agent + the extractable harness (`coo/`) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A self-contained `coo/` subsystem — an autonomous, read-only **Agent COO** (GLM-5.2 via Ollama, LangGraph) whose only domain tools are the operations MCP (`:8092`, Plan B), wrapped in a hand-rolled, **agent-agnostic** agentic **harness** (planning, todo lists, subagent spawning, context control, durable Qdrant memory). Phase-1 scope per the design: an operational analyst that observes and recommends but pulls no levers. `POST /ask` A2A on `:8093`, Streamlit console on `:8507`, k8s manifest, cross-backend live smoke.

**Design source:** `docs/superpowers/specs/2026-07-30-agent-coo-phase1-and-harness-design.md` (Component 2 + "The harness"). This plan builds Requirements 1–4; it leaves clean seams for levers (Phase 2), C-suite meetings (Phase 3), and harness extraction + CFO back-port (Phase 7).

**Architecture:** Mirrors `cfo/` (thin react agent + `model_factory`/`trace`/`verifier`/`claims`/`api`/`console`), plus a new `coo/harness/` package. The harness is composed by one entry point — `assemble(model, domain_tools, prompt, memory) -> harnessed agent` — so nothing COO-specific lives inside `harness/` and the later lift into a shared package (and CFO back-port) is a move, not a rewrite. All operational arithmetic already lives in `operations/metrics.py` (pure, tested); the COO never computes a figure.

**Tech stack (verified against `agent/.venv`):** Python 3.12, `langgraph` 1.x (`create_react_agent` with `state_schema` / `pre_model_hook` / `checkpointer`; `Command` + `InjectedToolCallId` + `InjectedState` for state-writing tools), `langchain-openai` (GLM over the Ollama OpenAI-compat endpoint), `langchain-mcp-adapters` (`MultiServerMCPClient`), `qdrant-client` + `fastembed` (durable memory), `fastapi`/`uvicorn`, `streamlit`, `pytest`.

## Global constraints

- **Read-only over the bank.** The COO's only domain tools come from the operations MCP, which only reads. No lever/write tool exists in Phase 1. The prompt enforces the analyst stance; the tool set enforces it structurally.
- **Tools do the arithmetic.** The model never computes an operational figure; every number is grounded in an operations-MCP tool result, checked by a deterministic verifier with one revise pass (the CFO's proven pattern).
- **Harness is agent-agnostic.** No COO strings, no operations specifics inside `coo/harness/`. The agent supplies its prompt, its domain tools, and its memory namespace. Every harness module is unit-testable with a fake model / in-memory Qdrant — no live LLM in CI.
- **Best-effort memory.** If Qdrant is down, recall/record degrade to no-ops and the agent still answers from live tools.
- **Self-contained subsystem.** Own `config.py`, `model_factory.py`, `requirements.txt`, own `coo/.venv` (uv), nested `.gitignore` (`.venv/ __pycache__/ *.pyc`), `Dockerfile`, `k8s/coo.yaml`, `README.md`, `verify-coo.sh` — mirroring `agent/`/`finance/`/`cfo/`/`operations/`.
- **Nano-bank-native naming.** No external product's name appears in code or docs.
- **Pin the SDK lines that float.** As Plan B found, unpinned `mcp` floats to a 2.x line that breaks `mcp.server.fastmcp`; pin `mcp>=1.2,<2`. Mirror the versions `agent/.venv` actually resolves (langgraph 1.x, langchain-core 1.x).
- **Ports:** COO API `:8093`, ops MCP `:8092`, COO console `:8507` — all free.

## File structure

```
coo/
  __init__.py
  config.py            Settings.from_env (ollama, coo_model, operations_mcp_url,
                       qdrant_url + memory_collection, api_port 8093, console_port 8507,
                       harness knobs: context_token_threshold, subagent_max_depth)
  model_factory.py     GLM client (copy of cfo/model_factory.py, s/cfo/coo/)
  trace.py             tool + harness-event recorder (extends cfo/trace.py)
  verifier.py          numeric grounding + one revise pass (copy of cfo/verifier.py)
  claims.py            operational phantom-metric / window claim guard (retarget of cfo/claims.py)
  tools.py             operations MCP tools (MultiServerMCPClient)
  agent.py             COO_PROMPT + async ask(settings, message, thread_id)
  api.py               FastAPI create_app (POST /ask, GET /health with 3 probes)
  api_main.py          container entrypoint (resolve GLM, serve)
  console.py           Streamlit chat console
  harness/
    __init__.py        assemble(model, domain_tools, prompt, memory, **knobs)
    state.py           HarnessState (messages + plan + todos + running_summary + depth)
    memory.py          HarnessMemory (generalizes agent/memory.py) + SafeMemory + memory tools
    planning.py        write_plan / update_plan tools (Command-updating)
    todos.py           write_todos tool (Command-updating)
    context.py         estimate_tokens / compact (pure) + make_context_hook (pre_model_hook)
    subagents.py       make_spawn_tool(...) -> spawn_subagent tool (depth-guarded, tool-subset)
    events.py          HarnessLog: an ordered, JSON-safe harness-event sink
  Dockerfile
  k8s/coo.yaml
  requirements.txt
  .gitignore
  README.md
  verify-coo.sh
  tests/
    __init__.py
    fakes.py           FakeChatModel (scriptable tool-calls), fake ops tools, in-mem memory
    test_config.py
    test_memory.py
    test_planning_todos.py
    test_context.py
    test_subagents.py
    test_harness_assemble.py
    test_agent.py       fake-LLM + fake-MCP: grounding + revise pass + trace has harness events
    test_verifier.py    (copy of cfo/tests/test_verifier.py)
    test_claims.py      operational retarget
    test_api.py         /ask + /health (probes stubbed)
```

---

## Part 1 — the harness (`coo/harness/`)

Built first because it is the substance and it is agent-agnostic; the COO agent (Part 2) is largely a `cfo/` clone that consumes `assemble()`. Tasks 1–5 need **no live stack** (pure + fake-model + in-memory Qdrant).

### Task 1: `harness/state.py` + `harness/events.py` + `harness/memory.py`

**Files:**
- Create: `coo/harness/__init__.py` (empty for now), `coo/harness/state.py`, `coo/harness/events.py`, `coo/harness/memory.py`
- Create: `coo/harness/tests` is under `coo/tests/`; create `coo/__init__.py`, `coo/tests/__init__.py`, `coo/tests/fakes.py`, `coo/tests/test_memory.py`
- Create: `coo/requirements.txt`, `coo/.gitignore`

**Interfaces:**
- `HarnessState` — a `create_react_agent` state schema: `messages` (via `AgentState`) plus `plan: list[str]`, `todos: list[dict]`, `running_summary: str`, `depth: int`.
- `HarnessLog` — `.add(kind, **fields)` appends `{seq, t, kind, ...}`; `.events()` returns a copy. Records the non-tool harness events (compaction, subagent spawn/return).
- `HarnessMemory(client, collection, embed, namespace)` — agent-agnostic generalization of `agent/memory.py`'s `QdrantMemory`: keyed by `namespace` (the agent's memory collection identity) instead of `customer_id`. `.record(fact, kind, thread_id)`, `.recall(query, k)`. `.in_memory(collection, namespace)` / `.from_settings(settings)` constructors.
- `SafeMemory(inner|None)` — best-effort wrapper: every method swallows exceptions and returns `[]`/`None`; `SafeMemory(None)` is a total no-op (used when Qdrant is unreachable).

- [ ] **Step 1: `coo/requirements.txt`, `coo/.gitignore`, package markers**

`coo/requirements.txt`:
```
mcp>=1.2,<2
langgraph>=1,<2
langchain-core>=1,<2
langchain-openai>=1,<2
langchain-mcp-adapters>=0.3,<1
qdrant-client>=1.12,<2
fastembed>=0.4
fastapi>=0.115
uvicorn>=0.30
streamlit>=1.38
httpx>=0.27,<1
pytest>=8.0
```
`coo/.gitignore`:
```
.venv/
__pycache__/
*.pyc
```
Create empty `coo/__init__.py`, `coo/harness/__init__.py`, `coo/tests/__init__.py`.

- [ ] **Step 2: write the failing memory test** — `coo/tests/test_memory.py`

```python
from coo.harness.memory import HarnessMemory, SafeMemory


def test_record_then_recall_by_semantic_query():
    m = HarnessMemory.in_memory(collection="t_coo", namespace="coo")
    m.record("interac settlement backlog spiked on 2026-07-30", kind="observation")
    m.record("card decline rate is nominal", kind="observation")
    hits = m.recall("what happened with interac settlement?", k=1)
    assert hits and "interac" in hits[0].lower()


def test_namespace_isolates_agents():
    from qdrant_client import QdrantClient
    from coo.harness.memory import _embedder
    client = QdrantClient(":memory:")
    coo = HarnessMemory(client, "shared", _embedder(), namespace="coo")
    cfo = HarnessMemory(client, "shared", _embedder(), namespace="cfo")
    coo.record("coo note about float", kind="observation")
    cfo.record("cfo note about raroc", kind="observation")
    assert all("raroc" not in h.lower() for h in coo.recall("float", k=5))


def test_safe_memory_swallows_failures():
    class Boom:
        def recall(self, *a, **k): raise RuntimeError("qdrant down")
        def record(self, *a, **k): raise RuntimeError("qdrant down")
    safe = SafeMemory(Boom())
    assert safe.recall("x") == []      # no raise
    assert safe.record("y") is None    # no raise
    assert SafeMemory(None).recall("x") == []  # total no-op
```

- [ ] **Step 3: run to verify it fails** — `cd /home/bmartins/dev/nano-bank && python -m pytest coo/tests/test_memory.py -q` → `ModuleNotFoundError: No module named 'coo.harness.memory'`.

- [ ] **Step 4: implement**

`coo/harness/state.py`:
```python
from __future__ import annotations
from typing import Annotated
from langgraph.prebuilt.chat_agent_executor import AgentState


class HarnessState(AgentState):
    """create_react_agent state + harness fields. `messages`/`remaining_steps`
    come from AgentState; the rest are harness working memory that survives
    across turns (checkpointed) and across context compaction."""
    plan: list[str]
    todos: list[dict]          # {"content": str, "status": "pending|in_progress|done"}
    running_summary: str
    depth: int                 # subagent nesting depth; 0 at the top level
```

`coo/harness/events.py`:
```python
from __future__ import annotations
import time


class HarnessLog:
    """An ordered, JSON-safe sink for harness events that are NOT tool calls
    (compaction, subagent spawn/return). Tool-shaped harness capabilities
    (plan/todo/memory tools) already surface through the TraceRecorder; this
    captures the rest so the full run stays auditable."""

    def __init__(self):
        self._events: list[dict] = []

    def add(self, kind: str, **fields) -> None:
        self._events.append({"seq": len(self._events), "t": time.time(),
                             "kind": kind, **fields})

    def events(self) -> list[dict]:
        return list(self._events)
```

`coo/harness/memory.py` (generalizes `agent/memory.py`; `namespace` replaces `customer_id`; adds `SafeMemory`):
```python
"""Durable, per-agent semantic memory over Qdrant (fastembed/CPU embeddings).
Agent-agnostic: a `namespace` (e.g. "coo") scopes an agent's notes so several
C-suite agents can share one Qdrant. Generalizes agent/memory.py's QdrantMemory
(which scoped by customer_id). Best-effort via SafeMemory."""
from __future__ import annotations
import time
import uuid
from typing import Optional

from qdrant_client import QdrantClient, models


def _embedder():
    from fastembed import TextEmbedding
    return TextEmbedding()  # small default CPU model


class HarnessMemory:
    def __init__(self, client: QdrantClient, collection: str, embed, *, namespace: str):
        self.client = client
        self.collection = collection
        self.namespace = namespace
        self._embed = embed
        self._dim = len(next(iter(embed.embed(["dim probe"]))))
        if not client.collection_exists(collection):
            client.create_collection(
                collection,
                vectors_config=models.VectorParams(size=self._dim, distance=models.Distance.COSINE))

    @classmethod
    def in_memory(cls, collection: str = "coo_memory", namespace: str = "coo") -> "HarnessMemory":
        return cls(QdrantClient(":memory:"), collection, _embedder(), namespace=namespace)

    @classmethod
    def from_settings(cls, settings) -> "HarnessMemory":
        return cls(QdrantClient(url=settings.qdrant_url), settings.memory_collection,
                   _embedder(), namespace=settings.memory_namespace)

    def _vec(self, text: str):
        return list(next(iter(self._embed.embed([text]))))

    def _filter(self):
        return models.Filter(must=[models.FieldCondition(
            key="namespace", match=models.MatchValue(value=self.namespace))])

    def record(self, fact: str, *, kind: str = "observation",
               thread_id: Optional[str] = None) -> str:
        pid = uuid.uuid4().hex
        self.client.upsert(self.collection, points=[models.PointStruct(
            id=pid, vector=self._vec(fact),
            payload={"namespace": self.namespace, "kind": kind, "fact": fact,
                     "thread_id": thread_id, "ts": time.time()})])
        return pid

    def recall(self, query: str, k: int = 3) -> list[str]:
        hits = self.client.query_points(self.collection, query=self._vec(query),
                                        limit=k, query_filter=self._filter()).points
        return [h.payload["fact"] for h in hits]


class SafeMemory:
    """Best-effort wrapper: memory is an enhancement, never a dependency. If the
    inner store is None or raises, recall yields [] and record is a no-op, so the
    agent still answers from live tools."""

    def __init__(self, inner: Optional[HarnessMemory]):
        self._inner = inner

    def recall(self, query: str, k: int = 3) -> list[str]:
        if self._inner is None:
            return []
        try:
            return self._inner.recall(query, k)
        except Exception:  # noqa: BLE001
            return []

    def record(self, fact: str, *, kind: str = "observation",
               thread_id: Optional[str] = None):
        if self._inner is None:
            return None
        try:
            return self._inner.record(fact, kind=kind, thread_id=thread_id)
        except Exception:  # noqa: BLE001
            return None
```

- [ ] **Step 5: run to pass** — `python -m pytest coo/tests/test_memory.py -q` → 3 passed. (First run downloads the fastembed model; allow time / network.)

- [ ] **Step 6: commit** — `git add coo/__init__.py coo/harness/__init__.py coo/harness/state.py coo/harness/events.py coo/harness/memory.py coo/tests/__init__.py coo/tests/test_memory.py coo/requirements.txt coo/.gitignore && git commit -m "feat(coo): harness state, event log, agent-agnostic Qdrant memory + tests"`

---

### Task 2: `harness/planning.py` + `harness/todos.py` (state-writing tools)

**Files:** Create `coo/harness/planning.py`, `coo/harness/todos.py`, `coo/tests/test_planning_todos.py`.

**Interfaces:**
- `planning_tools() -> list` → `[write_plan, update_plan]`. `write_plan(steps: list[str])` sets `state["plan"]`; `update_plan(steps)` replaces it (revision). Each returns a `Command` that both updates state and emits a confirming `ToolMessage`.
- `todo_tools() -> list` → `[write_todos]`. `write_todos(todos: list[dict])` sets `state["todos"]` (each `{content, status}`), validating `status ∈ {pending,in_progress,done}`.

- [ ] **Step 1: write the failing test** — `coo/tests/test_planning_todos.py`

```python
from langchain_core.messages import ToolMessage
from coo.harness.planning import planning_tools
from coo.harness.todos import todo_tools


def _by_name(tools):
    return {t.name: t for t in tools}


def test_write_plan_updates_state_and_confirms():
    write_plan = _by_name(planning_tools())["write_plan"]
    cmd = write_plan.invoke({"steps": ["read float", "read rails", "summarize"],
                             "tool_call_id": "c1"})
    assert cmd.update["plan"] == ["read float", "read rails", "summarize"]
    msgs = cmd.update["messages"]
    assert isinstance(msgs[0], ToolMessage) and msgs[0].tool_call_id == "c1"


def test_write_todos_validates_status():
    write_todos = _by_name(todo_tools())["write_todos"]
    cmd = write_todos.invoke({"todos": [{"content": "check interac", "status": "pending"}],
                              "tool_call_id": "c2"})
    assert cmd.update["todos"][0]["status"] == "pending"
    import pytest
    with pytest.raises(ValueError):
        write_todos.invoke({"todos": [{"content": "x", "status": "bogus"}],
                            "tool_call_id": "c3"})
```

- [ ] **Step 2: run to verify it fails.**

- [ ] **Step 3: implement**

`coo/harness/planning.py`:
```python
"""Planning tools: the agent lays out and revises the steps of a review. The
plan is graph state (HarnessState.plan), surfaced in the trace and preserved
across context compaction. Agent-agnostic."""
from __future__ import annotations
from typing import Annotated
from langchain_core.tools import tool, InjectedToolCallId
from langchain_core.messages import ToolMessage
from langgraph.types import Command


def _set_plan(steps: list[str], tool_call_id: str, verb: str) -> Command:
    return Command(update={
        "plan": list(steps),
        "messages": [ToolMessage(f"Plan {verb} ({len(steps)} steps).",
                                 tool_call_id=tool_call_id)],
    })


def planning_tools() -> list:
    @tool
    def write_plan(steps: list[str],
                   tool_call_id: Annotated[str, InjectedToolCallId]) -> Command:
        """Record an ordered plan for a multi-step review. Call this first on any
        non-trivial question; each step is a short phrase."""
        return _set_plan(steps, tool_call_id, "recorded")

    @tool
    def update_plan(steps: list[str],
                    tool_call_id: Annotated[str, InjectedToolCallId]) -> Command:
        """Replace the current plan with a revised list of steps."""
        return _set_plan(steps, tool_call_id, "revised")

    return [write_plan, update_plan]
```

`coo/harness/todos.py`:
```python
"""Todo tool (TodoWrite-shaped): an ordered checklist with statuses, held in
HarnessState.todos and preserved across compaction. Agent-agnostic."""
from __future__ import annotations
from typing import Annotated
from langchain_core.tools import tool, InjectedToolCallId
from langchain_core.messages import ToolMessage
from langgraph.types import Command

_STATUSES = {"pending", "in_progress", "done"}


def todo_tools() -> list:
    @tool
    def write_todos(todos: list[dict],
                    tool_call_id: Annotated[str, InjectedToolCallId]) -> Command:
        """Record/replace the working checklist. Each item is
        {"content": str, "status": "pending"|"in_progress"|"done"}."""
        cleaned = []
        for t in todos:
            status = t.get("status", "pending")
            if status not in _STATUSES:
                raise ValueError(f"bad todo status: {status!r}")
            cleaned.append({"content": t["content"], "status": status})
        done = sum(1 for t in cleaned if t["status"] == "done")
        return Command(update={
            "todos": cleaned,
            "messages": [ToolMessage(f"Todos updated ({done}/{len(cleaned)} done).",
                                     tool_call_id=tool_call_id)],
        })

    return [write_todos]
```

- [ ] **Step 4: run to pass** (2 passed). **Step 5: commit** — `feat(coo): planning + todo harness tools (state-writing) + tests`.

---

### Task 3: `harness/context.py` (summarize-and-compact)

**Files:** Create `coo/harness/context.py`, `coo/tests/test_context.py`.

**Interfaces:**
- `estimate_tokens(messages) -> int` — cheap char/4 heuristic (no tokenizer dependency).
- `compact(messages, *, threshold, summarize_fn, keep_last) -> CompactResult` — **pure**. If `estimate_tokens(messages) <= threshold`, returns unchanged (`compacted=False`). Else it summarizes all but the last `keep_last` messages via the injected `summarize_fn(msgs) -> str`, and returns the messages to drop + the new rolling summary. No LangGraph, no LLM — `summarize_fn` is injectable so the test drives it with a stub.
- `make_context_hook(*, threshold, summarizer, memory, log, keep_last=6)` — returns a `pre_model_hook(state)` closure for `create_react_agent`. It calls `compact`; when compaction happens it (a) returns a state update dropping the old messages and prepending a rolling-summary `SystemMessage`, updating `running_summary`; (b) best-effort `memory.record`s the dropped detail so it's recoverable; (c) `log.add("compaction", dropped=n, ...)`. `summarizer` is the model (or any callable) used to build the summary text.

- [ ] **Step 1: write the failing test** — `coo/tests/test_context.py`

```python
from langchain_core.messages import HumanMessage, AIMessage
from coo.harness.context import estimate_tokens, compact


def _big(n):
    return [HumanMessage("x" * 4000) if i % 2 == 0 else AIMessage("y" * 4000)
            for i in range(n)]


def test_below_threshold_is_untouched():
    msgs = [HumanMessage("hi"), AIMessage("hello")]
    res = compact(msgs, threshold=10_000, summarize_fn=lambda m: "S", keep_last=6)
    assert res.compacted is False
    assert res.kept == msgs and res.dropped == []


def test_over_threshold_summarizes_all_but_last_k():
    msgs = _big(20)
    res = compact(msgs, threshold=5_000, summarize_fn=lambda m: "ROLLED", keep_last=6)
    assert res.compacted is True
    assert res.summary == "ROLLED"
    assert len(res.kept) == 6                       # only the tail kept
    assert len(res.dropped) == 14                   # the rest summarized away
    assert estimate_tokens(msgs) > 5_000
```

- [ ] **Step 2: run to verify it fails.**

- [ ] **Step 3: implement** — `coo/harness/context.py`

```python
"""Context control: when the message history grows past a token threshold, older
messages are summarized into a rolling summary and dropped; the plan, todos, and
summary always survive (they live in HarnessState, not the message list), and the
dropped detail is written to memory so it stays recoverable. Agent-agnostic.

`compact` is pure (summarize_fn injected) so the policy is unit-tested without an
LLM; `make_context_hook` wires it as a create_react_agent pre_model_hook."""
from __future__ import annotations
from dataclasses import dataclass, field

from langchain_core.messages import SystemMessage, RemoveMessage
from langgraph.graph.message import REMOVE_ALL_MESSAGES


def estimate_tokens(messages) -> int:
    return sum(len(str(getattr(m, "content", m))) for m in messages) // 4


@dataclass
class CompactResult:
    compacted: bool
    kept: list = field(default_factory=list)
    dropped: list = field(default_factory=list)
    summary: str = ""


def compact(messages, *, threshold: int, summarize_fn, keep_last: int) -> CompactResult:
    if estimate_tokens(messages) <= threshold or len(messages) <= keep_last:
        return CompactResult(compacted=False, kept=list(messages))
    head, tail = messages[:-keep_last], messages[-keep_last:]
    return CompactResult(compacted=True, kept=list(tail), dropped=list(head),
                         summary=summarize_fn(head))


_SUMMARY_INSTRUCTION = (
    "Summarize the operational review so far in <=8 terse bullet points: the "
    "question, figures already obtained (with their window), and open threads. "
    "Facts only.")


def make_context_hook(*, threshold: int, summarizer, memory, log, keep_last: int = 6):
    def _summarize(head) -> str:
        try:
            resp = summarizer.invoke(
                [SystemMessage(_SUMMARY_INSTRUCTION), *head])
            return resp.content if hasattr(resp, "content") else str(resp)
        except Exception:  # noqa: BLE001
            return "(summary unavailable)"

    def pre_model_hook(state) -> dict:
        msgs = state["messages"]
        res = compact(msgs, threshold=threshold, summarize_fn=_summarize,
                      keep_last=keep_last)
        if not res.compacted:
            return {}
        prior = state.get("running_summary") or ""
        rolling = (prior + "\n" + res.summary).strip()
        # best-effort: park the dropped detail in durable memory
        memory.record("compacted context: " + res.summary, kind="context",
                      thread_id=state.get("thread_id"))
        log.add("compaction", dropped=len(res.dropped),
                kept=len(res.kept), summary_chars=len(res.summary))
        # Persisted mutation: drop everything, re-seed with a summary system
        # message + the recent tail. plan/todos/running_summary are separate
        # state keys and are untouched.
        new_messages = [RemoveMessage(id=REMOVE_ALL_MESSAGES),
                        SystemMessage("Rolling summary of earlier context:\n" + rolling),
                        *res.kept]
        return {"messages": new_messages, "running_summary": rolling}

    return pre_model_hook
```

- [ ] **Step 4: run to pass** (3 passed). **Step 5: commit** — `feat(coo): context-control harness (pure compact + pre_model_hook) + tests`.

---

### Task 4: `harness/subagents.py` (spawn_subagent, depth-guarded)

**Files:** Create `coo/harness/subagents.py`, `coo/tests/fakes.py` (if not already), `coo/tests/test_subagents.py`.

**Interfaces:**
- `FakeChatModel` (in `coo/tests/fakes.py`) — a minimal LangChain chat model that plays a scripted list of turns (each turn either a tool call or a final text), so agents run without a live LLM. Also `fake_ops_tools()` returning a couple of `@tool`s that return canned figures.
- `make_spawn_tool(*, build_agent, tools_by_name, log, max_depth) -> spawn_subagent` — a factory. `build_agent(tool_subset, depth)` builds a fresh harnessed agent (Task 5 supplies the real one; the test supplies a fake). `spawn_subagent(task: str, tools: list[str])` reads `depth` from state (`InjectedState`); if `depth >= max_depth` it refuses (returns a message, no spawn); else it builds a fresh agent over the named tool subset at `depth+1`, runs it to completion on its **own** thread, `log.add("subagent", ...)`, and returns only the subagent's final text to the parent (the subagent's tool chatter never enters the parent context).

- [ ] **Step 1: write the failing test** — `coo/tests/test_subagents.py`

```python
import asyncio
from langgraph.prebuilt import create_react_agent
from langgraph.checkpoint.memory import InMemorySaver
from coo.harness.state import HarnessState
from coo.harness.events import HarnessLog
from coo.harness.subagents import make_spawn_tool
from coo.tests.fakes import FakeChatModel, fake_ops_tools


def _build_agent(tool_subset, depth):
    model = FakeChatModel([{"text": "interac float is 700.00 (deep dive done)"}])
    return create_react_agent(model, tool_subset, state_schema=HarnessState,
                              checkpointer=InMemorySaver())


def test_spawn_runs_child_and_returns_summary():
    log = HarnessLog()
    tools = {t.name: t for t in fake_ops_tools()}
    spawn = make_spawn_tool(build_agent=_build_agent, tools_by_name=tools,
                            log=log, max_depth=2)
    out = spawn.invoke({"task": "deep dive interac", "tools": list(tools),
                        "state": {"depth": 0}})
    assert "deep dive done" in out
    assert any(e["kind"] == "subagent" for e in log.events())


def test_depth_guard_refuses_at_max():
    log = HarnessLog()
    spawn = make_spawn_tool(build_agent=_build_agent, tools_by_name={},
                            log=log, max_depth=2)
    out = spawn.invoke({"task": "x", "tools": [], "state": {"depth": 2}})
    assert "depth" in out.lower()
    assert not any(e["kind"] == "subagent" for e in log.events())
```

- [ ] **Step 2: run to verify it fails.**

- [ ] **Step 3: implement**

`coo/harness/subagents.py`:
```python
"""Subagent spawning: run a fresh harnessed agent, with its OWN thread/context
and a scoped subset of tools, to completion — returning only a summary to the
parent. This is both the parallel-work mechanism and a context-control mechanism
(the child's tool chatter never enters the parent's context). A depth guard stops
runaway nesting. Agent-agnostic: `build_agent(tool_subset, depth)` is injected."""
from __future__ import annotations
import asyncio
import uuid
from typing import Annotated

from langchain_core.tools import tool
from langchain_core.messages import HumanMessage, AIMessage
from langgraph.prebuilt import InjectedState


def _last_text(state) -> str:
    for m in reversed(state["messages"]):
        if isinstance(m, AIMessage) and (m.content or "").strip():
            return m.content
    return "(subagent produced no answer)"


def make_spawn_tool(*, build_agent, tools_by_name: dict, log, max_depth: int):
    @tool
    def spawn_subagent(task: str, tools: list[str],
                       state: Annotated[dict, InjectedState]) -> str:
        """Delegate a focused deep-dive to a subagent with its own context and a
        subset of your tools (by name). Returns only the subagent's summary. Use
        it to keep the main thread focused (e.g. one rail at a time)."""
        depth = int(state.get("depth", 0))
        if depth >= max_depth:
            return (f"Refused: subagent depth limit ({max_depth}) reached; do "
                    "this inline instead.")
        subset = [tools_by_name[n] for n in tools if n in tools_by_name]
        agent = build_agent(subset, depth + 1)
        thread = f"sub-{uuid.uuid4().hex[:6]}"
        cfg = {"configurable": {"thread_id": thread}, "recursion_limit": 30}
        init = {"messages": [HumanMessage(task)], "plan": [], "todos": [],
                "running_summary": "", "depth": depth + 1}
        out = asyncio.run(agent.ainvoke(init, config=cfg))
        summary = _last_text(out)
        log.add("subagent", task=task[:200], tools=list(tools),
                depth=depth + 1, thread=thread, chars=len(summary))
        return summary

    return spawn_subagent
```

`coo/tests/fakes.py`:
```python
"""Test doubles: a scriptable chat model and canned ops tools so harness + agent
tests run with no live LLM / MCP."""
from __future__ import annotations
from typing import Optional
from langchain_core.language_models.chat_models import BaseChatModel
from langchain_core.messages import AIMessage
from langchain_core.tools import tool


class FakeChatModel(BaseChatModel):
    """Plays a scripted list of turns. Each turn is either
    {"tool": name, "args": {...}} (emit a tool call) or {"text": "..."} (final)."""
    script: list
    i: int = 0

    def __init__(self, script, **kw):
        super().__init__(script=script, **kw)

    @property
    def _llm_type(self) -> str:
        return "fake"

    def _generate(self, messages, stop=None, run_manager=None, **kw):
        from langchain_core.outputs import ChatGeneration, ChatResult
        turn = self.script[min(self.i, len(self.script) - 1)]
        self.i += 1
        if "tool" in turn:
            msg = AIMessage(content="", tool_calls=[{
                "name": turn["tool"], "args": turn.get("args", {}),
                "id": f"call{self.i}"}])
        else:
            msg = AIMessage(content=turn["text"])
        return ChatResult(generations=[ChatGeneration(message=msg)])

    def bind_tools(self, tools, **kw):
        return self


def fake_ops_tools() -> list:
    @tool
    def float_position() -> dict:
        """Canned float."""
        return {"total_float": "700.00", "by_system": {"interac": "700.00"}}

    @tool
    def rails(window: str = "24h") -> dict:
        """Canned rails."""
        return {"window": window, "by_rail": {"interac": {"total_count": 7}}}

    return [float_position, rails]
```

- [ ] **Step 4: run to pass** (2 passed). **Step 5: commit** — `feat(coo): subagent spawning (depth-guarded, tool-subset, isolated context) + fakes + tests`.

---

### Task 5: `harness/__init__.py` — `assemble()` (wire it all) + integration test

**Files:** Edit `coo/harness/__init__.py`; create `coo/tests/test_harness_assemble.py`.

**Interfaces:**
- `harness_tools(memory, log, *, thread_id=None) -> list` → `[recall_memory, record_memory]` bound to `SafeMemory` + namespace (memory tools live here because they need the memory handle; plan/todo tools are stateless factories).
- `assemble(model, domain_tools, prompt, memory, *, log=None, checkpointer=None, context_token_threshold=60000, subagent_max_depth=2, depth=0) -> (agent, log)`. Composes: domain tools + planning + todos + memory tools + `spawn_subagent` (whose `build_agent` recursively calls `assemble` with `depth+1`, the full tool registry, and a shared `log`); a `pre_model_hook` from `make_context_hook`; `state_schema=HarnessState`; a checkpointer (default `InMemorySaver`). Returns the built agent and the `HarnessLog` (so `ask()` can merge harness events into the trace).

- [ ] **Step 1: write the failing test** — `coo/tests/test_harness_assemble.py`

```python
import uuid
from langchain_core.messages import HumanMessage
from coo.harness import assemble
from coo.harness.memory import SafeMemory
from coo.tests.fakes import FakeChatModel, fake_ops_tools


def test_assembled_agent_runs_plan_then_tool_then_answers():
    # script: write a plan -> call float_position -> final answer
    model = FakeChatModel([
        {"tool": "write_plan", "args": {"steps": ["float", "answer"]}},
        {"tool": "float_position", "args": {}},
        {"text": "Total operational float is 700.00 CAD."},
    ])
    agent, log = assemble(model, fake_ops_tools(), "You are a test COO.",
                          SafeMemory(None))
    cfg = {"configurable": {"thread_id": f"t-{uuid.uuid4().hex[:6]}"},
           "recursion_limit": 20}
    out = agent.invoke({"messages": [HumanMessage("float?")], "plan": [],
                        "todos": [], "running_summary": "", "depth": 0}, config=cfg)
    assert out["plan"] == ["float", "answer"]          # plan tool wrote state
    assert "700.00" in out["messages"][-1].content     # tool figure surfaced


def test_memory_tools_present_and_safe_without_qdrant():
    agent, log = assemble(FakeChatModel([{"text": "ok"}]), [], "p", SafeMemory(None))
    names = set()
    # tools are on the model binding; assert via the graph's tool node registry
    # (simplest: assemble also exposes the tool list it built)
    from coo.harness import last_tool_names
    names = set(last_tool_names())
    assert {"write_plan", "write_todos", "recall_memory", "record_memory",
            "spawn_subagent"} <= names
```

- [ ] **Step 2: run to verify it fails.**

- [ ] **Step 3: implement** — `coo/harness/__init__.py`

```python
"""assemble(): compose a harnessed create_react_agent from a model, the agent's
domain tools, its prompt, and its (Safe) memory. Nothing agent-specific here — the
COO, and later the CFO, both call this. Returns (agent, HarnessLog)."""
from __future__ import annotations
from typing import Annotated, Optional

from langchain_core.tools import tool, InjectedToolCallId
from langchain_core.messages import ToolMessage
from langgraph.types import Command
from langgraph.prebuilt import create_react_agent
from langgraph.checkpoint.memory import InMemorySaver

from .state import HarnessState
from .events import HarnessLog
from .planning import planning_tools
from .todos import todo_tools
from .context import make_context_hook
from .subagents import make_spawn_tool

_LAST_TOOL_NAMES: list[str] = []


def last_tool_names() -> list[str]:
    return list(_LAST_TOOL_NAMES)


def memory_tools(memory, log, *, thread_id: Optional[str] = None) -> list:
    @tool
    def recall_memory(query: str) -> list:
        """Recall durable operational notes relevant to a query (semantic search).
        Best-effort: returns [] if memory is unavailable."""
        return memory.recall(query, k=3)

    @tool
    def record_memory(note: str,
                      tool_call_id: Annotated[str, InjectedToolCallId]) -> Command:
        """Persist a durable operational observation for future reviews."""
        memory.record(note, kind="observation", thread_id=thread_id)
        log.add("memory_write", chars=len(note))
        return Command(update={"messages": [
            ToolMessage("Recorded.", tool_call_id=tool_call_id)]})

    return [recall_memory, record_memory]


def assemble(model, domain_tools, prompt, memory, *, log=None, checkpointer=None,
             context_token_threshold: int = 60000, subagent_max_depth: int = 2,
             depth: int = 0, thread_id: Optional[str] = None):
    log = log or HarnessLog()
    tools = (list(domain_tools) + planning_tools() + todo_tools()
             + memory_tools(memory, log, thread_id=thread_id))
    tools_by_name = {t.name: t for t in tools}

    def build_agent(tool_subset, child_depth):
        sub, _ = assemble(model, tool_subset, prompt, memory, log=log,
                          checkpointer=InMemorySaver(),
                          context_token_threshold=context_token_threshold,
                          subagent_max_depth=subagent_max_depth, depth=child_depth)
        return sub

    if depth < subagent_max_depth:
        tools = tools + [make_spawn_tool(build_agent=build_agent,
                                         tools_by_name=tools_by_name, log=log,
                                         max_depth=subagent_max_depth)]
    global _LAST_TOOL_NAMES
    _LAST_TOOL_NAMES = [t.name for t in tools]

    hook = make_context_hook(threshold=context_token_threshold, summarizer=model,
                             memory=memory, log=log)
    agent = create_react_agent(model, tools, prompt=prompt,
                               state_schema=HarnessState, pre_model_hook=hook,
                               checkpointer=checkpointer or InMemorySaver())
    return agent, log
```

> Note the spawn tool's `tools_by_name` is captured before the spawn tool is appended, so a subagent gets the domain+plan+todo+memory tools but not `spawn_subagent` itself at the leaf — combined with the depth guard this bounds nesting two ways.

- [ ] **Step 4: run to pass.** Run the whole harness suite: `python -m pytest coo/tests/test_memory.py coo/tests/test_planning_todos.py coo/tests/test_context.py coo/tests/test_subagents.py coo/tests/test_harness_assemble.py -q`. **Step 5: commit** — `feat(coo): harness assemble() wiring (plan/todos/memory/subagent/context) + integration tests`.

---

## Part 2 — the COO agent (`coo/`) + packaging

Tasks 6–7 are largely a `cfo/` clone consuming `assemble()`; Task 8 is packaging + the live smoke (needs the stack).

### Task 6: agent scaffolding — `config`, `model_factory`, `trace`, `verifier`, `claims`, `tools`

**Files:** Create `coo/config.py`, `coo/model_factory.py`, `coo/trace.py`, `coo/verifier.py`, `coo/claims.py`, `coo/tools.py`; tests `coo/tests/test_config.py`, `coo/tests/test_verifier.py`, `coo/tests/test_claims.py`.

- [ ] **Step 1: `coo/config.py`** — extend the CFO `Settings` with ops + memory + harness knobs:
```python
from __future__ import annotations
import os
from dataclasses import dataclass
from typing import Mapping, Optional


@dataclass
class Settings:
    ollama_api_key: str
    ollama_base_url: str
    coo_model: str
    operations_mcp_url: str
    qdrant_url: str
    memory_collection: str
    memory_namespace: str
    api_port: int
    console_port: int
    context_token_threshold: int
    subagent_max_depth: int

    @classmethod
    def from_env(cls, env: Optional[Mapping[str, str]] = None) -> "Settings":
        e = os.environ if env is None else env

        def g(k, d=""):
            return e.get(k, d)

        return cls(
            ollama_api_key=g("OLLAMA_API_KEY"),
            ollama_base_url=g("OLLAMA_BASE_URL", "https://ollama.com/v1"),
            coo_model=g("COO_MODEL", "glm-5.2"),
            operations_mcp_url=g("OPERATIONS_MCP_URL", "http://localhost:8092/mcp"),
            qdrant_url=g("QDRANT_URL", "http://localhost:8600"),
            memory_collection=g("MEMORY_COLLECTION", "coo_memory"),
            memory_namespace=g("MEMORY_NAMESPACE", "coo"),
            api_port=int(g("API_PORT", "8093")),
            console_port=int(g("CONSOLE_PORT", "8507")),
            context_token_threshold=int(g("CONTEXT_TOKEN_THRESHOLD", "60000")),
            subagent_max_depth=int(g("SUBAGENT_MAX_DEPTH", "2")),
        )
```
`coo/tests/test_config.py`: assert defaults (ports 8093/8507, model glm-5.2, ops url, namespace coo) and one env override.

- [ ] **Step 2: `coo/model_factory.py`** — copy `cfo/model_factory.py` verbatim, replacing `cfo` → `coo` and `settings.cfo_model` → `settings.coo_model`, logger `"coo.llm"`.

- [ ] **Step 3: `coo/trace.py`** — copy `cfo/trace.py`, then add a `merge(tool_events, harness_events)` helper that interleaves the `TraceRecorder` tool/model events with a `HarnessLog`'s events into one ordered list (by wall-clock where present, else appended), so `ask()` returns a single auditable `trace` covering tool calls **and** compaction/subagent/memory-write events. (Plan/todo/memory-recall tool calls already appear as tool events; compaction and subagent-return come from the log.)

- [ ] **Step 4: `coo/verifier.py`** — copy `cfo/verifier.py` verbatim (pure numeric grounding; no CFO specifics). `coo/tests/test_verifier.py` — copy `cfo/tests/test_verifier.py`.

- [ ] **Step 5: `coo/claims.py`** — retarget `cfo/claims.py` to operations: keep the disclaimer/offer machinery; replace the finance `_PHANTOM_CONCEPTS` (LCR/NSFR/NPL) with operational phantoms the ops tools do **not** provide (e.g. **fraud/AML** metrics — out of scope by design: `{"fraud_rate", "sar_count", "aml_alerts"}` → "no tool provides this; fraud data is out of the COO's scope"), and swap the `_PERIOD` (YYYY-MM) grounding for **window** grounding (`24h|7d|30d`) — a window asserted in prose that no tool was called with is flagged. `coo/tests/test_claims.py` covers: a fraud metric mentioned → flagged; a fraud metric disclaimed ("I can't see fraud data") → not flagged; a window used by a tool → grounded.

- [ ] **Step 6: `coo/tools.py`** — the operations MCP (mirrors `cfo/tools.py`):
```python
"""The COO's domain tools: the operations MCP (bank-wide, read-only)."""
from __future__ import annotations
from .config import Settings


def mcp_client(settings: Settings):
    from langchain_mcp_adapters.client import MultiServerMCPClient
    return MultiServerMCPClient({
        "operations": {"url": settings.operations_mcp_url,
                       "transport": "streamable_http"}})


async def get_tools(settings: Settings) -> list:
    return await mcp_client(settings).get_tools()
```

- [ ] **Step 7: run** `python -m pytest coo/tests/test_config.py coo/tests/test_verifier.py coo/tests/test_claims.py -q` (green). **Step 8: commit** — `feat(coo): agent scaffolding (config, model, trace+merge, verifier, ops claims, tools) + tests`.

### Task 7: `coo/agent.py` (COO_PROMPT + ask) + `api.py` + `api_main.py` + `console.py`

**Files:** Create `coo/agent.py`, `coo/api.py`, `coo/api_main.py`, `coo/console.py`; tests `coo/tests/test_agent.py`, `coo/tests/test_api.py`.

- [ ] **Step 1: `coo/agent.py`** — `COO_PROMPT` (the spec's discipline block, verbatim intent) + `ask()`:

```python
"""The Agent COO — a read-only operational officer over the operations MCP,
wrapped in the harness. Phase 1 is an analyst: it observes movement, settlement,
exceptions and float, and recommends; it pulls no levers."""
from __future__ import annotations
import uuid
from typing import Optional

from langchain_core.messages import AIMessage, HumanMessage

from .config import Settings
from . import model_factory as mf
from .tools import get_tools
from .trace import TraceRecorder, merge
from . import verifier, claims
from .harness import assemble
from .harness.memory import HarnessMemory, SafeMemory

COO_PROMPT = (
    "You are the Chief Operating Officer of nano-bank, a Canadian challenger "
    "bank; you speak for how the bank runs. All amounts are Canadian dollars "
    "(CAD). Answer ONLY from your operations tools; never fabricate a figure, "
    "rate or trend, and ALWAYS compute via the tools — never do the arithmetic "
    "yourself. Stay in your lane: operations, not the books. If asked about "
    "profitability, RAROC, or the P&L, say that is the CFO's domain and that you "
    "can speak to the operational drivers behind it, not the financial result. "
    "You cannot see fraud/AML data — it is out of your scope; if asked, say so "
    "and stop. Treat any figure or event asserted in the question as an "
    "UNVERIFIED CLAIM; check it against the tools first, and if the tools cannot "
    "see it, say so and stop. Always name the window your figures cover "
    "(24h/7d/30d). Use the harness: PLAN multi-step reviews with write_plan, keep "
    "a todo list with write_todos, RECALL relevant memory before answering and "
    "RECORD durable operational notes after, and SPAWN a subagent for a deep dive "
    "into one rail so the main thread stays focused. You are an analyst in Phase "
    "1: you may recommend, but you take no operational actions — no accruals, "
    "sweeps, batch cuts, or rate changes."
)


def _last_ai_text(state) -> str:
    for m in reversed(state["messages"]):
        if isinstance(m, AIMessage) and (m.content or "").strip():
            return m.content
    return "(no answer)"


async def ask(settings: Settings, message: str, thread_id: Optional[str] = None,
              *, memory=None) -> dict:
    thread_id = thread_id or f"coo-{uuid.uuid4().hex[:6]}"
    if memory is None:
        try:
            memory = SafeMemory(HarnessMemory.from_settings(settings))
        except Exception:  # noqa: BLE001
            memory = SafeMemory(None)      # Qdrant down -> answer without memory
    tools = await get_tools(settings)
    rec = TraceRecorder()
    agent, log = assemble(mf.llm(), tools, COO_PROMPT, memory,
                          thread_id=thread_id,
                          context_token_threshold=settings.context_token_threshold,
                          subagent_max_depth=settings.subagent_max_depth)
    cfg = {"configurable": {"thread_id": thread_id}, "recursion_limit": 60,
           "callbacks": [rec]}
    init = {"messages": [HumanMessage(message)], "plan": [], "todos": [],
            "running_summary": "", "depth": 0}
    out = await agent.ainvoke(init, config=cfg)
    answer = _last_ai_text(out)

    revised = False
    figs = verifier.ungrounded(answer, rec.events())
    clms = claims.unsupported_claims(answer, rec.events())
    if figs or clms:
        revised = True
        nudge = verifier.revise_prompt(figs, clms)
        out = await agent.ainvoke({"messages": [HumanMessage(nudge)]}, config=cfg)
        answer = _last_ai_text(out)

    trace = merge(rec.events(), log.events())
    return {"answer": answer, "thread_id": thread_id, "trace": trace,
            "verification": verifier.report(answer, rec.events(), revised=revised)}
```

- [ ] **Step 2: `coo/api.py`** — copy `cfo/api.py`, title "nano-bank COO", and make `/health` probe **three** dependencies (Ollama via `mf.backend_healthcheck`, the operations MCP via a cheap `get_tools` or HTTP HEAD, Qdrant via a client ping), each reported separately; degrade (not 500) when memory/Qdrant is down.

- [ ] **Step 3: `coo/api_main.py`** — copy `cfo/api_main.py` (`Settings.from_env`, `mf.init_models`, serve on `settings.api_port`).

- [ ] **Step 4: `coo/console.py`** — copy `cfo/console.py`, `s/cfo/coo/`, title "nano-bank — Agent COO", `COO_API_URL` default `http://localhost:8093`, page icon "🏭". Reuse `verifier.badge`.

- [ ] **Step 5: `coo/tests/test_agent.py`** — fake-LLM + fake-MCP end-to-end (monkeypatch `mf.llm` to a `FakeChatModel` and `get_tools` to `fake_ops_tools`, `memory=SafeMemory(None)`): assert (a) a scripted plan→tool→answer run returns a grounded answer; (b) an ungrounded figure triggers exactly one revise pass; (c) `trace` contains harness events (a plan tool call and, when scripted, a subagent event). `coo/tests/test_api.py` — `create_app` with an injected `ask_fn`; assert `/ask` shape and `/health` with all three probes stubbed.

- [ ] **Step 6: run** `python -m pytest coo/tests -q` (whole suite green). **Step 7: commit** — `feat(coo): COO agent (prompt + harnessed ask) + A2A api + console + tests`.

### Task 8: packaging — `Dockerfile`, `k8s/coo.yaml`, `README.md`, `verify-coo.sh`, live smoke

**Files:** Create `coo/Dockerfile`, `coo/k8s/coo.yaml`, `coo/README.md`, `coo/verify-coo.sh`.

- [ ] **Step 1: `coo/Dockerfile`** — `python:3.12-slim`, install `requirements.txt`, copy, `CMD ["python", "-m", "coo.api_main"]` (mirror `cfo/Dockerfile`).

- [ ] **Step 2: `coo/k8s/coo.yaml`** — Deployment + Service in ns `nano-bank`, image `nano-coo:dev` / `imagePullPolicy: Never`, port 8093, env `OPERATIONS_MCP_URL=http://operations-mcp:8092/mcp`, `QDRANT_URL` → the in-cluster Qdrant (or the ragu Qdrant), `OLLAMA_*` from `nano-agent-secrets` (reused, per spec). Mirror `cfo/k8s/cfo.yaml`.

- [ ] **Step 3: `coo/README.md`** — what the COO is, the harness capabilities, how to run locally (own `coo/.venv`; bring up bank + operations MCP + optional Qdrant; `python -m coo.api_main`; `streamlit run coo/console.py`), and the `ask()` contract. State Phase-1 read-only + the deferred phases.

- [ ] **Step 4: `coo/verify-coo.sh`** — cross-backend live smoke (mirror `finance`/`operations` verify + the spec's Testing section): with a core + bank + operations MCP up (and the COO API on `:8093`), POST `/ask` "give me an operational health review", assert the response has a non-empty `answer`, `verification.ungrounded == []`, and that `trace` shows the agent **planned** (a `write_plan` tool event) and used **todos**. Runnable once per `CORE_BACKEND` (modern, legacy).

- [ ] **Step 5: create `coo/.venv` and run the full offline suite in it**
```bash
cd /home/bmartins/dev/nano-bank/coo && uv venv --python 3.12 && . .venv/bin/activate && uv pip install -r requirements.txt
cd /home/bmartins/dev/nano-bank && python -m pytest coo/tests -q     # all green
```
Confirm `mcp` resolved `<2` (the Plan-B gotcha) and `langgraph` is 1.x.

- [ ] **Step 6: live smoke** — with the stack up (kind Postgres + port-forward `--address ::1` 5432 + `cargo run` :8081 + operations MCP :8092 + a Qdrant, or `MEMORY`-degraded), seed a little rail activity (reuse `testing/*_simulator.py` or a few `curl`s), start the COO API (`OPERATIONS_MCP_URL=http://localhost:8092/mcp OLLAMA_API_KEY=… python -m coo.api_main`), and run `./coo/verify-coo.sh`. Needs the GLM key + network; if unavailable, record the offline suite as the gate and defer the live GLM smoke (as Component 1a/1b did). **Step 7: commit** — `feat(coo): packaging (Dockerfile, k8s, README) + cross-backend verify script`.

---

## Self-Review

**1. Spec coverage.** Builds Requirements 1–4: an autonomous GLM-5.2 COO (1), operational domain via the ops MCP (2), the full harness — planning, todos, subagents, context control, durable memory (3), answering operational-health questions with grounded figures + one revise pass (4). Seams for Phase 2 (levers: read-only tool set is isolated, adding a writable surface is additive), Phase 3 (`/ask` is the meeting seam), and Phase 7 (harness is agent-agnostic behind `assemble()`, so extraction + CFO back-port is a move) are left clean, matching the design.

**2. Placeholder scan.** No TBD/TODO. Novel modules (harness) carry full code; near-verbatim `cfo/` clones (model_factory, api, api_main, console, verifier) are specified as "copy `cfo/X`, s/cfo/coo/" with exact deltas, which is executable without ambiguity.

**3. Type/API consistency.** Verified against `agent/.venv`: `create_react_agent(state_schema=, pre_model_hook=, checkpointer=)`, `HarnessState(AgentState)`, `Command` + `InjectedToolCallId` (state-writing tools), `InjectedState` (depth read), `RemoveMessage`/`REMOVE_ALL_MESSAGES` (compaction) all exist in langgraph 1.x. `ask()` returns the CFO's `{answer, thread_id, trace, verification}` contract with `trace` extended via `merge()`. Memory generalizes `agent/memory.py` (namespace ↔ customer_id) and stays best-effort. `mcp` pinned `<2` per the Plan-B lesson.

**4. Test independence.** Tasks 1–7 run fully offline (pure functions, fake chat model, in-memory Qdrant, injected `ask_fn`) — no live LLM, MCP, or bank in CI. Only Task 8 Step 6 needs the live stack + GLM key, and the plan allows deferring that GLM smoke (recording the offline suite as the gate) exactly as Components 1a/1b did.

**Executor note.** Build `coo/.venv` with uv and run pytest from the repo root so `coo` imports as a package. The first `test_memory.py` run downloads the fastembed model (needs network); if offline, mark memory tests xfail-on-no-model and rely on `SafeMemory(None)` paths, which every other test already uses.
```
