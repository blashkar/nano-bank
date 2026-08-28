"""Read the live CX scorecard for the presentation console. Mirrors the CTO
console's ledger.py: no host DB driver — we `kubectl exec` into the cx-mcp pod
(which has the cx package + the bank DB env) and run cx.metrics there, returning
the NPS / CSAT / issues / adoption headline. The pure parser is unit-tested; the
subprocess wrapper is exercised live."""
from __future__ import annotations
import json
import subprocess

CTX = "kind-nano-bank"
NS = "nano-bank"

_SCRIPT = (
    "import json;"
    "from cx.db import CxDB;"
    "from cx.config import Settings;"
    "from cx import metrics;"
    "db=CxDB(Settings.from_env().db); w=30;"
    "print(json.dumps({"
    "'nps': metrics.nps(db.survey_scores(instrument='nps')),"
    "'csat': metrics.csat(db.survey_scores(instrument='csat')),"
    "'issues': metrics.issue_summary(db.issue_rows()),"
    "'adoption': metrics.product_adoption(db.product_activity(w), db.active_customer_count(w))"
    "}))"
)


def parse_scorecard(text: str) -> dict:
    """The last JSON line the exec printed, or {} if empty/garbled."""
    text = (text or "").strip()
    if not text:
        return {}
    try:
        return json.loads(text.splitlines()[-1])
    except (json.JSONDecodeError, IndexError):
        return {}


def read_scorecard(ctx: str = CTX, ns: str = NS) -> dict:
    p = subprocess.run(
        ["kubectl", "--context", ctx, "-n", ns, "exec", "deploy/cx-mcp", "--",
         "python", "-c", _SCRIPT],
        capture_output=True, text=True)
    return parse_scorecard(p.stdout)
