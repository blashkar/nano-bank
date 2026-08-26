#!/usr/bin/env python3
"""Narrated CEO debate — a BOARD DEBATE on one pressing topic. Unlike the round-
the-table meeting (drive.py), here the officers argue WITH each other: the CEO
chairs a cross-functional debate on 'should we ship a capped PILOT of recurring
e-Transfers this quarter?' — relaying each officer's position to the next so
they respond to one another (CXO demand -> CTO capacity -> CFO cost -> COO
load), then rules. The back-and-forth is the point; every figure stays
attributed to its officer.

Scoped as a bounded pilot (enrollment cap + per-customer monthly cap,
Interac-only) rather than an open-ended rollout: the real, ungamed operational
data supports a small pilot's incremental load even where it would not support
an unbounded launch, and AFT/Lynx settlement risk simply isn't implicated by an
Interac-only feature — so the COO is asked to reason about the pilot's actual
incremental footprint, not the whole payments estate.

    CEO_API_URL=http://localhost:8099 python demos/10-ceo/debate.py
    python demos/10-ceo/debate.py --beats 1,5      # motion + ruling only
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))  # demos/
from _driver import run  # noqa: E402

_TOPIC = ("Pressing decision: should we ship a LIMITED PILOT of recurring "
          "e-Transfers this quarter -- capped at 500 enrolled customers, each "
          "capped at $50/month, Interac-only (it does not touch AFT or Lynx)? "
          "It is the top customer feature request, and a capped pilot bounds "
          "the platform, cost and operational exposure while we validate it "
          "before any wider rollout. ")

BEATS = [
    {
        "title": "The CEO tables the motion",
        "shows": "the CEO opens the debate and frames the question, before anyone else speaks",
        "message": _TOPIC + "Table this motion to open the debate. Do NOT consult any "
                   "officer yet — that starts next turn. Right now just state the motion "
                   "in your own words and name the order you will hear positions in: "
                   "CXO (demand) -> CTO (capacity) -> CFO (cost) -> COO (operational "
                   "load) -> your ruling. This is a framing statement, not a report: you "
                   "have gathered no figures yet because none are needed yet, so do NOT "
                   "mention tools, grounding, or what you have or haven't gathered — that "
                   "reads as an apology, not an opening. Open the debate the way a chair "
                   "actually would: confident, plain, in command of the room.",
        "thread": "debate",
    },
    {
        "title": "The CXO makes the case",
        "shows": "the CEO calls on the proponent to open",
        "message": "Open the debate: consult ONLY the CXO right now (do not "
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
                   "what the CXO just argued, then ask: can the platform take a capped "
                   "pilot (500 customers, $50/month cap, Interac-only) this quarter, and "
                   "what is the reliability/capacity risk right now? Relay the CTO's "
                   "answer and say where it agrees or conflicts with the CXO's position.",
        "thread": "debate",
    },
    {
        "title": "The CFO weighs the cost",
        "shows": "the CEO carries the CTO's concern to the CFO for the economics",
        "message": "Now consult ONLY the CFO (no other officer this turn). Summarize the "
                   "CTO's capacity concern for the CFO, then ask: what does the capped "
                   "pilot (500 customers, $50/month cap) cost, and does the return (NIM / "
                   "RAROC / fee income) justify piloting it this quarter versus deferring? "
                   "Relay the CFO's answer, attributing every figure.",
        "thread": "debate",
    },
    {
        "title": "The COO on operational load",
        "shows": "the CEO asks the operator whether Interac can absorb the pilot",
        "message": "Now consult ONLY the COO (no other officer this turn). Given the "
                   "CXO's demand, the CTO's capacity risk and the CFO's economics, ask "
                   "the COO: can Interac specifically absorb THIS PILOT's bounded "
                   "incremental volume — 500 customers at up to $50/month each, roughly "
                   "$25,000/month (~$833/day) of scheduled sends layered on top of "
                   "current Interac activity? This pilot does not touch AFT or Lynx, so "
                   "do not evaluate those rails — stay on Interac capacity/float for this "
                   "bounded volume specifically. What would break first, if anything, at "
                   "this scale? Relay the COO's answer.",
        "thread": "debate",
    },
    {
        "title": "The chair rules",
        "shows": "the CEO weighs the four positions and makes a grounded call",
        "message": "Close the debate and RULE — do NOT consult anyone again, reason "
                   "only from the four positions already on the record this session. "
                   "Weigh them against each other: who is right on what, and where the "
                   "real trade-off sits. Give your decision on the CAPPED PILOT as "
                   "scoped (500 customers, $50/month cap, Interac-only) — ship the pilot, "
                   "defer, or phase it — with the grounded reasons, and name which "
                   "officer you would direct next and why (do not act; this is the "
                   "ruling).",
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
