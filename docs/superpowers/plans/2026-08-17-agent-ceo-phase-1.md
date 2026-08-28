# Agent CEO — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the capstone sixth C-suite seat — a CEO agent that consults the four officers, synthesizes a grounded executive brief, and directs the two acting officers (COO/CTO) through their own audited levers, plus the shared `csuite/collab.py` consult/direct primitive it is wired into first.

**Architecture:** A new `ceo/` FastAPI seat (`:8099`) mirroring `cxo/`, whose tools come from a new shared `csuite/collab.py` factory. `consult_<peer>` relays an officer's grounded `/ask` answer; `direct_<peer>` POSTs an imperative to the peer's `/ask` (its own agent self-verifies and acts via its existing lever), then **reads back** the peer's fresh `agent_action_ledger` row to prove a lever fired, and records a CEO-level directive row via `ceo/audit.py` (`append_agent_action('ceo',…)`, the `finance/db.py` psycopg2 pattern). A C-suite-meeting demo (`demos/10-ceo/`) chairs the whole thing. No bank/Rust/schema change.

**Tech Stack:** Python 3.12, LangChain/LangGraph (`langchain-core>=1`, `langgraph>=1`), `langchain-openai` (kimi-k2.6 over `https://ollama.com/v1`), FastAPI + uvicorn, httpx, psycopg2, Streamlit (present console), pytest. The shared `csuite/runtime.py` harness (plan/todos/memory/subagent/verifier). Docker + Kind + kubectl for deploy.

**Spec:** `docs/superpowers/specs/2026-08-17-agent-ceo-phase-1-design.md`

## Global Constraints

- **No bank/Rust/schema change.** The CEO writes only its own directive rows to the **existing** `agent_action_ledger` via the existing `append_agent_action(actor, action, params::jsonb, effect::jsonb)` function (returns `TABLE(seq BIGINT, entry_hash TEXT)`). Never alter the table or the function.
- **Directable seats = COO + CTO only.** CFO + CXO are consult-only (no `direct_*` tool).
- **Never bypass a guardrail.** A directive is an imperative to the peer's own `/ask`; the peer's agent decides and acts. The CEO reports honestly whether a lever fired.
- **Grounding.** CEO domain figures are auto-grounded by the shared number verifier against consult-tool outputs in the trace (`kind=="tool"`); `ceo/claims.py` adds the CEO-specific **directive-honesty** guard only.
- **Ports:** CEO API `8099`, present console `8511`. In-cluster peer URLs: `http://cfo:8089`, `http://coo:8093`, `http://cto:8095`, `http://cxo:8098` (all `/ask`).
- **Model:** `kimi-k2.6` primary + fallback, via `nano-agent-secrets` (`OLLAMA_API_KEY`), base `https://ollama.com/v1`.
- **DB env (in-cluster):** `DB_HOST=postgres-service`, `DB_PORT=5432`, `DB_NAME=nano_bank_db`, `DB_USER=nanobank_user`, `DB_PASSWORD=secure_nano_password_2024!` (local default `DB_HOST=::1`).
- **Namespace:** `nano-bank`; kube context `kind-nano-bank`. Docker images `imagePullPolicy: Never`, built from **repo root** so the `csuite` package is in context.
- **Trace event shape** (for guards): `{"kind": "tool", "name": <tool>, "output": <str|obj>}`.
- Run all `pytest` from the **repo root** (`/home/bmartins/dev/nano-bank`).

## File Structure

**New — shared primitive:**
- `csuite/collab.py` — consult/direct tool factory + `AuditPort` protocol.
- `csuite/tests/test_collab.py` — offline unit tests (fake httpx client + fake audit).

**New — the seat (`ceo/`, mirrors `cxo/`):**
- `ceo/__init__.py`
- `ceo/config.py` — `Settings` (ports, peer URLs, DB params, model, memory).
- `ceo/model_factory.py` — model resolution (copy of `cxo/model_factory.py`, `CEO_MODEL`).
- `ceo/audit.py` — the only writer: `Audit` implementing `AuditPort` over Postgres.
- `ceo/claims.py` — directive-honesty guard (`unsupported_claims`).
- `ceo/tools.py` — builds the peer registry from `Settings`, calls `csuite.collab.build_tools`.
- `ceo/agent.py` — `CEO_PROMPT` + `ask`/`ask_stream` over `csuite.runtime`.
- `ceo/api.py` — `/ask`, `/ask/stream`, `/livez`, `/health`.
- `ceo/api_main.py` — container entrypoint.
- `ceo/requirements.txt`, `ceo/Dockerfile`.
- `ceo/k8s/ceo.yaml`, `ceo/k8s/deploy.sh`.
- `ceo/tests/__init__.py`, `ceo/tests/test_audit.py`, `ceo/tests/test_claims.py`, `ceo/tests/test_tools.py`, `ceo/tests/test_prompt.py`, `ceo/tests/test_api.py`.

**New — the demo (`demos/10-ceo/`, mirrors `demos/09-cxo/`):**
- `demos/10-ceo/drive.py` — the meeting beats (uses `demos/_driver.py`).
- `demos/10-ceo/run-demo.sh` — up → seed (pending AFT batch) → drive.
- `demos/10-ceo/README.md`.
- `demos/10-ceo/present/state.py`, `present/app.py`, `present/requirements.txt`, `present/README.md`, `present/tests/__init__.py`, `present/tests/test_state.py`.

---

## Task 1: `csuite/collab.py` — the consult tool

**Files:**
- Create: `csuite/collab.py`
- Test: `csuite/tests/test_collab.py`

**Interfaces:**
- Consumes: nothing from earlier tasks; `langchain_core.tools.StructuredTool`, `httpx`.
- Produces:
  - `async def post_ask(base_url: str, message: str, client=None) -> dict` — POSTs `{"message": message}` to `base_url.rstrip("/") + "/ask"`, `raise_for_status()`, returns `.json()`. If `client` is given, uses `await client.post(url, json=...)`; else opens `httpx.AsyncClient(timeout=600)`.
  - `def consult_tool(peer: str, base_url: str, *, client=None) -> StructuredTool` — a `StructuredTool` named `consult_<peer>` whose coroutine takes `question: str` and returns `{"officer": peer, "answer": <str>}`.

- [ ] **Step 1: Write the failing test**

