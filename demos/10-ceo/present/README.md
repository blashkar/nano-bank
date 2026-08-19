# Agent CEO — presentation console

A C-suite-meeting view: the CEO chairs the board. Three panes — the **agenda**
(a button per beat, each captioned), the **selected beat** (the CEO's turn at the
table: question → answer → verified-minutes chip), and **the board** (the four
officers, which are directable vs consult-only).

## Run (from the host)

    export XDG_RUNTIME_DIR=/run/user/1000 XDG_DATA_HOME=/home/bmartins/.local/share
    uv pip install --python demos/10-ceo/.venv/bin/python -r demos/10-ceo/present/requirements.txt
    demos/10-ceo/.venv/bin/streamlit run demos/10-ceo/present/app.py \
      --server.port 8511 --server.address 0.0.0.0 --server.headless true

## Controls

- **▶ Run live** — drives the meeting against the deployed CEO (saves a recording).
- **⏮ Replay last good run** — plays the newest recording beat-by-beat.
- **▦ All beats** — the whole meeting stacked. **↺ Reset** — clear the loaded run.

The **minutes** chip on the directive beats reports whether a lever actually fired
(🟢) or the officer took no action (🟡) — read back from the tamper-evident ledger.
