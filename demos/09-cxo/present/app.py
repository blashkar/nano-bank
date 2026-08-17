"""Agent CXO presentation console — a presenter-paced, three-pane screen:

  · left rail  — a BUTTON PER BEAT, each captioned with what that beat tests
  · centre     — the selected beat's card (question → agent answer → outcome)
  · right      — a live CX scorecard (NPS / CSAT / issues / adoption)

Driven live by run-demo.sh (--emit-jsonl) with a recorded fallback you can replay
beat-by-beat. Run from the HOST:

    export XDG_RUNTIME_DIR=/run/user/1000 XDG_DATA_HOME=/home/bmartins/.local/share
    streamlit run demos/09-cxo/present/app.py --server.port 8513

Live runs need docker+kind+kubectl+uv and the deployed CX stack (see
demos/09-cxo/run-demo.sh)."""
from __future__ import annotations
import os
import subprocess
import sys
import tempfile

import streamlit as st

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import scorecard  # noqa: E402
import state  # noqa: E402

REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
RUN_DEMO = os.path.join(REPO_ROOT, "demos", "09-cxo", "run-demo.sh")
DRIVE_PY = os.path.join(REPO_ROOT, "demos", "09-cxo", "drive.py")
RECORDINGS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "recordings")

st.set_page_config(page_title="Agent CXO", layout="wide")

CATALOG = state.beat_catalog(DRIVE_PY)          # all beats: title + what-it-tests + question

ss = st.session_state
ss.setdefault("beats", [])          # rendered beat records (live or replay)
ss.setdefault("mode", "idle")       # idle | live | replay
ss.setdefault("proc", None)         # live run subprocess
ss.setdefault("jsonl_path", None)   # live run JSONL file
ss.setdefault("snapshot", None)     # scorecard snapshot when replaying
ss.setdefault("selected", 1)        # centre pane beat (None = all)
ss.setdefault("primed", False)      # auto-load the newest recording only once


def _beat_card(rec: dict) -> None:
    label, color = state.outcome_style(rec["outcome"]["kind"])
    st.markdown(f"#### Beat {rec['beat']} — {rec['title']}")
    st.caption(rec["shows"])
    st.markdown(f"**Q:** {rec['question']}")
    h = rec.get("harness", {})
    bits = []
    if h.get("planned"):
        bits.append(f"planned {h['planned']}")
    if h.get("todos"):
        bits.append(f"todos {h['todos']}")
    if h.get("subagents"):
        bits.append(f"subagent×{h['subagents']}")
    if h.get("tools"):
        bits.append("tools: " + ", ".join(h["tools"]))
    if bits:
        with st.expander("harness · " + " · ".join(bits)):
            st.json(h)
    st.write(rec["answer"])
    detail = f" → {rec['outcome']['detail']}" if rec["outcome"]["detail"] else ""
    st.markdown(
        f"<span style='background:{color};color:white;padding:2px 10px;"
        f"border-radius:10px;font-weight:700'>{label}{detail}</span>",
        unsafe_allow_html=True)


def _pending_card(cat: dict) -> None:
    st.markdown(f"#### Beat {cat['beat']} — {cat['title']}")
    st.caption(cat["shows"])
    st.markdown(f"**Q:** {cat['question']}")
    st.info("Not shown yet — click **▶ Run live** or **⏮ Replay last good run**, "
            "then pick this beat.")


def _reset() -> None:
    ss.beats, ss.mode, ss.proc, ss.jsonl_path, ss.snapshot = [], "idle", None, None, None
    ss.selected, ss.primed = 1, True


def _start_live() -> None:
    os.makedirs(RECORDINGS, exist_ok=True)
    fd, path = tempfile.mkstemp(suffix=".jsonl", prefix="cxo-run-")
    os.close(fd)
    env = dict(os.environ,
               XDG_RUNTIME_DIR=os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"),
               XDG_DATA_HOME=os.environ.get("XDG_DATA_HOME",
                                            os.path.expanduser("~/.local/share")))
    ss.proc = subprocess.Popen(["bash", RUN_DEMO, "--no-up", "--emit-jsonl", path],
                               cwd=REPO_ROOT, env=env)
    ss.jsonl_path, ss.beats, ss.mode, ss.snapshot, ss.selected = path, [], "live", None, 1


def _start_replay() -> None:
    latest = state.latest_recording(RECORDINGS)
    if not latest:
        st.toast("No recording yet — run live once to capture one.")
        return
    rec = state.load_recording(latest)
    ss.beats, ss.mode, ss.snapshot, ss.selected = (
        rec["beats"], "replay", rec.get("scorecard") or {}, 1)


# On first load prime the stepper from the newest recording so the beat buttons
# show results immediately; runs once per session so Reset can leave it empty.
if not ss.primed and not ss.beats and ss.mode == "idle":
    _latest = state.latest_recording(RECORDINGS)
    if _latest:
        try:
            ss.beats = state.load_recording(_latest)["beats"]
        except (OSError, ValueError):
            pass
    ss.primed = True

# --- control bar -----------------------------------------------------------
st.title("Agent CXO — customer-experience analyst")
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

# --- left rail: a button + "what it tests" per beat ------------------------
with nav:
    st.subheader("Beats")
    st.caption("Click a beat to show it. ✅ = has a result this session.")
    for b in CATALOG:
        n = b["beat"]
        mark = "✅" if n in by_num else "⚪"
        sel = "▶ " if ss.selected == n else ""
        if st.button(f"{sel}{mark} Beat {n} — {b['title']}",
                     key=f"beat-btn-{n}", use_container_width=True):
            ss.selected = n
        st.caption(b["shows"])

# --- centre: the selected beat (or the whole run) --------------------------
with centre:
    if ss.mode == "live" and ss.jsonl_path:
        try:
            with open(ss.jsonl_path, encoding="utf-8") as f:
                ss.beats = state.read_jsonl(f.read())
            by_num = {int(r["beat"]): r for r in ss.beats}
        except FileNotFoundError:
            pass

    if ss.selected is None:                       # "All beats" — stacked view
        st.subheader("Full run")
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
        # live run finished: snapshot the scorecard + save the recording
        card = scorecard.read_scorecard()
        state.save_recording(RECORDINGS, ss.beats, card)
        ss.mode, ss.proc = "idle", None
        st.toast("Live run complete — recording saved.")

# --- right: live CX scorecard ----------------------------------------------
with right:
    st.subheader("CX scorecard")
    try:
        card = ss.snapshot if ss.mode == "replay" else scorecard.read_scorecard()
    except Exception:  # noqa: BLE001
        card = {}
    if not card:
        st.caption("scorecard unavailable — is cx-mcp up?")
    else:
        nps = card.get("nps", {})
        csat = card.get("csat", {})
        iss = card.get("issues", {})
        adopt = card.get("adoption", {})
        st.metric("NPS", nps.get("score", "—"), f"{nps.get('responses', 0)} responses")
        st.metric("CSAT", f"{csat.get('csat_rate', 0)}%",
                  f"mean {csat.get('mean', 0)} · {csat.get('responses', 0)} resp")
        st.metric("Open issues", iss.get("open", 0), f"top: {iss.get('top_theme', '—')}")
        st.markdown("**Adoption** (active customers)")
        for p in (adopt.get("products") or [])[:4]:
            st.write(f"- {p['product']}: {p['adoption_rate']}%  ({p['customers']})")

# live view refreshes itself while a run is in flight
if ss.mode == "live":
    import time
    time.sleep(1.5)
    st.rerun()
