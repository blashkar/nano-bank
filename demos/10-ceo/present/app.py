"""Agent CEO presentation console — the boardroom, in two tabs:

  · 🏛️ C-suite meeting — round-the-table consults → synthesis → a directive
  · ⚖️ Board debate    — a back-and-forth on one pressing topic, then a ruling

Each tab is a per-beat stepper: three panes — the agenda (a button per beat), the
selected beat (round-the-table: each officer's contribution, then the CEO/chair),
and the board roster. Driven live by run-demo.sh (--emit-jsonl) with a recorded
fallback. Run from the HOST:

    export XDG_RUNTIME_DIR=/run/user/1000 XDG_DATA_HOME=/home/bmartins/.local/share
    streamlit run demos/10-ceo/present/app.py --server.port 8511
"""
from __future__ import annotations
import os
import subprocess
import sys
import tempfile

import streamlit as st

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import state  # noqa: E402

REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
RUN_DEMO = os.path.join(REPO_ROOT, "demos", "10-ceo", "run-demo.sh")
PRESENT = os.path.dirname(os.path.abspath(__file__))

_OFFICER_ICON = {"cfo": "💰", "coo": "🏭", "cto": "🖥️", "cxo": "🙂"}
_OFFICERS = [("CFO", "the books — profitability, NIM, capital", "consult-only"),
             ("COO", "money-movement operations — rails, settlement", "directable"),
             ("CTO", "the platform — reliability, deployments", "directable"),
             ("CXO", "customer experience — friction, NPS/CSAT", "consult-only")]

# Each tab: label, its driver (for the beat catalog + live run), recordings dir,
# the run-demo flag that selects that driver, and a session-state key prefix.
TABS = [
    {"label": "🏛️ C-suite meeting", "driver": "drive.py",
     "recordings": os.path.join(PRESENT, "recordings", "meeting"),
     "flag": "", "key": "mt"},
    {"label": "⚖️ Board debate", "driver": "debate.py",
     "recordings": os.path.join(PRESENT, "recordings", "debate"),
     "flag": "--debate", "key": "db"},
]

st.set_page_config(page_title="Agent CEO", layout="wide")
ss = st.session_state


def _contribution_panel(c: dict) -> None:
    officer = (c.get("officer") or "").lower()
    icon = _OFFICER_ICON.get(officer, "🧑‍💼")
    if c.get("role") == "direct":
        acted = c.get("acted")
        tag = ("🟢 lever fired" if acted else "🟡 no action") if acted is not None else "directed"
        header = f"{icon} **{officer.upper()}** — directed · {tag}"
    else:
        header = f"{icon} **{officer.upper()}** — speaks"
    with st.container(border=True):
        st.markdown(header)
        text = (c.get("text") or "").strip()
        st.markdown(text if text else "_(no content captured)_")


def _beat_card(rec: dict) -> None:
    st.markdown(f"#### Beat {rec['beat']} — {rec['title']}")
    st.caption(rec["shows"])
    st.markdown(f"**🗣️ Chair puts to the board:** {rec['question']}")
    contributions = rec.get("contributions") or []
    if contributions:
        st.markdown("##### 🏛️ Round the table")
        for c in contributions:
            _contribution_panel(c)
    st.markdown("##### 🎙️ The CEO (chair)")
    st.write(rec["answer"])
    chip = state.outcome_chip(rec["outcome"]["kind"])
    if chip:
        st.markdown(f"**minutes:** {chip}")


def _pending_card(cat: dict) -> None:
    st.markdown(f"#### Beat {cat['beat']} — {cat['title']}")
    st.caption(cat["shows"])
    st.markdown(f"**Q:** {cat['question']}")
    st.info("Not shown yet — click **▶ Run live** or **⏮ Replay last good run**.")


def _start_live(cfg: dict) -> None:
    os.makedirs(cfg["recordings"], exist_ok=True)
    fd, path = tempfile.mkstemp(suffix=".jsonl", prefix=f"ceo-{cfg['key']}-")
    os.close(fd)
    env = dict(os.environ,
               XDG_RUNTIME_DIR=os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"),
               XDG_DATA_HOME=os.environ.get("XDG_DATA_HOME", os.path.expanduser("~/.local/share")))
    cmd = ["bash", RUN_DEMO, "--no-up", "--no-seed"]
    if cfg["flag"]:
        cmd.append(cfg["flag"])
    cmd += ["--emit-jsonl", path]
    k = cfg["key"]
    ss[f"{k}_proc"] = subprocess.Popen(cmd, cwd=REPO_ROOT, env=env)
    ss[f"{k}_jsonl"], ss[f"{k}_beats"], ss[f"{k}_mode"], ss[f"{k}_selected"] = path, [], "live", 1


