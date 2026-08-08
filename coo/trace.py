from __future__ import annotations
import time
from typing import Any

from langchain_core.callbacks import BaseCallbackHandler


def _short(x: Any, n: int = 2000) -> str:
    s = x if isinstance(x, str) else str(x)
    return s if len(s) <= n else s[:n] + "…"


class TraceRecorder(BaseCallbackHandler):
    """Records tool/model steps of a LangGraph run as ordered, JSON-safe events.
    Each closed event carries a wall-clock `t` so it can be merged with the
    harness event log (compaction / subagent) into one ordered trace."""

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
        # Full output, not _short: the verifier parses these numbers to build
        # its grounded set, and a truncated bundle would drop figures and
        # produce false "ungrounded" flags. Tool outputs are bounded (a few KB).
        text = output if isinstance(output, str) else str(output)
        self._close(kwargs.get("run_id"), ok=True, output=text)

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
            "seq": len(self._events), "t": time.time(),
            "kind": info["kind"], "name": info["name"],
            "ok": ok, "elapsed_ms": int((time.perf_counter() - info["t0"]) * 1000),
            "input": info.get("input"), "output": output, "error": error,
        })

    def events(self) -> list[dict]:
        return list(self._events)


def merge(tool_events: list[dict], harness_events: list[dict]) -> list[dict]:
    """One ordered, auditable trace covering tool/model steps AND the non-tool
    harness events (compaction, subagent spawn/return, memory writes). Both carry
    a wall-clock `t`; sort by it, falling back to arrival order so events without
    a timestamp keep their relative position."""
    tagged = ([{**e, "source": "tool"} for e in tool_events]
              + [{**e, "source": "harness"} for e in harness_events])
    return sorted(tagged, key=lambda e: (e.get("t", 0.0), e.get("seq", 0)))
