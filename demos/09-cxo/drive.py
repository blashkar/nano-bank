#!/usr/bin/env python3
"""Narrated CXO demo — the beats; rendering/streaming lives in demos/_driver.py.

    CXO_API_URL=http://localhost:8098 python demos/09-cxo/drive.py
    python demos/09-cxo/drive.py --beats 1,5      # posture + backlog only

The CXO is an ANALYST: it reads the cx metrics service, surfaces the customer
voice (issues + personal-manager escalations), and produces a ranked feature
backlog. It has no acting levers, so — unlike the CTO — there are no ledger acting
rows; grounding and the backlog are the point. Setup (demos/09-cxo/run-demo.sh)
seeds cx_issues and fires one scripted urgent escalation before driving.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))  # demos/
from _driver import run  # noqa: E402

BEATS = [
    {
        "title": "Grounded CX posture",
        "shows": "every CX figure is tool-grounded: onboarding, adoption, friction, engagement",
        "message": "Give me a grounded customer-experience posture right now: "
                   "onboarding/activation, product adoption, friction (failed "
                   "transactions + Interac expiries), and engagement. Use the "
                   "numbers; this is an assessment.",
        "thread": "new",
    },
    {
        "title": "Derived figure (compute)",
        "shows": "a rate the raw tools don't return — the CXO calls compute",
        "message": "What share of Interac e-Transfers expired unclaimed in the "
                   "window? Give me the percentage — just the number.",
        "thread": "new",
    },
    {
        "title": "The customer voice",
        "shows": "issue_summary + notable_issues: the top complaint themes, grounded",
        "message": "What are customers complaining about? Give me the open issues by "
                   "category and severity, the top theme, and the most severe "
                   "individual ones.",
        "thread": "new",
    },
    {
        "title": "Urgent escalation",
        "shows": "a personal-manager escalation is surfaced, re-grounded from cx_issues",
        "message": "Any urgent escalations from the personal managers right now? "
                   "Surface them with the grounded issue details.",
        "thread": "new",
        "outcome_hint": "read_only",
    },
    {
        "title": "Ranked feature backlog",
        "shows": "the signature output: a prioritised backlog, each item citing its grounded signal",
        "message": "Give me a ranked feature backlog for next quarter — top 3 — each "
                   "item justified by the CX signal that motivates it and its magnitude.",
        "thread": "new",
    },
    {
        "title": "Scope discipline + memory",
        "shows": "defers a P&L question to the CFO; records + recalls a durable CX note",
        "message": "What was our net interest margin last month, and note the top CX "
                   "risk you'd watch as a durable note.",
        "thread": "new",
        "outcome_hint": "deferred",
    },
]

if __name__ == "__main__":
    raise SystemExit(run(
        BEATS,
        api_url=os.environ.get("CXO_API_URL", "http://localhost:8098"),
        agent_label="Agent CXO",
        run_hint="demos/09-cxo/run-demo.sh",
    ))
