"""Pure state helpers for the demo-4 gateway console: parse the JSONL event
stream, save/load recordings, and style a gateway decision. No Streamlit
here so it stays unit-testable."""
from __future__ import annotations
import glob
import json
import os
from datetime import datetime, timezone


def read_jsonl(text: str) -> list[dict]:
    out = []
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            out.append(json.loads(line))
        except json.JSONDecodeError:
            continue  # partial trailing line mid-write
    return out


def save_recording(dir_: str, events: list[dict]) -> str:
    os.makedirs(dir_, exist_ok=True)
    ts = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S_%fZ")
    path = os.path.join(dir_, f"{ts}.json")
    with open(path, "w", encoding="utf-8") as f:
        json.dump({"events": events, "captured_at": ts}, f, indent=2)
    return path


def load_recording(path: str) -> dict:
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def latest_recording(dir_: str) -> str | None:
    files = sorted(glob.glob(os.path.join(dir_, "*.json")), key=os.path.getmtime)
    return files[-1] if files else None


_DECISION_STYLE = {
    "allow": ("ALLOW", "#1a7f37"),
    "deny": ("DENY", "#cf222e"),
    "pending_approval": ("PENDING APPROVAL", "#9a6700"),
}


def decision_style(decision: str) -> tuple[str, str]:
    return _DECISION_STYLE.get(decision, (decision.upper(), "#57606a"))
