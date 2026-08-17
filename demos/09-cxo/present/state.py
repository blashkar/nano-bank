"""Pure state helpers for the CXO presentation console: parse the JSONL beat
stream, save/load recordings, read the static beat catalog from the driver, and
map an outcome kind to a chip style. No Streamlit here so it stays unit-testable."""
from __future__ import annotations
import ast
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


def save_recording(dir_: str, beats: list[dict], scorecard: dict | None = None) -> str:
    os.makedirs(dir_, exist_ok=True)
    ts = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S_%fZ")
    path = os.path.join(dir_, f"{ts}.json")
    with open(path, "w", encoding="utf-8") as f:
        json.dump({"beats": beats, "scorecard": scorecard or {}, "captured_at": ts},
                  f, indent=2)
    return path


def load_recording(path: str) -> dict:
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def latest_recording(dir_: str) -> str | None:
    files = sorted(glob.glob(os.path.join(dir_, "*.json")), key=os.path.getmtime)
    return files[-1] if files else None


def beat_catalog(drive_path: str) -> list[dict]:
    """The demo's beats as a static catalog — {beat, title, shows, question} per
    entry — parsed straight from drive.py's BEATS list via ast (no import, no
    network), so the per-beat buttons + their 'what is being tested' captions
    render before any run and stay in sync with the driver. Empty on any problem."""
    try:
        with open(drive_path, encoding="utf-8") as f:
            tree = ast.parse(f.read())
    except (OSError, SyntaxError):
        return []
    node = next((n.value for n in tree.body if isinstance(n, ast.Assign)
                 and any(isinstance(t, ast.Name) and t.id == "BEATS" for t in n.targets)),
                None)
    if node is None:
        return []
    try:
        raw = ast.literal_eval(node)
    except (ValueError, SyntaxError):
        return []
    return [{"beat": i, "title": b.get("title", ""), "shows": b.get("shows", ""),
             "question": b.get("message", "")} for i, b in enumerate(raw, 1)]


_STYLES = {
    "read_only": ("READ-ONLY", "#57606a"),
    "deferred":  ("DEFERRED", "#6639ba"),
    "executed":  ("EXECUTED", "#1a7f37"),
    "refused":   ("REFUSED", "#b35900"),
}


def outcome_style(kind: str) -> tuple[str, str]:
    return _STYLES.get(kind, (kind.upper(), "#57606a"))
