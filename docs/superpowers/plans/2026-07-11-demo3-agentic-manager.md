# Demo 3 — Agentic Manager Console Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Streamlit demo with per-action chat boxes (open account / register Interac payee / transactions) driving the personal manager, each with a LangSmith-like trace panel — backed by a new `open_account` manager tool and a run-trace surfaced from `assist()`.

**Architecture:** Two small agent-backend additions (an immediate `open_account` MCP tool; a `TraceRecorder` callback whose events `assist()` returns as `trace`), then a Streamlit app talking to the Branch API (`:8086`). Per-box `thread_id`s. No confirm-gate changes; trace is telemetry only.

**Tech Stack:** Python 3.12 (agent), LangChain/LangGraph, pytest, Streamlit, Docker/kind/kubectl.

## Global Constraints

- **Money movement stays two-phase confirm-gated** (`propose_*` → confirm). `open_account` and `register_interac_recipient` are immediate (not money movement).
- **Trace is best-effort telemetry** — it must never change the answer or gate. JSON-serializable primitives only.
- **Agent boundary unchanged:** reads via the read-only DB view, writes via `BankClient`. `open_account` writes via `bank.create_account` with the customer token.
- **Branch:** `agent-k8s-e2e` (PR #22). Agent venv `agent/.venv`. kubectl context `kind-nano-bank`.
- **Backend changes live in `agent-mcp` (tools) and `agent-api` (assist)** — both images rebuild on deploy.

## File Structure

- Modify `agent/mcp_server.py` — `open_account` tool + `LLM_TOOL_NAMES`.
- Create `agent/trace.py` — `TraceRecorder`.
- Modify `agent/nano_manager.py` — wire the recorder into `assist()`, return `trace`.
- Create `agent/tests/test_trace.py`; extend `agent/tests/test_interac_tools.py`.
- Create `demos/03-agentic-manager/{app.py,requirements.txt}`; update `demos/README.md`.

---

## Task 1: `open_account` manager tool

**Files:**
- Modify: `agent/mcp_server.py`
- Test: `agent/tests/test_interac_tools.py`

**Interfaces:**
- Consumes: `deps.bank.create_account(token, payload)` (exists), `current_token()`.
- Produces: MCP tool `open_account(account_type: str) -> dict`; `"open_account"` in `LLM_TOOL_NAMES`.

- [ ] **Step 1: Write the failing test**

Append to `agent/tests/test_interac_tools.py`:
```python
def test_open_account_tool_registered():
    from agent.mcp_server import LLM_TOOL_NAMES
    assert "open_account" in LLM_TOOL_NAMES
```

- [ ] **Step 2: Run to verify fail**

Run: `agent/.venv/bin/python -m pytest agent/tests/test_interac_tools.py -q -k open_account`
Expected: FAIL (`open_account` not in the set).

- [ ] **Step 3: Implement**

In `agent/mcp_server.py`, add `"open_account"` to `LLM_TOOL_NAMES`:
```python
LLM_TOOL_NAMES = frozenset({
    "get_profile", "get_accounts", "get_transactions", "get_cards",
    "recall", "remember", "propose_transfer", "propose_deposit", "propose_withdraw",
    "register_interac_recipient", "list_interac_recipients",
    "remove_interac_recipient", "propose_interac_transfer", "open_account"})
```
Add the tool inside `build_mcp` (next to `register_interac_recipient`):
```python
    @mcp.tool()
    def open_account(account_type: str) -> dict:
        """Open a new account for the bound client. account_type is one of
        'chequing', 'savings', 'credit_card'. Opening an account is immediate
        (not money movement)."""
        return deps.bank.create_account(current_token(), {"account_type": account_type})
```

- [ ] **Step 4: Run to verify pass**

Run: `agent/.venv/bin/python -m pytest agent/tests/test_interac_tools.py -q`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add agent/mcp_server.py agent/tests/test_interac_tools.py
git commit -m "feat(agent): open_account manager tool (immediate, like payee registration)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 2: `TraceRecorder` callback + wire into `assist()`

**Files:**
- Create: `agent/trace.py`
- Modify: `agent/nano_manager.py`
- Test: `agent/tests/test_trace.py`

**Interfaces:**
- Produces:
  - `class TraceRecorder(BaseCallbackHandler)` with `events() -> list[dict]`, each event
    `{seq:int, kind:"tool"|"model", name:str, ok:bool, elapsed_ms:int, input:str|None, output:str|None, error:str|None}`.
  - `assist(...)` return dict gains `trace: list[dict]`.

- [ ] **Step 1: Write the failing test**

Create `agent/tests/test_trace.py`:
```python
from agent.trace import TraceRecorder


def test_tool_start_end_produces_one_event():
    r = TraceRecorder()
    r.on_tool_start({"name": "get_accounts"}, "{}", run_id="a")
    r.on_tool_end("[{'account_id': 'x'}]", run_id="a")
    evs = r.events()
    assert len(evs) == 1
    e = evs[0]
    assert e["kind"] == "tool" and e["name"] == "get_accounts" and e["ok"] is True
    assert "account_id" in e["output"] and isinstance(e["elapsed_ms"], int)
    assert e["seq"] == 0


def test_tool_error_marks_not_ok():
    r = TraceRecorder()
    r.on_tool_start({"name": "propose_transfer"}, "{...}", run_id="b")
    r.on_tool_error(ValueError("nope"), run_id="b")
    e = r.events()[0]
    assert e["ok"] is False and "nope" in e["error"]
```

- [ ] **Step 2: Run to verify fail**

Run: `agent/.venv/bin/python -m pytest agent/tests/test_trace.py -q`
Expected: FAIL (`ModuleNotFoundError: agent.trace`).

- [ ] **Step 3: Implement the recorder**

Create `agent/trace.py`:
```python
from __future__ import annotations
import time
from typing import Any, Optional

from langchain_core.callbacks import BaseCallbackHandler


def _short(x: Any, n: int = 2000) -> str:
    s = x if isinstance(x, str) else str(x)
    return s if len(s) <= n else s[:n] + "…"


class TraceRecorder(BaseCallbackHandler):
    """Records tool/model steps of a LangGraph run as ordered, JSON-safe events."""

    def __init__(self):
        self._open: dict = {}      # run_id -> {kind, name, t0, input}
        self._events: list[dict] = []

    # --- tools ---
    def on_tool_start(self, serialized, input_str, **kwargs):
        rid = kwargs.get("run_id")
        name = (serialized or {}).get("name", "tool")
        self._open[rid] = {"kind": "tool", "name": name,
                           "t0": time.perf_counter(), "input": _short(input_str)}

    def on_tool_end(self, output, **kwargs):
        self._close(kwargs.get("run_id"), ok=True, output=_short(output))

    def on_tool_error(self, error, **kwargs):
        self._close(kwargs.get("run_id"), ok=False, error=_short(error))

    # --- model ---
    def on_chat_model_start(self, serialized, messages, **kwargs):
        rid = kwargs.get("run_id")
        name = (serialized or {}).get("name", "model")
        self._open[rid] = {"kind": "model", "name": name,
                           "t0": time.perf_counter(), "input": None}

    def on_llm_end(self, response, **kwargs):
        rid = kwargs.get("run_id")
        if rid in self._open:
            self._close(rid, ok=True, output=None)

    def _close(self, rid, *, ok, output=None, error=None):
        info = self._open.pop(rid, None)
        if info is None:
            return
        self._events.append({
            "seq": len(self._events), "kind": info["kind"], "name": info["name"],
            "ok": ok, "elapsed_ms": int((time.perf_counter() - info["t0"]) * 1000),
            "input": info.get("input"), "output": output, "error": error,
        })

    def events(self) -> list[dict]:
        return list(self._events)
```

- [ ] **Step 4: Run to verify pass**

Run: `agent/.venv/bin/python -m pytest agent/tests/test_trace.py -q`
Expected: PASS (2 tests).

- [ ] **Step 5: Wire into `assist()`**

In `agent/nano_manager.py`, add the import near the top imports:
```python
from .trace import TraceRecorder
```
Replace the `agent.ainvoke` call to pass a recorder via callbacks:
```python
    rec = TraceRecorder()
    agent = create_react_agent(mf.llm("fast"), tools, prompt=MANAGER_PROMPT,
                               checkpointer=InMemorySaver())
    out = await agent.ainvoke(
        {"messages": [context, HumanMessage(message)]},
        config={"configurable": {"thread_id": thread_id}, "recursion_limit": 40,
                "callbacks": [rec]})
```
Add `trace` to the return dict:
```python
    res = {"answer": answer, "thread_id": thread_id, "trace": rec.events()}
    if pending:
        res["pending_action"] = pending
    return res
```

- [ ] **Step 6: Run the full agent suite**

Run: `agent/.venv/bin/python -m pytest agent -q`
Expected: PASS (previous + new trace tests).

- [ ] **Step 7: Commit**

```bash
git add agent/trace.py agent/nano_manager.py agent/tests/test_trace.py
git commit -m "feat(agent): TraceRecorder callback — assist() returns a run trace

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 3: the demo — `demos/03-agentic-manager/`

**Files:**
- Create: `demos/03-agentic-manager/app.py`, `demos/03-agentic-manager/requirements.txt`
- Modify: `demos/README.md`

**Interfaces:**
- Consumes the Branch API: `POST /branch/seed`,
  `GET /branch/clients/{cid}/{profile,accounts}`,
  `POST /branch/clients/{cid}/message` `{message, thread_id}` → `{answer, thread_id, pending_action?, trace}`,
  `POST /branch/clients/{cid}/actions/{aid}/{confirm,cancel}`.

- [ ] **Step 1: Write the app**

Create `demos/03-agentic-manager/app.py`:
```python
"""Agentic manager console — per-action chat boxes with LangSmith-like traces.

Talks to the personal manager's Branch API (:8086). Seed a demo client, then use
three action boxes (open account / register Interac payee / transactions); each
box is its own conversation thread and shows the manager's run trace.

Config: DEMO_BRANCH_BASE (default http://localhost:8086) + DEMO_BRANCH_TOKEN
(the BRANCH_SERVICE_TOKEN). See demos/README.md.
"""
from __future__ import annotations
import os
import json
import requests
import streamlit as st

API = os.environ.get("DEMO_BRANCH_BASE", "http://localhost:8086").rstrip("/")
TOKEN = os.environ.get("DEMO_BRANCH_TOKEN", "")
HDR = {"Authorization": f"Bearer {TOKEN}"}
TIMEOUT = 180

st.set_page_config(page_title="nano-bank · agentic manager", layout="wide")
ss = st.session_state
ss.setdefault("clients", [])
ss.setdefault("cid", None)
ss.setdefault("threads", {})     # box_key -> thread_id
ss.setdefault("last", {})        # box_key -> last response dict
ss.setdefault("pending", {})     # box_key -> pending_action


def _post(path, body=None):
    try:
        r = requests.post(f"{API}{path}", json=body, headers=HDR, timeout=TIMEOUT)
        return r.status_code, (r.json() if r.content else {})
    except requests.RequestException as e:
        return 0, {"error": str(e)}


def _get(path):
    try:
        r = requests.get(f"{API}{path}", headers=HDR, timeout=60)
        return r.status_code, (r.json() if r.content else {})
    except requests.RequestException as e:
        return 0, {"error": str(e)}


st.title("🤖 nano-bank — agentic manager console")
st.caption(f"Branch API: `{API}` · seed a client, then chat by action with live traces")

# --- client picker ----------------------------------------------------------
c1, c2 = st.columns([1, 2])
with c1:
    if st.button("🌱 Seed a demo client"):
        code, body = _post("/branch/seed")
        ss["clients"] = body.get("customers", []) if isinstance(body, dict) else []
        if ss["clients"]:
            ss["cid"] = ss["clients"][0]["customer_id"]
        st.rerun()
    if ss["clients"]:
        ss["cid"] = st.selectbox("Active client",
                                 [c["customer_id"] for c in ss["clients"]],
                                 index=0)
with c2:
    if ss["cid"]:
        _, prof = _get(f"/branch/clients/{ss['cid']}/profile")
        _, accts = _get(f"/branch/clients/{ss['cid']}/accounts")
        if isinstance(prof, dict):
            st.markdown(f"**Client:** {prof.get('first_name','?')} {prof.get('last_name','')} "
                        f"· `{ss['cid'][:8]}`")
        if isinstance(accts, list) and accts:
            st.table([{"type": a.get("account_type"), "balance": a.get("balance"),
                       "id": a.get("account_id", "")[:8]} for a in accts])

if not ss["cid"]:
    st.info("Seed a demo client to begin.")
    st.stop()


def _render_trace(trace):
    if not trace:
        st.caption("no trace")
        return
    for e in trace:
        icon = "🔧" if e["kind"] == "tool" else "🧠"
        mark = "✅" if e.get("ok") else "❌"
        head = f"{icon} {mark} **{e['name']}** · {e['elapsed_ms']}ms"
        with st.expander(head, expanded=False):
            if e.get("input"):
                st.markdown("input"); st.code(e["input"])
            if e.get("output"):
                st.markdown("output"); st.code(e["output"])
            if e.get("error"):
                st.error(e["error"])


def action_box(title, key, placeholder):
    st.subheader(title)
    msg = st.text_input("Message", key=f"in_{key}", placeholder=placeholder)
    if st.button("Send", key=f"send_{key}") and msg:
        body = {"message": msg}
        if ss["threads"].get(key):
            body["thread_id"] = ss["threads"][key]
        code, data = _post(f"/branch/clients/{ss['cid']}/message", body)
        if isinstance(data, dict):
            ss["threads"][key] = data.get("thread_id", ss["threads"].get(key))
            ss["last"][key] = data
            ss["pending"][key] = data.get("pending_action")
        st.rerun()
    data = ss["last"].get(key)
    if data:
        st.markdown(f"**Manager:** {data.get('answer','')}")
        pa = ss["pending"].get(key)
        if pa:
            st.warning(f"Proposed: {pa.get('summary', pa.get('id'))}")
            b1, b2 = st.columns(2)
            if b1.button("Confirm", key=f"ok_{key}"):
                _post(f"/branch/clients/{ss['cid']}/actions/{pa['id']}/confirm")
                ss["pending"][key] = None
                st.rerun()
            if b2.button("Cancel", key=f"no_{key}"):
                _post(f"/branch/clients/{ss['cid']}/actions/{pa['id']}/cancel")
                ss["pending"][key] = None
                st.rerun()
        with st.expander("🪵 Interaction trace (LangSmith-style)", expanded=True):
            _render_trace(data.get("trace"))


action_box("① Open account", "open", "e.g. open a savings account")
st.divider()
action_box("② Register Interac payee", "payee", "e.g. register sam@example.ca as Sam")
st.divider()
action_box("③ Perform transactions", "txn",
           "e.g. deposit 100 to chequing · transfer 25 to savings · send 30 interac to sam@example.ca")
```

- [ ] **Step 2: requirements + README**

Create `demos/03-agentic-manager/requirements.txt`:
```
streamlit>=1.36
requests>=2.31
```
Add a row to the table in `demos/README.md`:
```markdown
| 3 | Agentic manager | `03-agentic-manager/` | Per-action chat boxes (open account / register Interac payee / transactions) driving the personal manager, each its own thread with a LangSmith-like run-trace panel. Talks to the Branch API (`:8086`). |
```
And note its different backend + run command under the run section:
```markdown
Demo 3 talks to the **manager Branch API** (`:8086`), not the bank API directly:

    kubectl --context kind-nano-bank -n nano-bank port-forward svc/agent-api 8086:8086
    TOKEN=$(grep -E '^BRANCH_SERVICE_TOKEN=' agent/.env | cut -d= -f2-)
    DEMO_BRANCH_BASE=http://localhost:8086 DEMO_BRANCH_TOKEN=$TOKEN \
      streamlit run demos/03-agentic-manager/app.py
```

- [ ] **Step 3: Verify (parse + boot)**

```bash
agent/.venv/bin/python -c "import ast; ast.parse(open('demos/03-agentic-manager/app.py').read()); print('parse-ok')"
agent/.venv/bin/python -m streamlit run demos/03-agentic-manager/app.py \
  --server.headless true --server.port 8598 >/tmp/d3.log 2>&1 &
sleep 8; curl -fsS -o /dev/null -w 'boot HTTP %{http_code}\n' http://localhost:8598/; \
  grep -iE "error|traceback" /tmp/d3.log | head || echo "(clean)"; pkill -f "port 8598" 2>/dev/null
```
Expected: `parse-ok`, `boot HTTP 200`, clean.

- [ ] **Step 4: Commit**

```bash
git add demos/
git commit -m "feat(demos): agentic manager console (demo 3) — per-action chats + traces

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 4: Deploy, live-verify, run demo 3

**Files:** none (deploy + verify)

- [ ] **Step 1: Rebuild + reload agent-mcp (open_account) and agent-api (trace)**

```bash
cd /home/bmartins/dev/nano-bank/agent
docker build -f Dockerfile.mcp -t nano-agent-mcp:dev . -q && kind load docker-image nano-agent-mcp:dev --name nano-bank
docker build -f Dockerfile.api -t nano-agent-api:dev . -q && kind load docker-image nano-agent-api:dev --name nano-bank
kubectl --context kind-nano-bank -n nano-bank rollout restart deploy/agent-mcp deploy/agent-api
kubectl --context kind-nano-bank -n nano-bank rollout status deploy/agent-mcp --timeout=180s
kubectl --context kind-nano-bank -n nano-bank rollout status deploy/agent-api --timeout=180s
```

- [ ] **Step 2: Live check — open_account tool + trace over the Branch API**

```bash
cd /home/bmartins/dev/nano-bank/agent
kubectl --context kind-nano-bank -n nano-bank port-forward svc/agent-api 8086:8086 >/tmp/pf.log 2>&1 &
PF=$!; sleep 5
TOKEN=$(grep -E '^BRANCH_SERVICE_TOKEN=' .env | cut -d= -f2-); H="Authorization: Bearer $TOKEN"
SEED=$(curl -fsS -m120 -X POST localhost:8086/branch/seed -H "$H")
CID=$(echo "$SEED" | python3 -c 'import sys,json;print(json.load(sys.stdin)["customers"][0]["customer_id"])')
curl -fsS -m150 -X POST "localhost:8086/branch/clients/$CID/message" -H "$H" \
  -H 'content-type: application/json' -d '{"message":"open a savings account for me"}' \
  | python3 -c 'import sys,json;d=json.load(sys.stdin);print("answer:",d["answer"][:120]);print("trace tools:",[e["name"] for e in d.get("trace",[]) if e["kind"]=="tool"])'
kill $PF 2>/dev/null
```
Expected: an answer confirming a savings account, and the trace's tool list
includes `open_account` (evidence the tool + trace both work end-to-end).

- [ ] **Step 3: Run demo 3 on the LAN (:8512)**

```bash
cd /home/bmartins/dev/nano-bank
kubectl --context kind-nano-bank -n nano-bank port-forward svc/agent-api 8086:8086 >/tmp/agentapi-pf.log 2>&1 &
sleep 4
TOKEN=$(grep -E '^BRANCH_SERVICE_TOKEN=' agent/.env | cut -d= -f2-)
setsid bash -c "DEMO_BRANCH_BASE=http://localhost:8086 DEMO_BRANCH_TOKEN=$TOKEN exec agent/.venv/bin/python -m streamlit run demos/03-agentic-manager/app.py --server.address 0.0.0.0 --server.port 8512 --server.headless true" >/tmp/demo3.log 2>&1 < /dev/null &
disown; sleep 9
curl -fsS -o /dev/null -w 'demo3 airig.local:8512 HTTP %{http_code}\n' http://airig.local:8512/
```
Expected: HTTP 200 — demo 3 reachable alongside demos 1 (`:8510`) and 2 (`:8511`).

- [ ] **Step 4: Commit (if any tweak) and push**

```bash
git add -A && git commit -m "chore(demos): demo 3 live-verified" || true
git push origin agent-k8s-e2e
```

---

## Self-Review notes

- **Spec coverage:** §backend open_account→T1, §trace callback→T2, §frontend (3 boxes, per-box threads, trace panel, seed client)→T3, §deploy+live→T4. Confirm-gate unchanged; `open_account`/payee immediate; money movement still `propose_*`.
- **Placeholder scan:** all code concrete (recorder, tool, full Streamlit app, exact curl checks). No TODOs.
- **Type/name consistency:** `TraceRecorder.events()` shape (`seq,kind,name,ok,elapsed_ms,input,output,error`) is produced in T2 and consumed by `_render_trace` in T3; `open_account(account_type)` name matches T1 tool + T2/T4 trace assertions; Branch API paths match `agent/api.py`.
- **Watch-outs:** `assist()` return gains `trace` (additive — the console in `agent/console.py` ignores unknown keys, so no break); the demo needs the agent-api port-forward + `BRANCH_SERVICE_TOKEN`; both `agent-mcp` and `agent-api` must be rebuilt (tool lives in mcp, trace in api's assist).
