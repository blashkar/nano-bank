"""Agent CEO presentation console — a C-suite-meeting view. Three panes:

  · left rail  — a BUTTON PER BEAT, each captioned with what that beat shows
  · centre     — the selected beat's card (the CEO's turn at the table)
  · right      — the C-suite roster + verified-minutes chip (did a lever fire?)

Driven live by run-demo.sh (--emit-jsonl) with a recorded fallback you can replay
beat-by-beat. Run from the HOST:

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
DRIVE_PY = os.path.join(REPO_ROOT, "demos", "10-ceo", "drive.py")
RECORDINGS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "recordings")

_OFFICERS = [("CFO", "the books — profitability, NIM, capital", "consult-only"),
             ("COO", "money-movement operations — rails, settlement", "directable"),
             ("CTO", "the platform — reliability, deployments", "directable"),
             ("CXO", "customer experience — friction, NPS/CSAT", "consult-only")]

st.set_page_config(page_title="Agent CEO", layout="wide")
CATALOG = state.beat_catalog(DRIVE_PY)

ss = st.session_state
ss.setdefault("beats", [])
ss.setdefault("mode", "idle")
ss.setdefault("proc", None)
ss.setdefault("jsonl_path", None)
ss.setdefault("selected", 1)
ss.setdefault("primed", False)


def _beat_card(rec: dict) -> None:
    st.markdown(f"#### Beat {rec['beat']} — {rec['title']}")
    st.caption(rec["shows"])
    st.markdown(f"**Q:** {rec['question']}")
    h = rec.get("harness", {})
    bits = []
    for k in ("planned", "todos", "subagents"):
        if h.get(k):
            bits.append(f"{k} {h[k]}")
    if h.get("tools"):
        bits.append("tools: " + ", ".join(h["tools"]))
    if bits:
        with st.expander("harness · " + " · ".join(bits)):
            st.json(h)
    st.write(rec["answer"])
    chip = state.outcome_chip(rec["outcome"]["kind"])
    if chip:
        st.markdown(f"**minutes:** {chip}")


def _pending_card(cat: dict) -> None:
    st.markdown(f"#### Beat {cat['beat']} — {cat['title']}")
    st.caption(cat["shows"])
    st.markdown(f"**Q:** {cat['question']}")
    st.info("Not shown yet — click **▶ Run live** or **⏮ Replay last good run**.")


def _reset() -> None:
    ss.beats, ss.mode, ss.proc, ss.jsonl_path = [], "idle", None, None
    ss.selected, ss.primed = 1, True


def _start_live() -> None:
    os.makedirs(RECORDINGS, exist_ok=True)
    fd, path = tempfile.mkstemp(suffix=".jsonl", prefix="ceo-run-")
    os.close(fd)
    env = dict(os.environ,
               XDG_RUNTIME_DIR=os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"),
               XDG_DATA_HOME=os.environ.get("XDG_DATA_HOME", os.path.expanduser("~/.local/share")))
    ss.proc = subprocess.Popen(["bash", RUN_DEMO, "--no-up", "--emit-jsonl", path],
                               cwd=REPO_ROOT, env=env)
    ss.jsonl_path, ss.beats, ss.mode, ss.selected = path, [], "live", 1


def _start_replay() -> None:
    latest = state.latest_recording(RECORDINGS)
    if not latest:
        st.toast("No recording yet — run live once to capture one.")
        return
    ss.beats, ss.mode, ss.selected = state.load_recording(latest)["beats"], "replay", 1


if not ss.primed and not ss.beats and ss.mode == "idle":
    _latest = state.latest_recording(RECORDINGS)
    if _latest:
        try:
            ss.beats = state.load_recording(_latest)["beats"]
        except (OSError, ValueError):
            pass
    ss.primed = True

st.title("Agent CEO — the C-suite meeting")
c1, c2, c3, c4, _ = st.columns([1, 1.4, 1, 1, 3])
if c1.button("▶ Run live", type="primary", disabled=ss.mode == "live"):
    _start_live()
if c2.button("⏮ Replay last good run", disabled=ss.mode == "live"):
    _start_replay()
if c3.button("▦ All beats"):
    ss.selected = None
if c4.button("↺ Reset", disabled=ss.mode == "live"):
    _reset()

by_num = {int(r["beat"]): r for r in ss.beats}
nav, centre, right = st.columns([2.2, 4, 2.6])

with nav:
    st.subheader("Agenda")
    st.caption("Click a beat. ✅ = has a result this session.")
    for b in CATALOG:
        n = b["beat"]
        mark = "✅" if n in by_num else "⚪"
        sel = "▶ " if ss.selected == n else ""
        if st.button(f"{sel}{mark} Beat {n} — {b['title']}",
                     key=f"beat-btn-{n}", use_container_width=True):
            ss.selected = n
        st.caption(b["shows"])

with centre:
    if ss.mode == "live" and ss.jsonl_path:
        try:
            with open(ss.jsonl_path, encoding="utf-8") as f:
                ss.beats = state.read_jsonl(f.read())
            by_num = {int(r["beat"]): r for r in ss.beats}
        except FileNotFoundError:
            pass

    if ss.selected is None:
        st.subheader("Full meeting")
        if not ss.beats:
            st.info("No run loaded yet. Click ▶ Run live or ⏮ Replay last good run.")
        for rec in ss.beats:
            _beat_card(rec)
            st.divider()
    else:
        rec = by_num.get(ss.selected)
        cat = next((b for b in CATALOG if b["beat"] == ss.selected), None)
        if rec:
            _beat_card(rec)
        elif ss.mode == "live":
            _pending_card(cat or {"beat": ss.selected, "title": "", "shows": "", "question": ""})
            st.caption("⏳ live run in progress — this beat will fill in when it lands.")
        elif cat:
            _pending_card(cat)

    if ss.mode == "live" and ss.proc and ss.proc.poll() is not None:
        state.save_recording(RECORDINGS, ss.beats)
        ss.mode, ss.proc = "idle", None
        st.toast("Live run complete — recording saved.")

with right:
    st.subheader("The board")
    for name, lane, kind in _OFFICERS:
        tag = "🔧 directable" if kind == "directable" else "📊 consult-only"
        st.markdown(f"**{name}** · {tag}")
        st.caption(lane)
    st.divider()
    st.caption("A directive posts an imperative to the officer's own /ask; its lever "
               "self-verifies and acts. The CEO reads back the officer's ledger row "
               "to prove a lever fired before recording its own directive row.")

if ss.mode == "live":
    import time
    time.sleep(1.5)
    st.rerun()