def _start_replay(cfg: dict) -> None:
    latest = state.latest_recording(cfg["recordings"])
    if not latest:
        st.toast("No recording yet — run live once to capture one.")
        return
    k = cfg["key"]
    ss[f"{k}_beats"], ss[f"{k}_mode"], ss[f"{k}_selected"] = (
        state.load_recording(latest)["beats"], "replay", 1)


def render_tab(cfg: dict) -> None:
    k = cfg["key"]
    catalog = state.beat_catalog(os.path.join(REPO_ROOT, "demos", "10-ceo", cfg["driver"]))
    ss.setdefault(f"{k}_beats", [])
    ss.setdefault(f"{k}_mode", "idle")
    ss.setdefault(f"{k}_proc", None)
    ss.setdefault(f"{k}_jsonl", None)
    ss.setdefault(f"{k}_selected", 1)
    ss.setdefault(f"{k}_primed", False)

    if not ss[f"{k}_primed"] and not ss[f"{k}_beats"] and ss[f"{k}_mode"] == "idle":
        latest = state.latest_recording(cfg["recordings"])
        if latest:
            try:
                ss[f"{k}_beats"] = state.load_recording(latest)["beats"]
            except (OSError, ValueError):
                pass
        ss[f"{k}_primed"] = True

    live = ss[f"{k}_mode"] == "live"
    c1, c2, c3, c4, _ = st.columns([1, 1.4, 1, 1, 3])
    if c1.button("▶ Run live", key=f"{k}-live", type="primary", disabled=live):
        _start_live(cfg)
    if c2.button("⏮ Replay last good run", key=f"{k}-replay", disabled=live):
        _start_replay(cfg)
    if c3.button("▦ All beats", key=f"{k}-all"):
        ss[f"{k}_selected"] = None
    if c4.button("↺ Reset", key=f"{k}-reset", disabled=live):
        ss[f"{k}_beats"], ss[f"{k}_mode"], ss[f"{k}_proc"], ss[f"{k}_jsonl"] = [], "idle", None, None
        ss[f"{k}_selected"], ss[f"{k}_primed"] = 1, True

    if live and ss[f"{k}_jsonl"]:
        try:
            with open(ss[f"{k}_jsonl"], encoding="utf-8") as f:
                ss[f"{k}_beats"] = state.read_jsonl(f.read())
        except FileNotFoundError:
            pass

    by_num = {int(r["beat"]): r for r in ss[f"{k}_beats"]}
    nav, centre, right = st.columns([2.2, 4, 2.6])

    with nav:
        st.subheader("Agenda")
        st.caption("Click a beat. ✅ = has a result this session.")
        for b in catalog:
            n = b["beat"]
            mark = "✅" if n in by_num else "⚪"
            sel = "▶ " if ss[f"{k}_selected"] == n else ""
            if st.button(f"{sel}{mark} Beat {n} — {b['title']}",
                         key=f"{k}-beat-{n}", use_container_width=True):
                ss[f"{k}_selected"] = n
            st.caption(b["shows"])

    with centre:
        sel = ss[f"{k}_selected"]
        if sel is None:
            st.subheader("Full session")
            if not ss[f"{k}_beats"]:
                st.info("No run loaded yet. Click ▶ Run live or ⏮ Replay last good run.")
            for rec in ss[f"{k}_beats"]:
                _beat_card(rec)
                st.divider()
        else:
            rec = by_num.get(sel)
            cat = next((b for b in catalog if b["beat"] == sel), None)
            if rec:
                _beat_card(rec)
            elif live:
                _pending_card(cat or {"beat": sel, "title": "", "shows": "", "question": ""})
                st.caption("⏳ live run in progress — this beat will fill in when it lands.")
            elif cat:
                _pending_card(cat)

        if live and ss[f"{k}_proc"] and ss[f"{k}_proc"].poll() is not None:
            state.save_recording(cfg["recordings"], ss[f"{k}_beats"])
            ss[f"{k}_mode"], ss[f"{k}_proc"] = "idle", None
            st.toast("Live run complete — recording saved.")

    with right:
        st.subheader("The board")
        for name, lane, kind in _OFFICERS:
            tag = "🔧 directable" if kind == "directable" else "📊 consult-only"
            st.markdown(f"**{name}** · {tag}")
            st.caption(lane)
        st.divider()
        st.caption("Officers relay positions to each other through the chair; a "
                   "directive posts an imperative to the officer's own /ask — its "
                   "lever self-verifies and acts, and the CEO reads back the ledger "
                   "row to prove a lever fired.")


st.title("Agent CEO — the boardroom")
tabs = st.tabs([t["label"] for t in TABS])
for tab, cfg in zip(tabs, TABS):
    with tab:
        render_tab(cfg)

# refresh while any tab is running a live capture
if any(ss.get(f"{t['key']}_mode") == "live" for t in TABS):
    import time
    time.sleep(1.5)
    st.rerun()
