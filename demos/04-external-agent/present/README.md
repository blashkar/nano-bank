# External mandated agent — presentation extras

`app.py` (one level up) is the live demo — unchanged in behavior, now
restyled into a two-tone nav+centre stepper and saving each run here as a
recording. This directory adds a standalone **animated replay**.

## 🎬 Animated gateway cinematic (standalone)

A split-screen view of a recorded run: the external agent on the left (an
"outsider" palette — it never touches the bank directly), the personal
manager on the right (nano-bank's own brand palette), and a gateway rail
between them that lights 🟢/🔴/🟡 for each mandate-gated act and animates
the A2A hand-off for each message. Zero model delay — it replays a captured
recording.

    demos/04-external-agent/present/gateway_server.py    # -> http://localhost:8521/gateway.html

Or build once and open the file directly (uses whatever
`recordings/canonical.json` is already checked in / captured):

    python3 demos/04-external-agent/present/build_gateway.py

Controls: ▶ Run / Pause (Space), ⏮ ⏭ step (arrows), a speed slider, a
progress scrubber. The **⦿ Capture live** button (only works when served via
`gateway_server.py`, needs `DEMO_BRANCH_BASE` + `AGENT_GATEWAY_TOKEN` +
`OLLAMA_API_KEY` in the environment, and a port-forward to `svc/agent-api`)
re-runs `capture.py` against the deployed stack, saves the result as the
canonical recording, and rebuilds `gateway.html`.
