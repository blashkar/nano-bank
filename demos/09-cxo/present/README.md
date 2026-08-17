# Agent CXO — presentation console

A presenter-paced, three-pane console for talks / screencasts:

- **left rail** — a **button per beat**, each captioned with *what that beat tests*
- **centre** — the selected beat's card (question → agent answer → harness → outcome)
- **right** — a live **CX scorecard** (NPS, CSAT, open issues + top theme, adoption)

Driven live via `run-demo.sh` (`--emit-jsonl`) with a recorded fallback you can
**replay beat-by-beat** — click a beat and it appears instantly. The CXO is an
analyst, so (unlike the CTO console) there is no tamper-evident ledger and no
coder step-through; the scorecard is the CXO's grounded-numbers analog.

## Run (from the host)

    export XDG_RUNTIME_DIR=/run/user/1000 XDG_DATA_HOME=/home/bmartins/.local/share
    uv pip install --python demos/09-cxo/.venv/bin/python -r demos/09-cxo/present/requirements.txt
    demos/09-cxo/.venv/bin/streamlit run demos/09-cxo/present/app.py \
      --server.port 8513 --server.address 0.0.0.0 --server.headless true

Open http://localhost:8513 (or the LAN URL streamlit prints).

## Controls

- **▶ Run live** — deploys-if-needed, seeds cx_issues + surveys, fires an escalation,
  and drives the beats against the deployed CXO (needs docker+kind+kubectl+uv). Saves
  each run to `recordings/`.
- **⏮ Replay last good run** — plays the newest recording; the scorecard replays from
  the recording's snapshot (network-independent). A canonical recording is committed.
- **▦ All beats** — the classic stacked view of the whole run.
- **↺ Reset** — clear the loaded run back to the initial (empty) state.
- **Beat buttons** — click any beat to show just that beat; ✅ marks beats with a result.

The console never mutates the cluster — all staging lives in `run-demo.sh`; the
console runs it, reads the scorecard (via `kubectl exec` into `cx-mcp`), and renders.