Create `csuite/tests/__init__.py` if missing (empty), then `csuite/tests/test_collab.py`. The repo has **no** pytest-asyncio, so drive coroutines with `asyncio.run(...)` in plain sync tests (matching the seat tests' style — no `@pytest.mark.asyncio`):

```python
import asyncio
import pytest
from csuite import collab


class FakeResp:
    def __init__(self, payload): self._p = payload
    def raise_for_status(self): pass
    def json(self): return self._p


class FakeClient:
    """Records posts; returns a scripted /ask payload."""
    def __init__(self, payload): self.payload = payload; self.posts = []
    async def post(self, url, json=None):
        self.posts.append((url, json))
        return FakeResp(self.payload)


def test_consult_relays_answer_attributed():
    client = FakeClient({"answer": "NIM was 3.1% last month."})
    tool = collab.consult_tool("cfo", "http://cfo:8089", client=client)
    assert tool.name == "consult_cfo"
    out = asyncio.run(tool.ainvoke({"question": "What was NIM?"}))
    assert out == {"officer": "cfo", "answer": "NIM was 3.1% last month."}
    assert client.posts == [("http://cfo:8089/ask", {"message": "What was NIM?"})]


def test_consult_surfaces_down_peer_as_error():
    class Boom:
        async def post(self, url, json=None): raise RuntimeError("connection refused")
    tool = collab.consult_tool("coo", "http://coo:8093", client=Boom())
    with pytest.raises(RuntimeError):
        asyncio.run(tool.ainvoke({"question": "status?"}))
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest csuite/tests/test_collab.py -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'csuite.collab'` (or `AttributeError`).

- [ ] **Step 3: Write minimal implementation**

Create `csuite/collab.py`:

```python
"""Shared C-suite consult/direct primitive (board-vision item #1).

`consult_<peer>` relays a peer officer's grounded `/ask` answer, attributed.
`direct_<peer>` POSTs an imperative to the peer's `/ask` — the peer's OWN agent
self-verifies and acts via its EXISTING audited lever — then reads back the peer's
fresh ledger row to prove a lever fired and records a directing-actor directive row
via an injected AuditPort. Wired into the CEO first; reusable by the Phase-2 board.
"""
from __future__ import annotations
from typing import Iterable, Optional, Protocol, runtime_checkable

from langchain_core.tools import StructuredTool


async def post_ask(base_url: str, message: str, client=None) -> dict:
    url = base_url.rstrip("/") + "/ask"
    if client is not None:
        r = await client.post(url, json={"message": message})
    else:
        import httpx
        async with httpx.AsyncClient(timeout=600) as c:
            r = await c.post(url, json={"message": message})
    r.raise_for_status()
    return r.json()


def consult_tool(peer: str, base_url: str, *, client=None) -> StructuredTool:
    async def _consult(question: str) -> dict:
        resp = await post_ask(base_url, question, client)
        return {"officer": peer, "answer": resp.get("answer", "")}

    return StructuredTool.from_function(
        coroutine=_consult, name=f"consult_{peer}",
        description=(f"Consult the {peer.upper()} and relay its grounded answer "
                     "to a question, attributed. Read-only: the officer analyses "
                     "and reports; it does not act."))
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest csuite/tests/test_collab.py -v`
Expected: PASS (2 passed).

- [ ] **Step 5: Commit**

```bash
git add csuite/collab.py csuite/tests/__init__.py csuite/tests/test_collab.py
git commit -m "feat(csuite): collab.consult_tool — relay a peer officer's grounded /ask answer"
```

---

## Task 2: `csuite/collab.py` — the direct tool with ledger read-back

**Files:**
- Modify: `csuite/collab.py`
- Test: `csuite/tests/test_collab.py`

**Interfaces:**
- Consumes: `post_ask` (Task 1).
- Produces:
  - `class AuditPort(Protocol)` with:
    - `def latest_actor_seq(self, actor: str) -> int` — the max `seq` in `agent_action_ledger` for `actor`, or `0` if none.
    - `def rows_since(self, actor: str, seq: int) -> list[dict]` — rows with `actor=actor AND seq>seq`, ordered by `seq`, each `{"seq": int, "action": str, "effect": dict}`.
    - `def direct(self, peer: str, params: dict, effect: dict) -> dict` — appends a `('ceo', 'direct_'+peer, params, effect)` ledger row; returns `{"seq": int, "entry_hash": str}`.
  - `def direct_tool(peer: str, base_url: str, audit: "AuditPort", *, client=None) -> StructuredTool` — a `StructuredTool` named `direct_<peer>` whose coroutine takes `directive: str, rationale: str = ""` and returns `{"peer", "directive", "officer_acted": bool, "officer_row": dict|None, "officer_response": str}`. It snapshots `before = audit.latest_actor_seq(peer)`, POSTs the directive, reads `new = audit.rows_since(peer, before)`, sets `officer_row = new[-1] if new else None` and `officer_acted = bool(new)`, then calls `audit.direct(peer, {"directive","rationale"}, effect)`.

- [ ] **Step 1: Write the failing test**

Append to `csuite/tests/test_collab.py`:

```python
class FakeAudit:
    """A fake ledger: `new_rows` are the peer rows that 'appear' during the POST."""
    def __init__(self, before_seq=10, new_rows=None):
        self.before_seq = before_seq
        self.new_rows = new_rows or []
        self.direct_calls = []

    def latest_actor_seq(self, actor): return self.before_seq
    def rows_since(self, actor, seq): return list(self.new_rows)
    def direct(self, peer, params, effect):
        self.direct_calls.append((peer, params, effect))
        return {"seq": 999, "entry_hash": "abc"}


def test_direct_records_officer_row_when_lever_fires():
    audit = FakeAudit(before_seq=10, new_rows=[
        {"seq": 11, "action": "cut_aft_batch", "effect": {"batch": "B7", "entries": 3}}])
    client = FakeClient({"answer": "Cut batch B7 (3 entries)."})
    tool = collab.direct_tool("coo", "http://coo:8093", audit, client=client)
    assert tool.name == "direct_coo"

    out = asyncio.run(tool.ainvoke({"directive": "Cut the pending AFT batch.",
                                    "rationale": "COO reported a stuck batch."}))
    assert out["peer"] == "coo"
    assert out["officer_acted"] is True
    assert out["officer_row"] == {"seq": 11, "action": "cut_aft_batch",
                                  "effect": {"batch": "B7", "entries": 3}}
    assert out["officer_response"] == "Cut batch B7 (3 entries)."
    # exactly one CEO directive row, carrying the read-back
    assert len(audit.direct_calls) == 1
    peer, params, effect = audit.direct_calls[0]
    assert peer == "coo"
    assert params == {"directive": "Cut the pending AFT batch.",
                      "rationale": "COO reported a stuck batch."}
    assert effect["officer_acted"] is True
    assert effect["officer_row"]["action"] == "cut_aft_batch"


def test_direct_reports_no_lever_when_no_new_row():
    audit = FakeAudit(before_seq=10, new_rows=[])   # officer only talked / refused
    client = FakeClient({"answer": "I reviewed it; no action was warranted."})
    tool = collab.direct_tool("cto", "http://cto:8095", audit, client=client)

    out = asyncio.run(tool.ainvoke({"directive": "Roll back the deploy."}))
    assert out["officer_acted"] is False
    assert out["officer_row"] is None
    assert audit.direct_calls[0][2]["officer_acted"] is False
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest csuite/tests/test_collab.py -k direct -v`
Expected: FAIL — `AttributeError: module 'csuite.collab' has no attribute 'direct_tool'`.

- [ ] **Step 3: Write minimal implementation**

Append to `csuite/collab.py`:

```python
@runtime_checkable
class AuditPort(Protocol):
    def latest_actor_seq(self, actor: str) -> int: ...
    def rows_since(self, actor: str, seq: int) -> list[dict]: ...
    def direct(self, peer: str, params: dict, effect: dict) -> dict: ...


def direct_tool(peer: str, base_url: str, audit: AuditPort, *,
                client=None) -> StructuredTool:
    async def _direct(directive: str, rationale: str = "") -> dict:
        before = audit.latest_actor_seq(peer)
        resp = await post_ask(base_url, directive, client)
        officer_response = resp.get("answer", "")
        new = audit.rows_since(peer, before)
        officer_row = new[-1] if new else None
        effect = {"officer_acted": bool(new),
                  "officer_row": officer_row,
                  "officer_response": officer_response}
        audit.direct(peer, {"directive": directive, "rationale": rationale}, effect)
        return {"peer": peer, "directive": directive, **effect}

    return StructuredTool.from_function(
        coroutine=_direct, name=f"direct_{peer}",
        description=(
            f"DIRECT the {peer.upper()} to act on an imperative. The {peer.upper()}'s "
            "own agent self-verifies and acts via its audited lever — you bypass no "
            "guardrail. This reads back the officer's fresh ledger row to prove a "
            "lever actually fired (officer_acted=false means the officer only "
            "deliberated or refused), and records the CEO directive row. Pass a "
            "`directive` (the imperative) and a `rationale` (your grounded why)."))
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest csuite/tests/test_collab.py -v`
Expected: PASS (4 passed).

- [ ] **Step 5: Commit**

```bash
git add csuite/collab.py csuite/tests/test_collab.py
git commit -m "feat(csuite): collab.direct_tool — imperative to peer /ask + ledger read-back verify"
```

---

## Task 3: `csuite/collab.py` — `build_tools` registry

**Files:**
- Modify: `csuite/collab.py`
- Test: `csuite/tests/test_collab.py`

**Interfaces:**
- Consumes: `consult_tool`, `direct_tool` (Tasks 1–2).
- Produces:
  - `def build_tools(registry: dict, audit: "AuditPort", *, client=None) -> list` where `registry = {"peers": {name: base_url, ...}, "directable": {name, ...}}`. Returns a `consult_<peer>` tool for every peer, plus a `direct_<peer>` tool for every peer in `directable`. Deterministic order: consults first (in `peers` insertion order), then directs.

- [ ] **Step 1: Write the failing test**

Append to `csuite/tests/test_collab.py`:

```python
def test_build_tools_wires_consults_for_all_and_directs_for_directable():
    audit = FakeAudit()
    registry = {"peers": {"cfo": "http://cfo:8089", "coo": "http://coo:8093",
                          "cto": "http://cto:8095", "cxo": "http://cxo:8098"},
                "directable": {"coo", "cto"}}
    tools = collab.build_tools(registry, audit)
    names = [t.name for t in tools]
    assert names[:4] == ["consult_cfo", "consult_coo", "consult_cto", "consult_cxo"]
    assert set(names[4:]) == {"direct_coo", "direct_cto"}
    # CFO/CXO are consult-only — no direct tool for them
    assert "direct_cfo" not in names and "direct_cxo" not in names
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest csuite/tests/test_collab.py -k build_tools -v`
Expected: FAIL — `AttributeError: ... has no attribute 'build_tools'`.

- [ ] **Step 3: Write minimal implementation**

Append to `csuite/collab.py`:

```python
def build_tools(registry: dict, audit: AuditPort, *, client=None) -> list:
    peers: dict = registry["peers"]
    directable = set(registry.get("directable", ()))
    tools = [consult_tool(name, url, client=client) for name, url in peers.items()]
    tools += [direct_tool(name, peers[name], audit, client=client)
              for name in peers if name in directable]
    return tools
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest csuite/tests/test_collab.py -v`
Expected: PASS (5 passed).

- [ ] **Step 5: Commit**

```bash
git add csuite/collab.py csuite/tests/test_collab.py
git commit -m "feat(csuite): collab.build_tools — assemble consult(all)+direct(directable) tools"
```

---

## Task 4: `ceo/config.py` + `ceo/model_factory.py`

**Files:**
- Create: `ceo/__init__.py` (empty), `ceo/config.py`, `ceo/model_factory.py`
- Test: `ceo/tests/__init__.py` (empty), `ceo/tests/test_config.py`

**Interfaces:**
- Produces:
  - `Settings` dataclass with fields: `ollama_api_key, ollama_base_url, ceo_model, ceo_model_fallback, cfo_url, coo_url, cto_url, cxo_url, db (dict), qdrant_url, memory_collection, memory_namespace, api_port, console_port, context_token_threshold, subagent_max_depth`, plus `@classmethod from_env(env=None) -> Settings`.
  - `Settings.peer_registry() -> dict` returning `{"peers": {"cfo":…, "coo":…, "cto":…, "cxo":…}, "directable": {"coo","cto"}}`.
  - `model_factory`: `init_models(settings, probe=None) -> str`, `llm(*, temperature=0.1, max_tokens=None) -> ChatOpenAI`, `resolve_model(settings, probe=None) -> str`, `backend_healthcheck(settings) -> bool` (same API as `cxo/model_factory.py`).

- [ ] **Step 1: Write the failing test**

Create `ceo/tests/test_config.py`:

```python
from ceo.config import Settings


def test_defaults_ports_and_peers():
    s = Settings.from_env({})
    assert s.api_port == 8099
    assert s.console_port == 8511
    assert s.coo_url == "http://coo:8093"
    assert s.cto_url == "http://cto:8095"
    reg = s.peer_registry()
    assert set(reg["peers"]) == {"cfo", "coo", "cto", "cxo"}
    assert reg["directable"] == {"coo", "cto"}
    assert reg["peers"]["cfo"] == "http://cfo:8089"


def test_db_and_model_from_env():
    s = Settings.from_env({"DB_HOST": "postgres-service", "CEO_MODEL": "kimi-k2.6"})
    assert s.db["host"] == "postgres-service"
    assert s.db["dbname"] == "nano_bank_db"
    assert s.ceo_model == "kimi-k2.6"


def test_resolve_model_prefers_primary_via_probe():
    from ceo import model_factory as mf
    s = Settings.from_env({"CEO_MODEL": "kimi-k2.6", "CEO_MODEL_FALLBACK": "kimi-k2.6"})
    assert mf.resolve_model(s, probe=lambda m, st: True) == "kimi-k2.6"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest ceo/tests/test_config.py -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'ceo'`.

- [ ] **Step 3: Write minimal implementation**

Create `ceo/__init__.py` (empty) and `ceo/tests/__init__.py` (empty). Create `ceo/config.py`:

```python
from __future__ import annotations
import os
from dataclasses import dataclass
from typing import Mapping, Optional


@dataclass
class Settings:
    ollama_api_key: str
    ollama_base_url: str
    ceo_model: str
    ceo_model_fallback: str
    cfo_url: str
    coo_url: str
    cto_url: str
    cxo_url: str
    db: dict
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
            ceo_model=g("CEO_MODEL", "kimi-k2.6"),
            ceo_model_fallback=g("CEO_MODEL_FALLBACK", "kimi-k2.6"),
            cfo_url=g("CFO_URL", "http://cfo:8089"),
            coo_url=g("COO_URL", "http://coo:8093"),
            cto_url=g("CTO_URL", "http://cto:8095"),
            cxo_url=g("CXO_URL", "http://cxo:8098"),
            db=dict(
                host=g("DB_HOST", "::1"),
                port=int(g("DB_PORT", "5432")),
                dbname=g("DB_NAME", "nano_bank_db"),
                user=g("DB_USER", "nanobank_user"),
                password=g("DB_PASSWORD", "secure_nano_password_2024!"),
            ),
            qdrant_url=g("QDRANT_URL", "http://agent-qdrant:6333"),
            memory_collection=g("MEMORY_COLLECTION", "ceo_memory"),
            memory_namespace=g("MEMORY_NAMESPACE", "ceo"),
            api_port=int(g("API_PORT", "8099")),
            console_port=int(g("CONSOLE_PORT", "8511")),
            context_token_threshold=int(g("CONTEXT_TOKEN_THRESHOLD", "60000")),
            subagent_max_depth=int(g("SUBAGENT_MAX_DEPTH", "2")),
        )

    def peer_registry(self) -> dict:
        return {
            "peers": {"cfo": self.cfo_url, "coo": self.coo_url,
                      "cto": self.cto_url, "cxo": self.cxo_url},
            "directable": {"coo", "cto"},
        }
```

Create `ceo/model_factory.py` — copy `cxo/model_factory.py` verbatim, then change: the logger name to `"ceo.llm"`, and both references to `settings.cxo_model` / `settings.cxo_model_fallback` (inside `_candidates`) to `settings.ceo_model` / `settings.ceo_model_fallback`. Everything else (build_model, `_default_probe`, `resolve_model`, `init_models`, `llm`, `backend_healthcheck`) is unchanged.

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest ceo/tests/test_config.py -v`
Expected: PASS (3 passed).

- [ ] **Step 5: Commit**

```bash
git add ceo/__init__.py ceo/config.py ceo/model_factory.py ceo/tests/__init__.py ceo/tests/test_config.py
git commit -m "feat(ceo): Settings (ports/peers/DB) + peer_registry + model_factory"
```

---

## Task 5: `ceo/audit.py` — the ledger writer + read-back (implements `AuditPort`)

**Files:**
- Create: `ceo/audit.py`
- Test: `ceo/tests/test_audit.py`

**Interfaces:**
- Consumes: `csuite.collab.AuditPort` (structural — no import needed); DB params from `Settings.db`.
- Produces: `class Audit` with `__init__(self, db_params: dict, connect=None)` and the three `AuditPort` methods. `connect` is an optional zero-arg factory returning a DB connection (for tests); default is `lambda: psycopg2.connect(**db_params)`.
  - `latest_actor_seq(actor)` runs `SELECT COALESCE(MAX(seq),0) FROM agent_action_ledger WHERE actor=%s` → int.
  - `rows_since(actor, seq)` runs `SELECT seq, action, effect FROM agent_action_ledger WHERE actor=%s AND seq>%s ORDER BY seq` → `[{"seq","action","effect"}]`.
  - `direct(peer, params, effect)` runs `SELECT seq, entry_hash FROM append_agent_action('ceo', %s, %s::jsonb, %s::jsonb)` with `(f"direct_{peer}", json.dumps(params), json.dumps(effect))` → `{"seq","entry_hash"}`.

- [ ] **Step 1: Write the failing test**

Create `ceo/tests/test_audit.py` (a fake DB-API connection records the SQL and returns scripted rows — offline, no psycopg2):

```python
import json
from ceo.audit import Audit


class FakeCursor:
    def __init__(self, script): self.script = script; self.executed = []; self._last = None
    def execute(self, sql, params=None):
        self.executed.append((" ".join(sql.split()), params))
        for key, rows in self.script.items():
            if key in " ".join(sql.split()):
                self._last = rows
                return
        self._last = []
    def fetchone(self): return self._last[0] if self._last else None
    def fetchall(self): return list(self._last)
    def __enter__(self): return self
    def __exit__(self, *a): return False


class FakeConn:
    def __init__(self, script): self._script = script; self.cur = FakeCursor(script)
    def set_session(self, **k): pass
    def cursor(self, **k): return self.cur
    def __enter__(self): return self
    def __exit__(self, *a): return False
    def close(self): pass


def _audit(script):
    conn = FakeConn(script)
    return Audit({"host": "x"}, connect=lambda: conn), conn


def test_latest_actor_seq_returns_int():
    audit, conn = _audit({"MAX(seq)": [(42,)]})
    assert audit.latest_actor_seq("coo") == 42
    sql, params = conn.cur.executed[-1]
    assert "FROM agent_action_ledger WHERE actor=%s" in sql and params == ("coo",)


def test_rows_since_shapes_rows():
    audit, conn = _audit({"seq>%s": [(11, "cut_aft_batch", {"batch": "B7"})]})
    rows = audit.rows_since("coo", 10)
    assert rows == [{"seq": 11, "action": "cut_aft_batch", "effect": {"batch": "B7"}}]
    sql, params = conn.cur.executed[-1]
    assert params == ("coo", 10)


def test_direct_appends_ceo_row_with_json_params():
    audit, conn = _audit({"append_agent_action": [(999, "deadbeef")]})
    out = audit.direct("coo", {"directive": "cut it", "rationale": "stuck"},
                       {"officer_acted": True})
    assert out == {"seq": 999, "entry_hash": "deadbeef"}
    sql, params = conn.cur.executed[-1]
    assert "append_agent_action('ceo', %s, %s::jsonb, %s::jsonb)" in sql
    assert params[0] == "direct_coo"
    assert json.loads(params[1]) == {"directive": "cut it", "rationale": "stuck"}
    assert json.loads(params[2]) == {"officer_acted": True}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest ceo/tests/test_audit.py -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'ceo.audit'`.

- [ ] **Step 3: Write minimal implementation**

Create `ceo/audit.py`:

```python
"""The CEO's ONLY writer: read-back the peer's ledger rows and append the CEO's own
directive row to the existing hash-chained agent_action_ledger via
append_agent_action('ceo', …). psycopg2, mirroring finance/db.py. No bank change."""
from __future__ import annotations
import json
from typing import Callable, Optional


class Audit:
    def __init__(self, db_params: dict, connect: Optional[Callable] = None):
        self._db = db_params
        self._connect = connect or (lambda: __import__("psycopg2").connect(**db_params))

    def latest_actor_seq(self, actor: str) -> int:
        conn = self._connect()
        try:
            with conn.cursor() as cur:
                cur.execute(
                    "SELECT COALESCE(MAX(seq),0) FROM agent_action_ledger WHERE actor=%s",
                    (actor,))
                row = cur.fetchone()
                return int(row[0]) if row else 0
        finally:
            conn.close()

    def rows_since(self, actor: str, seq: int) -> list[dict]:
        conn = self._connect()
        try:
            with conn.cursor() as cur:
                cur.execute(
                    "SELECT seq, action, effect FROM agent_action_ledger "
                    "WHERE actor=%s AND seq>%s ORDER BY seq", (actor, seq))
                return [{"seq": r[0], "action": r[1], "effect": r[2]}
                        for r in cur.fetchall()]
        finally:
            conn.close()

    def direct(self, peer: str, params: dict, effect: dict) -> dict:
        conn = self._connect()
        try:
            with conn:
                with conn.cursor() as cur:
                    cur.execute(
                        "SELECT seq, entry_hash FROM "
                        "append_agent_action('ceo', %s, %s::jsonb, %s::jsonb)",
                        (f"direct_{peer}", json.dumps(params), json.dumps(effect)))
                    row = cur.fetchone()
                    return {"seq": row[0], "entry_hash": row[1]}
        finally:
            conn.close()
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest ceo/tests/test_audit.py -v`
Expected: PASS (3 passed).

- [ ] **Step 5: Commit**

```bash
git add ceo/audit.py ceo/tests/test_audit.py
git commit -m "feat(ceo): audit.py — ledger read-back + append_agent_action('ceo',…) directive row"
```

---

## Task 6: `ceo/claims.py` — the directive-honesty guard

**Files:**
- Create: `ceo/claims.py`
- Test: `ceo/tests/test_claims.py`

**Interfaces:**
- Consumes: the trace event shape `{"kind":"tool","name":<str>,"output":<str|obj>}`.
- Produces: `def unsupported_claims(answer: str, trace: list[dict]) -> list[str]`. Rationale: CEO domain *figures* are grounded automatically by the shared number verifier against consult-tool outputs; the CEO's unique risk is **overclaiming a directive** — asserting an officer acted when the `direct_<peer>` read-back returned `officer_acted=false`. The guard scans the trace for `direct_*` tool outputs with a false `officer_acted`; if the answer uses a completion cue, it flags the overclaim.

- [ ] **Step 1: Write the failing test**

Create `ceo/tests/test_claims.py`:

```python
from ceo import claims


def _direct_event(peer, acted):
    return {"kind": "tool", "name": f"direct_{peer}",
            "output": {"peer": peer, "officer_acted": acted, "officer_row": None,
                       "officer_response": "..."}}


def test_flags_completion_claim_when_no_lever_fired():
    trace = [_direct_event("coo", False)]
    out = claims.unsupported_claims("I directed the COO and the batch was cut.", trace)
    assert out and "coo" in out[0].lower()


def test_ok_when_completion_claim_and_lever_fired():
    trace = [_direct_event("coo", True)]
    assert claims.unsupported_claims("The COO cut the batch, done.", trace) == []


def test_ok_when_no_completion_cue_even_if_no_lever():
    trace = [_direct_event("cto", False)]
    # honest reporting that nothing was actioned is fine
    assert claims.unsupported_claims(
        "I asked the CTO; it judged no rollback was warranted.", trace) == []


def test_handles_stringified_tool_output():
    trace = [{"kind": "tool", "name": "direct_coo",
              "output": "{'peer': 'coo', 'officer_acted': False}"}]
    out = claims.unsupported_claims("The COO executed the directive successfully.", trace)
    assert out
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest ceo/tests/test_claims.py -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'ceo.claims'`.

- [ ] **Step 3: Write minimal implementation**

Create `ceo/claims.py`:

```python
"""Directive-honesty guard for the Agent CEO. Domain FIGURES are grounded by the
shared number verifier (consult-tool outputs are tool results in the trace); the
CEO's unique integrity risk is overclaiming a DIRECTIVE — saying an officer acted
when the read-back proved no lever fired. Deterministic, cue-based, no LLM."""
from __future__ import annotations
import re

# Words that assert the directive was carried out (vs merely proposed / declined).
_COMPLETION = re.compile(
    r"\b(done|executed|completed|carried out|actioned|implemented|successfully"
    r"|was (?:cut|rolled back|done|executed)|has (?:cut|run|executed|rolled back)"
    r"|cut the batch|rolled back)\b", re.I)

# A false officer_acted in a direct_* tool output, tolerant of dict OR str(dict).
_ACTED_FALSE = re.compile(r"officer_acted['\"]?\s*[:=]\s*(?:False|false)")


def _peers_without_lever(trace: list[dict]) -> list[str]:
    out: list[str] = []
    for ev in trace:
        if ev.get("kind") != "tool":
            continue
        name = ev.get("name") or ""
        if not name.startswith("direct_"):
            continue
        raw = ev.get("output")
        acted_false = False
        if isinstance(raw, dict):
            acted_false = raw.get("officer_acted") is False
        else:
            acted_false = bool(_ACTED_FALSE.search(str(raw)))
        if acted_false:
            out.append(name[len("direct_"):])
    return out


def unsupported_claims(answer: str, trace: list[dict]) -> list[str]:
    peers = _peers_without_lever(trace)
    if not peers or not _COMPLETION.search(answer or ""):
        return []
    return [f"claimed a directive to the {p.upper()} completed, but the read-back "
            f"showed no lever fired (officer_acted=false)" for p in dict.fromkeys(peers)]
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest ceo/tests/test_claims.py -v`
Expected: PASS (4 passed).

- [ ] **Step 5: Commit**

```bash
git add ceo/claims.py ceo/tests/test_claims.py
git commit -m "feat(ceo): claims.py — directive-honesty guard (no overclaiming a lever that never fired)"
```

---

## Task 7: `ceo/tools.py` — build the CEO's tools from the registry

**Files:**
- Create: `ceo/tools.py`
- Test: `ceo/tests/test_tools.py`

**Interfaces:**
- Consumes: `Settings.peer_registry()` (Task 4), `csuite.collab.build_tools` (Task 3), `ceo.audit.Audit` (Task 5).
- Produces: `def get_tools(settings: Settings, *, audit=None, client=None) -> list` — builds `Audit(settings.db)` (unless one is injected) and returns `collab.build_tools(settings.peer_registry(), audit, client=client)`. Async-compatible signature is NOT needed (unlike cxo's MCP-backed `get_tools`), but keep it a plain function returning the tool list.

- [ ] **Step 1: Write the failing test**

Create `ceo/tests/test_tools.py`:

```python
from ceo.config import Settings
from ceo.tools import get_tools


class _Audit:
    def latest_actor_seq(self, a): return 0
    def rows_since(self, a, s): return []
    def direct(self, p, pa, e): return {"seq": 1, "entry_hash": "x"}


def test_get_tools_has_four_consults_and_two_directs():
    tools = get_tools(Settings.from_env({}), audit=_Audit())
    names = {t.name for t in tools}
    assert {"consult_cfo", "consult_coo", "consult_cto", "consult_cxo"} <= names
    assert {"direct_coo", "direct_cto"} <= names
    assert "direct_cfo" not in names and "direct_cxo" not in names
    assert len(tools) == 6
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest ceo/tests/test_tools.py -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'ceo.tools'`.

- [ ] **Step 3: Write minimal implementation**

Create `ceo/tools.py`:

```python
"""The CEO's tools: the shared consult/direct primitive, wired to the four officer
seats. Consult all four; direct the two acting seats (COO, CTO). The CEO holds no
domain MCP of its own — its knowledge comes from the officers it consults."""
from __future__ import annotations
from typing import Optional

from csuite import collab

from .config import Settings
from .audit import Audit


def get_tools(settings: Settings, *, audit=None, client=None) -> list:
    audit = audit if audit is not None else Audit(settings.db)
    return collab.build_tools(settings.peer_registry(), audit, client=client)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest ceo/tests/test_tools.py -v`
Expected: PASS (1 passed).

- [ ] **Step 5: Commit**

```bash
git add ceo/tools.py ceo/tests/test_tools.py
git commit -m "feat(ceo): tools.py — consult(all four)+direct(coo,cto) from csuite.collab"
```

---

## Task 8: `ceo/agent.py` — the CEO prompt + ask/ask_stream

**Files:**
- Create: `ceo/agent.py`
- Test: `ceo/tests/test_prompt.py`

**Interfaces:**
- Consumes: `csuite.runtime.ask` / `ask_stream` (keyword args `settings, message, prompt, model, tools, agent, thread_id, memory, claims_fn`), `ceo.model_factory.llm`, `ceo.claims.unsupported_claims`, `ceo.tools.get_tools`.
- Produces:
  - `CEO_PROMPT: str`
  - `async def ask(settings, message, thread_id=None, *, memory=None) -> dict`
  - `async def ask_stream(settings, message, thread_id=None, *, memory=None) -> AsyncIterator[dict]`

- [ ] **Step 1: Write the failing test**

Create `ceo/tests/test_prompt.py` (assert the prompt encodes the locked lane — no LLM call):

```python
from ceo.agent import CEO_PROMPT


def test_prompt_states_synthesizer_lane_and_attribution():
    p = CEO_PROMPT.lower()
    assert "chief executive" in p
    assert "consult" in p and "synthes" in p
    assert "attribut" in p            # figures attributed to the officer
    assert "never invent" in p or "do not invent" in p


def test_prompt_names_directable_and_consult_only_seats():
    p = CEO_PROMPT.lower()
    assert "direct_coo" in p and "direct_cto" in p
    # CFO + CXO are consult-only — the prompt must say they cannot be directed
    assert "cannot direct" in p or "consult-only" in p or "no levers" in p


def test_prompt_demands_honest_directive_reporting():
    p = CEO_PROMPT.lower()
    assert "fired" in p or "acted" in p
    assert "guardrail" in p
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest ceo/tests/test_prompt.py -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'ceo.agent'`.

- [ ] **Step 3: Write minimal implementation**

Create `ceo/agent.py`:

```python
"""The Agent CEO — the capstone C-suite seat. It holds no domain data: it CONSULTS
the four officers, SYNTHESIZES a grounded cross-functional executive brief (every
figure attributed to the officer who reported it), and DIRECTS the two acting seats
(COO, CTO) via imperatives their own agents self-verify and act on. Wrapped in the
shared csuite harness; grounded by the shared number verifier + a directive-honesty
guard."""
from __future__ import annotations
from typing import AsyncIterator, Optional

from csuite import runtime

from .config import Settings
from . import model_factory as mf
from . import claims as ceo_claims
from .tools import get_tools

CEO_PROMPT = (
    "You are the Chief Executive Officer of nano-bank, a Canadian challenger bank. "
    "You hold NO domain data of your own: you CONSULT your officers and SYNTHESIZE "
    "their reports into one grounded, cross-functional executive brief. Consult the "
    "CFO (the books: profitability, NIM, RAROC, capital) with `consult_cfo`, the "
    "COO (money-movement operations: rails, settlement, float) with `consult_coo`, "
    "the CTO (the platform: reliability, deployments, incidents) with `consult_cto`, "
    "and the CXO (customer experience: onboarding, friction, the customer voice, "
    "NPS/CSAT) with `consult_cxo`. Every figure in your brief MUST be attributed to "
    "the officer who reported it (e.g. 'the CFO reports NIM of 3.1%'); NEVER invent "
    "a number, rate, count or ratio, and never quote a figure no officer gave you. "
    "For any DERIVED figure, call the `compute` tool — never do arithmetic yourself. "
    "You may DIRECT the two ACTING officers to act: `direct_coo` and `direct_cto` "
    "each take an imperative and a rationale; the officer's OWN agent self-verifies "
    "and acts via its audited lever. You BYPASS NO GUARDRAIL — if the officer judges "
    "the action unwarranted it refuses, and that is a valid outcome. The direct tool "
    "reads back the officer's ledger row: report HONESTLY whether a lever actually "
    "FIRED (officer_acted) — never claim an action completed when officer_acted is "
    "false. The CFO and the CXO are ANALYST seats with NO levers: you may consult "
    "them but you CANNOT direct them (there is no direct_cfo / direct_cxo). "
    "Only direct an officer when the grounded picture warrants it; otherwise "
    "OBSERVE and RECOMMEND. Use the harness: PLAN the meeting with write_plan, keep "
    "a todo list with write_todos, RECALL relevant memory before and RECORD durable "
    "executive notes after, and SPAWN a subagent for a focused deep-dive. Your "
    "signature output is a grounded EXECUTIVE BRIEF: a finance/ops/platform/CX "
    "synthesis with every figure attributed, the top cross-functional priorities "
    "and risks, and any directive you took with its verified outcome."
)


async def ask(settings: Settings, message: str, thread_id: Optional[str] = None,
              *, memory=None) -> dict:
    tools = get_tools(settings)
    return await runtime.ask(settings=settings, message=message, prompt=CEO_PROMPT,
                             model=mf.llm(), tools=tools, agent="ceo",
                             thread_id=thread_id, memory=memory,
                             claims_fn=ceo_claims.unsupported_claims)


async def ask_stream(settings: Settings, message: str,
                     thread_id: Optional[str] = None, *, memory=None
                     ) -> AsyncIterator[dict]:
    tools = get_tools(settings)
    async for chunk in runtime.ask_stream(settings=settings, message=message,
                                          prompt=CEO_PROMPT, model=mf.llm(),
                                          tools=tools, agent="ceo",
                                          thread_id=thread_id, memory=memory,
                                          claims_fn=ceo_claims.unsupported_claims):
        yield chunk
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest ceo/tests/test_prompt.py -v`
Expected: PASS (3 passed).

- [ ] **Step 5: Commit**

```bash
git add ceo/agent.py ceo/tests/test_prompt.py
git commit -m "feat(ceo): agent.py — CEO_PROMPT (synthesize+direct) + ask/ask_stream over csuite harness"
```

---

## Task 9: `ceo/api.py` + `ceo/api_main.py` — the A2A surface

**Files:**
- Create: `ceo/api.py`, `ceo/api_main.py`
- Test: `ceo/tests/test_api.py`

**Interfaces:**
- Consumes: `ceo.agent.ask`/`ask_stream`, `ceo.config.Settings`, `ceo.model_factory`.
- Produces:
  - `def create_app(settings, ask_fn=None, probes=None, ask_stream_fn=None) -> FastAPI` with routes `GET /livez`, `GET /health`, `POST /ask` (`{message, thread_id?}`), `POST /ask/stream` (NDJSON).
  - `def build()` in `api_main.py`: `Settings.from_env()` → `mf.init_models(settings)` → `create_app(settings)`; `__main__` runs uvicorn on `settings.api_port`.

- [ ] **Step 1: Write the failing test**

Create `ceo/tests/test_api.py`:

```python
from fastapi.testclient import TestClient
from ceo.config import Settings
from ceo.api import create_app


async def _fake_ask(settings, message, thread_id=None):
    return {"answer": "brief: the CFO reports NIM 3.1%; no directive taken."}


def _client():
    app = create_app(Settings.from_env({}), ask_fn=_fake_ask, probes={})
    return TestClient(app)


def test_livez_ok():
    assert _client().get("/livez").json()["service"] == "ceo"


def test_health_reports_service():
    body = _client().get("/health").json()
    assert body["service"] == "ceo" and body["status"] == "ok"


def test_ask_delegates_to_ask_fn():
    r = _client().post("/ask", json={"message": "state of the bank?"})
    assert "CFO reports" in r.json()["answer"]
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest ceo/tests/test_api.py -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'ceo.api'`.

- [ ] **Step 3: Write minimal implementation**

Create `ceo/api.py`:

```python
from __future__ import annotations
import json
from typing import Callable, Optional

from fastapi import FastAPI
from fastapi.responses import StreamingResponse
from pydantic import BaseModel

from .config import Settings
from .agent import ask as default_ask
from .agent import ask_stream as default_ask_stream


class AskRequest(BaseModel):
    message: str
    thread_id: Optional[str] = None


def _default_probes(settings: Settings) -> dict:
    """Best-effort dependency probes for /health; each returns a bool, never raises."""
    def ollama() -> bool:
        from . import model_factory as mf
        return mf.backend_healthcheck(settings)

    def peers() -> bool:
        import httpx
        reg = settings.peer_registry()["peers"]
        ok = 0
        for url in reg.values():
            try:
                r = httpx.get(url.rstrip("/") + "/livez", timeout=3)
                ok += 1 if r.status_code == 200 else 0
            except Exception:  # noqa: BLE001
                pass
        return ok > 0

    def qdrant() -> bool:
        try:
            from qdrant_client import QdrantClient
            QdrantClient(url=settings.qdrant_url).get_collections()
            return True
        except Exception:  # noqa: BLE001
            return False

    return {"ollama": ollama, "peers": peers, "qdrant": qdrant}


def create_app(settings: Settings, ask_fn: Optional[Callable] = None,
               probes: Optional[dict] = None,
               ask_stream_fn: Optional[Callable] = None) -> FastAPI:
    ask_fn = ask_fn or default_ask
    ask_stream_fn = ask_stream_fn or default_ask_stream
    probes = probes if probes is not None else _default_probes(settings)
    app = FastAPI(title="nano-bank CEO")

    @app.get("/livez")
    def livez():
        # Liveness only: is the process up? No dependency probes / model round-trip.
        return {"status": "ok", "service": "ceo"}

    @app.get("/health")
    def health():
        checks = {}
        for name, probe in probes.items():
            try:
                checks[name] = bool(probe())
            except Exception:  # noqa: BLE001
                checks[name] = False
        return {"status": "ok", "service": "ceo", "checks": checks}

    @app.post("/ask")
    async def ask_endpoint(req: AskRequest):
        return await ask_fn(settings, req.message, req.thread_id)

    @app.post("/ask/stream")
    async def ask_stream_endpoint(req: AskRequest):
        async def gen():
            async for chunk in ask_stream_fn(settings, req.message, req.thread_id):
                yield json.dumps(chunk) + "\n"

        return StreamingResponse(gen(), media_type="application/x-ndjson")

    return app
```

Create `ceo/api_main.py`:

```python
"""Container entrypoint for the CEO A2A API: resolve the model at startup, serve."""
from __future__ import annotations
import uvicorn

from .config import Settings
from . import model_factory as mf
from .api import create_app


def build():
    settings = Settings.from_env()
    mf.init_models(settings)
    return settings, create_app(settings)


if __name__ == "__main__":
    settings, app = build()
    uvicorn.run(app, host="0.0.0.0", port=settings.api_port)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest ceo/tests/test_api.py -v`
Expected: PASS (3 passed). Then run the whole seat suite: `python -m pytest ceo/tests csuite/tests/test_collab.py -v` — all green.

- [ ] **Step 5: Commit**

```bash
git add ceo/api.py ceo/api_main.py ceo/tests/test_api.py
git commit -m "feat(ceo): api.py + api_main.py — /ask, /ask/stream, /livez, /health"
```

---

## Task 10: `ceo/requirements.txt` + `ceo/Dockerfile`

**Files:**
- Create: `ceo/requirements.txt`, `ceo/Dockerfile`

**Interfaces:**
- Consumes: the full `ceo/` package + `csuite/`. Adds `psycopg2-binary` (the CEO writes the ledger directly — the other analyst seats don't).

- [ ] **Step 1: Create `ceo/requirements.txt`**

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
httpx>=0.27,<1
psycopg2-binary>=2.9,<3
pytest>=8.0
```

- [ ] **Step 2: Create `ceo/Dockerfile`**

```dockerfile
# Build from the REPO ROOT so the shared csuite package is in context:
#   docker build -f ceo/Dockerfile -t nano-ceo:dev .
# (ceo consults its peers over HTTP and writes only the agent_action_ledger via
# psycopg2; no domain MCP package is vendored here.)
FROM python:3.12-slim
WORKDIR /app
COPY ceo/requirements.txt /app/requirements.txt
RUN pip install --no-cache-dir -r requirements.txt
COPY csuite /app/csuite
COPY ceo /app/ceo
ENV PYTHONUNBUFFERED=1
CMD ["python", "-m", "ceo.api_main"]
```

- [ ] **Step 3: Verify the image builds**

Run (from repo root): `docker build -f ceo/Dockerfile -t nano-ceo:dev .`
Expected: builds successfully; final line `naming to docker.io/library/nano-ceo:dev`.

- [ ] **Step 4: Commit**

```bash
git add ceo/requirements.txt ceo/Dockerfile
git commit -m "build(ceo): requirements + Dockerfile (adds psycopg2 for the ledger writer)"
```

---

## Task 11: `ceo/k8s/ceo.yaml` + `ceo/k8s/deploy.sh`

**Files:**
- Create: `ceo/k8s/ceo.yaml`, `ceo/k8s/deploy.sh`

**Interfaces:**
- Consumes: `nano-ceo:dev` image (Task 10), the Kind cluster + `nano-agent-secrets` + `postgres-service`. The CEO needs BOTH the officer peer URLs AND DB env (unlike the analyst seats).

- [ ] **Step 1: Create `ceo/k8s/ceo.yaml`**

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: ceo
  namespace: nano-bank
  labels: { app: ceo }
spec:
  replicas: 1
  selector: { matchLabels: { app: ceo } }
  template:
    metadata: { labels: { app: ceo } }
    spec:
      containers:
      - name: ceo
        image: nano-ceo:dev
        imagePullPolicy: Never
        ports: [ { containerPort: 8099 } ]
        envFrom:
        - secretRef: { name: nano-agent-secrets }   # provides OLLAMA_API_KEY
        env:
        - { name: CFO_URL,            value: http://cfo:8089 }
        - { name: COO_URL,            value: http://coo:8093 }
        - { name: CTO_URL,            value: http://cto:8095 }
        - { name: CXO_URL,            value: http://cxo:8098 }
        - { name: OLLAMA_BASE_URL,    value: https://ollama.com/v1 }
        - { name: CEO_MODEL,          value: kimi-k2.6 }
        - { name: CEO_MODEL_FALLBACK, value: kimi-k2.6 }
        - { name: API_PORT,           value: "8099" }
        - { name: QDRANT_URL,         value: http://agent-qdrant:6333 }
        - { name: MEMORY_NAMESPACE,   value: ceo }
        - { name: DB_HOST,            value: postgres-service }
        - { name: DB_PORT,            value: "5432" }
        - { name: DB_NAME,            value: nano_bank_db }
        - { name: DB_USER,            value: nanobank_user }
        - { name: DB_PASSWORD,        value: "secure_nano_password_2024!" }
        livenessProbe:
          httpGet: { path: /livez, port: 8099 }
          initialDelaySeconds: 5
          periodSeconds: 10
        readinessProbe:
          httpGet: { path: /health, port: 8099 }
          initialDelaySeconds: 10
          periodSeconds: 30
          timeoutSeconds: 10
---
apiVersion: v1
kind: Service
metadata:
  name: ceo
  namespace: nano-bank
spec:
  selector: { app: ceo }
  ports: [ { port: 8099, targetPort: 8099 } ]
```

- [ ] **Step 2: Create `ceo/k8s/deploy.sh`** (mirror `cxo/k8s/deploy.sh` — read it first for the exact house form, then adapt names/ports)

```bash
#!/usr/bin/env bash
# Build the CEO image, load it into Kind, and apply the manifest.
#   ceo/k8s/deploy.sh
# Prereqs: the bank stack up (postgres + bank-api), the four officer seats
# (cfo/coo/cto/cxo) deployed, and nano-agent-secrets minted.
set -euo pipefail
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1000}"
export XDG_DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
cd "$(dirname "$0")/../.."          # -> repo root
CTX=kind-nano-bank
NS=nano-bank

docker build -f ceo/Dockerfile -t nano-ceo:dev .
kind load docker-image nano-ceo:dev --name nano-bank
kubectl --context "$CTX" -n "$NS" apply -f ceo/k8s/ceo.yaml
kubectl --context "$CTX" -n "$NS" rollout status deploy/ceo --timeout=120s
```

- [ ] **Step 3: Make it executable + verify apply (dry-run if no cluster)**

Run: `chmod +x ceo/k8s/deploy.sh && kubectl apply --dry-run=client -f ceo/k8s/ceo.yaml`
Expected: `deployment.apps/ceo created (dry run)` and `service/ceo created (dry run)`.
(If a live cluster + peers are up, run `ceo/k8s/deploy.sh` and expect `deployment "ceo" successfully rolled out`.)

- [ ] **Step 4: Commit**

```bash
git add ceo/k8s/ceo.yaml ceo/k8s/deploy.sh
git commit -m "deploy(ceo): k8s Deployment+Service (:8099) with peer URLs + DB env, and deploy.sh"
```

---

## Task 12: `demos/10-ceo/` — the C-suite meeting driver

**Files:**
- Create: `demos/10-ceo/drive.py`, `demos/10-ceo/run-demo.sh`, `demos/10-ceo/README.md`

**Interfaces:**
- Consumes: `demos/_driver.py` `run(BEATS, api_url, agent_label, run_hint)` (read `demos/09-cxo/drive.py` and `demos/_driver.py` first for the exact BEAT dict keys — `title`, `shows`, `message`, `thread`, optional `outcome_hint` — and the CLI). The CEO is reached at `CEO_API_URL` (default `http://localhost:8099`).

- [ ] **Step 1: Read the driver contract**

Run: `sed -n '1,60p' demos/_driver.py` and re-read `demos/09-cxo/drive.py`. Confirm the BEAT keys and the `run(...)` signature before writing (do not guess key names).

- [ ] **Step 2: Create `demos/10-ceo/drive.py`**

```python
#!/usr/bin/env python3
"""Narrated CEO demo — a C-SUITE MEETING. The CEO chairs the board: it goes round
the table consulting each officer, synthesizes a grounded executive brief, then
makes ONE decision — a directive to an acting officer (the COO), whose own agent
acts via its lever. The directive reads back the officer's ledger row to prove the
lever fired, and the CEO records its own directive row. Setup (run-demo.sh) seeds a
pending AFT batch so the directive has something real to act on.

    CEO_API_URL=http://localhost:8099 python demos/10-ceo/drive.py
    python demos/10-ceo/drive.py --beats 1,6      # agenda + verified minutes only
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))  # demos/
from _driver import run  # noqa: E402

BEATS = [
    {
        "title": "Call to order — the agenda",
        "shows": "the CEO opens the meeting and frames the cross-functional question",
        "message": "Chair a C-suite meeting. Agenda: state of the bank this week. "
                   "State the question you will put to each officer, in one line each.",
        "thread": "board",
    },
    {
        "title": "Round the table — CFO + COO",
        "shows": "the CEO consults the finance + operations seats; figures attributed",
        "message": "Go round the table. First the CFO: what do the books say this "
                   "week (profitability / NIM / capital)? Then the COO: how are the "
                   "payment rails and settlement running? Attribute every figure.",
        "thread": "board",
    },
    {
        "title": "Round the table — CTO + CXO",
        "shows": "the CEO consults the platform + experience seats; figures attributed",
        "message": "Continue round the table. The CTO: is the platform healthy "
                   "(reliability, deployments, incidents)? The CXO: how is the "
                   "customer experience and what is the customer voice saying? "
                   "Attribute every figure to the officer who gave it.",
        "thread": "board",
    },
    {
        "title": "The CEO's synthesis",
        "shows": "the signature output: a cross-functional brief — priorities + risks",
        "message": "Synthesize the four reports into an executive brief: the top "
                   "cross-functional priorities and the top risks, each citing the "
                   "officer and figure that motivates it.",
        "thread": "board",
    },
    {
        "title": "A decision — direct the COO",
        "shows": "the CEO directs an acting officer; the officer acts via its own lever",
        "message": "From that picture, if there is a pending AFT batch that should be "
                   "cut, DIRECT the COO to cut it — give the COO the imperative and "
                   "your rationale. Then tell me exactly what the COO did.",
        "thread": "board",
        "outcome_hint": "acted",
    },
    {
        "title": "Verified minutes",
        "shows": "read-back honesty: did a lever actually fire? the CEO reports the truth",
        "message": "Close the meeting. Did the directive actually fire a lever "
                   "(officer_acted)? Record the minutes: the decision you took and "
                   "the officer action it caused, with the ledger evidence.",
        "thread": "board",
        "outcome_hint": "acted",
    },
]

if __name__ == "__main__":
    raise SystemExit(run(
        BEATS,
        api_url=os.environ.get("CEO_API_URL", "http://localhost:8099"),
        agent_label="Agent CEO",
        run_hint="demos/10-ceo/run-demo.sh",
    ))
```

- [ ] **Step 3: Create `demos/10-ceo/run-demo.sh`** (mirror `demos/09-cxo/run-demo.sh`; read it for the exact `pf()`/cleanup/port-forward house form, then adapt)

```bash
#!/usr/bin/env bash
# One-command CEO demo: bring the C-suite up (cfo+coo+cto+cxo+ceo), seed a pending
# AFT batch so the directive has something real to cut, and run the narrated
# C-suite-meeting arc (demos/10-ceo/drive.py). The CEO consults all four officers,
# synthesizes a brief, and directs the COO — whose lever cuts the batch; the CEO's
# directive row reads back the COO's ledger row.
#
#   demos/10-ceo/run-demo.sh            # up (if needed) -> seed -> drive
#   demos/10-ceo/run-demo.sh --no-up    # assume the C-suite is already deployed
#   demos/10-ceo/run-demo.sh --no-seed  # drive against the estate as-is
#
# Prereqs: docker + kind + kubectl + uv, the bank stack up (postgres + bank-api),
# the four officer seats deployed, and nano-agent-secrets minted.
set -euo pipefail
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1000}"
export XDG_DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
cd "$(dirname "$0")/../.."          # -> repo root
CTX=kind-nano-bank
NS=nano-bank

DO_UP=1 DO_SEED=1
EMIT_ARG=""
while [ $# -gt 0 ]; do
  case "$1" in
    --no-up)      DO_UP=0 ;;
    --no-seed)    DO_SEED=0 ;;
    --emit-jsonl) EMIT_ARG="--emit-jsonl $2"; shift ;;
    *) echo "unknown flag: $1"; exit 2 ;;
  esac
  shift
done

PF_PIDS=()
cleanup() { for pid in "${PF_PIDS[@]:-}"; do kill "$pid" 2>/dev/null || true; done; }
trap 'cleanup' EXIT

pf() {  # svc localport [--address ::1]
  local svc="$1" port="$2"; shift 2
  kubectl --context "$CTX" -n "$NS" port-forward "$@" "svc/$svc" "$port:$port" \
    >"/tmp/ceo-demo-pf-$svc.log" 2>&1 &
  PF_PIDS+=("$!")
}

if [ "$DO_UP" = 1 ]; then
  ceo/k8s/deploy.sh
fi

# Seed a pending AFT batch (the COO's cut_aft_batch lever needs an open batch with
# entries). Reuse the AFT simulator's originate path; see testing/aft/README.md.
if [ "$DO_SEED" = 1 ]; then
  echo "seeding a pending AFT batch via the AFT simulator ..."
  # NOTE: implement using testing/aft/aft_simulator.py originate calls against
  # bank-api; leave a batch OPEN (do not submit) so the COO can cut it.
fi

pf ceo 8099
sleep 2
CEO_API_URL="http://localhost:8099" python demos/10-ceo/drive.py $EMIT_ARG
```

Note: the seed block references `testing/aft/aft_simulator.py`. During execution, wire the actual originate calls that leave one batch open (read `testing/aft/README.md` + the COO's `cut_aft_batch` lever to confirm what "a cuttable batch" requires). If seeding proves involved, land the driver + a `--no-seed` path first and file the seed helper as a follow-up step — the driver must run against a manually-seeded batch.

- [ ] **Step 4: Create `demos/10-ceo/README.md`** — a short doc: what the demo shows (a C-suite meeting: round-the-table consults → synthesis → a directive decision → verified minutes), how to run it (`run-demo.sh`, `--no-up`, `--no-seed`), and the prereqs (four officer seats + a pending AFT batch). Keep it to ~30 lines mirroring `demos/09-cxo/README.md`.

- [ ] **Step 5: Verify the driver imports + lists beats offline**

Run: `chmod +x demos/10-ceo/run-demo.sh && python -c "import importlib.util,sys; spec=importlib.util.spec_from_file_location('d','demos/10-ceo/drive.py'); m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m); print(len(m.BEATS),'beats'); assert [b['title'] for b in m.BEATS][0].startswith('Call to order')"`
Expected: `6 beats` printed, no error. (This imports `demos/_driver.py`; if it needs a running API it must still import cleanly — the driver only connects when `run()` is called.)

- [ ] **Step 6: Commit**

```bash
git add demos/10-ceo/drive.py demos/10-ceo/run-demo.sh demos/10-ceo/README.md
git commit -m "demo(ceo): C-suite meeting arc — round-the-table consults, synthesis, a COO directive"
```

---

## Task 13: `demos/10-ceo/present/` — the meeting presentation console

**Files:**
- Create: `demos/10-ceo/present/state.py`, `present/app.py`, `present/requirements.txt`, `present/README.md`, `present/tests/__init__.py`, `present/tests/test_state.py`

**Interfaces:**
- Consumes: the CXO present console as the template (`demos/09-cxo/present/state.py` + `app.py`) — read both first. Reuses the same JSONL-beat-stream + recording model. The CEO twist: a "minutes" view that pairs the CEO directive row with the officer row it caused.
- Produces (in `state.py`): `read_jsonl(text)`, `save_recording(dir_, beats, meta=None)`, `load_recording(path)`, `latest_recording(dir_)`, `beat_catalog(drive_path)` (parse `BEATS` from `drive.py` via `ast`, no import), and a CEO-specific `outcome_chip(kind) -> str` mapping `"acted" → "🟢 lever fired"`, `"deferred" → "🟡 no action"`, `"read_only" → "⚪ report"`.

- [ ] **Step 1: Write the failing test**

Create `demos/10-ceo/present/tests/test_state.py`:

```python
import os, sys
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import state  # noqa: E402


def test_read_jsonl_skips_blank_and_partial_lines():
    text = '{"event": 1}\n\n{"final": 2}\n{bad partial'
    rows = state.read_jsonl(text)
    assert rows == [{"event": 1}, {"final": 2}]


def test_beat_catalog_parses_titles_without_importing(tmp_path):
    drive = tmp_path / "drive.py"
    drive.write_text(
        'BEATS = [\n'
        '  {"title": "Call to order — the agenda", "shows": "x", "message": "m", "thread": "board"},\n'
        '  {"title": "Verified minutes", "shows": "y", "message": "m2", "thread": "board"},\n'
        ']\n')
    cat = state.beat_catalog(str(drive))
    assert [b["title"] for b in cat] == ["Call to order — the agenda", "Verified minutes"]


def test_outcome_chip_marks_lever_fired():
    assert "fired" in state.outcome_chip("acted").lower()
    assert state.outcome_chip("unknown") == ""
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest demos/10-ceo/present/tests/test_state.py -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'state'`.

- [ ] **Step 3: Write minimal implementation**

Create `demos/10-ceo/present/state.py` — copy the pure helpers from `demos/09-cxo/present/state.py` (`read_jsonl`, `save_recording`, `load_recording`, `latest_recording`, `beat_catalog`) verbatim, then add:

```python
def outcome_chip(kind: str) -> str:
    return {"acted": "🟢 lever fired",
            "deferred": "🟡 no action",
            "read_only": "⚪ report"}.get(kind, "")
```

(If the copied `save_recording` in the CXO version takes a `scorecard` arg, keep its signature as-is — the test only calls `read_jsonl`, `beat_catalog`, `outcome_chip`.)

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest demos/10-ceo/present/tests/test_state.py -v`
Expected: PASS (3 passed).

- [ ] **Step 5: Write `present/app.py`, `requirements.txt`, `README.md`**

Create `present/app.py` — a Streamlit console modeled on `demos/09-cxo/present/app.py`: a per-beat stepper (buttons from `state.beat_catalog("demos/10-ceo/drive.py")`), a panel per officer that lights up as its consult streams, the CEO synthesis panel, and a **"minutes"** footer that, for the directive beat, renders the CEO directive row beside the officer row it caused (using `outcome_chip` on the beat's `outcome_hint`). It runs on port `8511` (`streamlit run present/app.py --server.port 8511`). Reuse the CXO app's JSONL-stream reading and recording load/save wiring; swap the CX scorecard for the officer-panels + minutes layout. Create `present/requirements.txt` (copy `demos/09-cxo/present/requirements.txt`) and a short `present/README.md` (how to run the console on :8511, how it replays a recording).

- [ ] **Step 6: Verify the console imports offline**

Run: `python -c "import ast; ast.parse(open('demos/10-ceo/present/app.py').read()); print('app.py parses')"`
Expected: `app.py parses`. (Full Streamlit run is a live check under `run-demo.sh`.)

- [ ] **Step 7: Commit**

```bash
git add demos/10-ceo/present
git commit -m "demo(ceo): presentation console (:8511) — officer panels + verified-minutes footer"
```

---

## Task 14: Estate wiring + docs

**Files:**
- Modify: `CLAUDE.md` (repo root) — add a short "Agent CEO" bullet to the C-suite section noting the seat (`ceo/` :8099), the shared `csuite/collab.py` primitive, and the meeting demo.
- Modify: `scripts/deploy-all.sh` (if it enumerates the seats) — add the CEO deploy step after the CXO. Read it first; if it does not enumerate seats, skip this edit.

**Interfaces:** documentation only; no code.

- [ ] **Step 1: Read the targets**

Run: `grep -n "cxo\|CXO\|coo/k8s\|cxo/k8s" scripts/deploy-all.sh CLAUDE.md`
Confirm where the CXO is wired so the CEO slots in the same way.

- [ ] **Step 2: Add the CEO to `scripts/deploy-all.sh`** (only if it lists seats)

Add, after the CXO line, following the file's existing form (e.g. `ceo/k8s/deploy.sh` invocation). If the script uses an array of seats, append `ceo`.

- [ ] **Step 3: Document the seat in `CLAUDE.md`**

Add a bullet to the C-suite section:

```markdown
- **Agent CEO** (`ceo/` :8099) — the capstone synthesizer seat: consults the four
  officers, produces a grounded executive brief (every figure attributed), and can
  DIRECT the two acting officers (COO/CTO) via the shared `csuite/collab.py`
  consult/direct primitive. A directive posts an imperative to the peer's `/ask`,
  the peer acts via its own audited lever, and the CEO reads back the peer's ledger
  row to prove a lever fired before recording its own `append_agent_action('ceo',…)`
  directive row. Demo: `demos/10-ceo/run-demo.sh` (a C-suite meeting; present
  console :8511). No bank/Rust change.
```

- [ ] **Step 4: Run the whole suite once more**

Run: `python -m pytest csuite/tests/test_collab.py ceo/tests demos/10-ceo/present/tests -v`
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add CLAUDE.md scripts/deploy-all.sh
git commit -m "docs(ceo): wire the CEO seat into the estate docs + deploy-all"
```

---

## Self-Review

**1. Spec coverage:**
- `csuite/collab.py` consult/direct primitive → Tasks 1–3. ✓
- Read-back verify (before/after `MAX(seq)`) → Tasks 2 (logic) + 5 (queries). ✓
- Directable = COO+CTO, consult-only = CFO+CXO → Tasks 3, 4, 7, 8. ✓
- `ceo/` seat mirroring `cxo/` (config/model_factory/claims/tools/agent/api/api_main/Dockerfile/k8s/tests) → Tasks 4–11. ✓
- `ceo/audit.py` sole writer via `append_agent_action('ceo',…)`, no bank change → Task 5. ✓
- Directive row shape (params: directive+rationale; effect: officer_acted/officer_row/officer_response) → Tasks 2 + 5. ✓
- CEO prompt/lane + grounding (number verifier + directive-honesty guard) → Tasks 6, 8. ✓
- Ports 8099 / 8511, peer URLs, nano-agent-secrets, kimi-k2.6 → Tasks 4, 9, 11. ✓
- C-suite-meeting demo + present console → Tasks 12–13. ✓
- Phase-2 board line = out of scope (noted, no task) → correct. ✓

**2. Placeholder scan:** The one deferred detail is the AFT-batch seed helper in `run-demo.sh` (Task 12 Step 3), explicitly flagged with a concrete instruction (use `testing/aft/aft_simulator.py`, leave a batch open) and a `--no-seed` fallback — an executor-time wiring step against a real script, not a plan gap. The `present/app.py` (Task 13 Step 5) is specified by concrete behavior + a named template file rather than full code, because it is a Streamlit view best matched to the existing CXO console; its pure logic (`state.py`) is fully TDD'd.

**3. Type consistency:** `AuditPort` methods (`latest_actor_seq`, `rows_since`, `direct`) are named identically in Tasks 2 (protocol + fake), 5 (concrete `Audit`), and 7 (`_Audit` test double). `Settings.peer_registry()` shape (`{"peers":…, "directable":…}`) matches between Task 4 (producer), Task 3 (`build_tools` consumer), and Task 7. Tool names (`consult_<peer>`, `direct_<peer>`) are consistent across Tasks 1–3, 7, 8, and the demo. `post_ask` returns the raw `/ask` JSON dict (`.get("answer")`) everywhere.
