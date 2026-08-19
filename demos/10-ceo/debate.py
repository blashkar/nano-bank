#!/usr/bin/env python3
"""Narrated CEO debate — a BOARD DEBATE on one pressing topic. Unlike the round-
the-table meeting (drive.py), here the officers argue WITH each other: the CEO
chairs a cross-functional debate on 'should we ship recurring e-Transfers this
quarter?' — relaying each officer's position to the next so they respond to one
another (CXO demand → CTO capacity → CFO cost → COO load), then rules. The
back-and-forth is the point; every figure stays attributed to its officer.

    CEO_API_URL=http://localhost:8099 python demos/10-ceo/debate.py
    python demos/10-ceo/debate.py --beats 1,5      # motion + ruling only
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))  # demos/
from _driver import run  # noqa: E402

_TOPIC = ("Pressing decision: should we ship RECURRING e-Transfers this quarter? "
          "It is the top customer feature request, but it adds platform, cost and "
          "operational load. ")

BEATS = [
    {
        "title": "Table the motion — the CXO makes the case",
        "shows": "the CEO tables a pressing decision and asks the proponent to open",
        "message": _TOPIC + "Open the debate: consult ONLY the CXO right now (do not "
                   "consult anyone else this turn) for the customer case — how strong "
                   "is demand for recurring e-Transfers, and what CX signal backs it "
                   "(issues, NPS/CSAT, the feature-request theme)? Relay the CXO's "
                   "answer, attributing the figures.",
        "thread": "debate",
    },
    {
        "title": "The CTO responds — can the platform take it?",
        "shows": "the CEO relays the CXO's case to the CTO, who answers back",
        "message": "Now consult ONLY the CTO (no other officer this turn). Tell the CTO "
                   "what the CXO just argued, then ask: can the platform take recurring "
                   "e-Transfers this quarter, and what is the reliability/capacity risk "
                   "right now? Relay the CTO's answer and say where it agrees or "
                   "conflicts with the CXO's position.",
        "thread": "debate",
    },
    {
        "title": "The CFO weighs the cost",
        "shows": "the CEO carries the CTO's concern to the CFO for the economics",
        "message": "Now consult ONLY the CFO (no other officer this turn). Summarize the "
                   "CTO's capacity concern for the CFO, then ask: what does this cost, "
                   "and does the return (NIM / RAROC / fee income) justify shipping it "
                   "this quarter versus deferring? Relay the CFO's answer, attributing "
                   "every figure.",
        "thread": "debate",
    },
    {
        "title": "The COO on operational load",
        "shows": "the CEO asks the operator whether the rails can absorb it",
        "message": "Now consult ONLY the COO (no other officer this turn). Given the "
                   "CXO's demand, the CTO's capacity risk and the CFO's economics, ask "
                   "the COO: can the payment rails and settlement absorb recurring "
                   "e-Transfers operationally, and what would break first? Relay the "
                   "COO's answer.",
        "thread": "debate",
    },
    {
        "title": "The chair rules",
        "shows": "the CEO weighs the four positions and makes a grounded call",
        "message": "Close the debate and RULE — do NOT consult anyone again, reason "
                   "only from the four positions already on the record this session. "
                   "Weigh them against each other: who is right on what, and where the "
                   "real trade-off sits. Give your decision — ship, defer, or phase it "
                   "— with the grounded reasons, and name which officer you would "
                   "direct next and why (do not act; this is the ruling).",
        "thread": "debate",
        "outcome_hint": "read_only",
    },
]

if __name__ == "__main__":
    raise SystemExit(run(
        BEATS,
        api_url=os.environ.get("CEO_API_URL", "http://localhost:8099"),
        agent_label="Agent CEO",
        run_hint="demos/10-ceo/run-demo.sh --debate",
    ))
