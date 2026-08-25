#!/usr/bin/env python3
"""Narrated CEO demo — BUILD IT. After the board phases the recurring
e-Transfers pilot (see debate.py), the CEO directs the CTO to start the first
piece of implementation: delegate a scoped coding task to the coder, nano-bank's
own in-cluster PR-gated coding agent. The CTO's own agent decides how to phrase
the task, delegates it, and reports back exactly what the developer did —
verified against the tamper-evident audit ledger, not just what was asked for.

    CEO_API_URL=http://localhost:8099 python demos/10-ceo/build.py
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))  # demos/
from _driver import run  # noqa: E402

BEATS = [
    {
        "title": "The board's call, relayed to the CTO",
        "shows": "the CEO turns the board's phased-pilot ruling into an implementation directive",
        "message": "The board just ruled on recurring e-Transfers: PHASE a capped pilot "
                   "(500 customers, $50/month per customer, Interac-only) rather than an "
                   "open-ended launch — the binding condition was a notification-pipeline "
                   "capacity finding, not the rail's own capacity. DIRECT the CTO: start "
                   "the pilot's first piece of implementation — delegate the pilot's "
                   "monthly-cap guardrail (enforcing the $50/customer/month cap) to the "
                   "coder as a gated pull request. Do not merge it yourself; a human "
                   "reviews and merges. Give the CTO your rationale, then tell me exactly "
                   "what the CTO did and what the developer produced.",
        "thread": "build",
        "outcome_hint": "delegated",
    },
    {
        "title": "Verified minutes — did the developer actually deliver?",
        "shows": "read-back honesty: the CEO confirms the delegation against the audit ledger",
        "message": "Close this out. Did the CTO's delegation actually reach the coder and "
                   "produce a real, reviewable outcome? Record the minutes: what was "
                   "delegated, what the developer (coder) did — code changed, tests, PR — "
                   "and the ledger evidence that this happened, not just that it was asked "
                   "for.",
        "thread": "build",
        "outcome_hint": "delegated",
    },
]

if __name__ == "__main__":
    raise SystemExit(run(
        BEATS,
        api_url=os.environ.get("CEO_API_URL", "http://localhost:8099"),
        agent_label="Agent CEO",
        run_hint="demos/10-ceo/build.py",
    ))
