"""External mandated agent console.

An autonomous LLM agent operates a customer's bank ONLY through the agentic
branch's /agent-gateway/*, under a customer-granted mandate (scoped, capped,
revocable). It never sees the bank. Seed a demo mandate, give a high-level
instruction, and watch the agent plan → act (mandate-gated) → ask the manager.

Config: DEMO_BRANCH_BASE (default http://localhost:8086) + AGENT_GATEWAY_TOKEN.
The demo builds the planner LLM locally (needs OLLAMA_API_KEY).
"""
from __future__ import annotations
import os
import requests
import streamlit as st

from agent.external_agent.agent import ExternalAgent, GatewayHTTP

BASE = os.environ.get("DEMO_BRANCH_BASE", "http://localhost:8086").rstrip("/")
TOKEN = os.environ.get("AGENT_GATEWAY_TOKEN", "")
HDR = {"Authorization": f"Bearer {TOKEN}"}

st.set_page_config(page_title="nano-bank · external agent", layout="wide")
ss = st.session_state
ss.setdefault("events", None)
ss.setdefault("instr", "Pay my $50 Epcor utility bill and tell me whether I should open a savings account.")

st.title("🛰️ nano-bank — external mandated agent")
st.caption(f"Gateway: `{BASE}/agent-gateway` · the agent's ONLY door — mandate-gated, capped, revocable")


@st.cache_resource(show_spinner=False)
def _llm():
    from agent import model_factory as mf
    from agent.config import Settings
    s = Settings.from_env()
    mf.init_models(s)
    return mf.llm("fast", temperature=0.0)


def _gw_post(path):
    return requests.post(f"{BASE}{path}", headers=HDR, timeout=180)


# --- mandate panel ----------------------------------------------------------
top = st.columns([3, 1, 1])
with top[0]:
    r = requests.get(f"{BASE}/agent-gateway/mandate", headers=HDR, timeout=30)
    if r.status_code == 200:
        m = r.json()
        st.success(f"**Mandate active** · account `{m.get('account_id','')[:8]}` "
                   f"({m.get('account_type','')}) · scopes: {', '.join(m.get('scopes', []))} "
                   f"· cap/tx: ${m.get('max_per_tx','—')} · expires {m.get('expires_at','')[:19]}")
    else:
        st.warning("No active mandate — click **Seed mandate** to register an agent + grant consent.")
with top[1]:
    if st.button("🌱 Seed mandate"):
        _gw_post("/agent-gateway/demo-seed")
        ss["events"] = None
        st.rerun()
with top[2]:
    if st.button("⛔ Revoke"):
        _gw_post("/agent-gateway/revoke")
        ss["events"] = None
        st.rerun()

st.divider()

# --- instruction + run ------------------------------------------------------
ss["instr"] = st.text_area("High-level instruction to the autonomous agent", ss["instr"], height=70)
if st.button("▶ Run agent", type="primary"):
    try:
        agent = ExternalAgent(gateway=GatewayHTTP(BASE, TOKEN), llm=_llm())
        with st.spinner("agent planning + acting through the gateway…"):
            ss["events"] = agent.run(ss["instr"])
    except Exception as e:  # noqa: BLE001
        st.error(f"agent run failed: {e}")
    st.rerun()


def _bubble(side, title, body, tone="neutral"):
    cols = st.columns([3, 2]) if side == "left" else st.columns([2, 3])
    col = cols[0] if side == "left" else cols[1]
    with col, st.container(border=True):
        st.markdown(title)
        if tone == "deny":
            st.error(body)
        elif tone == "allow":
            st.success(body)
        else:
            st.markdown(body)


def _trace_strip(trace):
    if not trace:
        return
    st.caption("trace: " + "  ·  ".join(
        f"{'🔧' if e['kind'] == 'tool' else '🧠'}{'✅' if e.get('ok') else '❌'} "
        f"{e['name']} {e['elapsed_ms']}ms" for e in trace))


# --- transcript -------------------------------------------------------------
if ss["events"]:
    for e in ss["events"]:
        if e["kind"] == "plan":
            st.subheader(f"🧠 Agent plan")
            st.caption(e["instruction"])
        elif e["kind"] == "act":
            res = e.get("result", {})
            dec = res.get("decision", "?")
            _bubble("left", f"🤖 **Agent → act**", f"`{e['operation']}` {e.get('params', {})}")
            tone = "allow" if dec == "allow" else "deny"
            detail = res.get("reason") or (res.get("result") if dec == "allow" else res)
            _bubble("right", f"🏦 **Gateway** · mandate check → **{dec}**", f"{detail}", tone=tone)
        elif e["kind"] == "message":
            _bubble("left", "🤖 **Agent → asks the manager**", e.get("text", ""))
            with st.columns([2, 3])[1], st.container(border=True):
                st.markdown("🏦 **Manager**")
                st.markdown(e.get("answer", ""))
                _trace_strip(e.get("trace"))
        elif e["kind"] == "result":
            st.caption(f"✅ done — {e['steps']} step(s). Try **Revoke** then **Run** again: "
                       "the next act is denied at the gateway.")
