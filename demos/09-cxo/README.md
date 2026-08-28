# Demo 9 — Agent CXO (customer-experience analyst + a metrics-&-surveys service)

The **Chief Experience Officer** agent over the `cx` metrics service: a grounded
CX analyst that reads onboarding, adoption, friction, and engagement from the bank
data, surfaces the **customer voice** (complaints the personal managers file into
`cx_issues`, plus urgent A2A escalations), and produces a **ranked, grounded
feature backlog**. Same `csuite` harness as the COO/CFO/CTO demos.

## What it shows

A 6-beat narrated arc (`drive.py`): a grounded CX posture where every figure is
tool-grounded; a derived rate via the `compute` tool; the top complaint themes
from `issue_summary` + `notable_issues`; an **urgent escalation** surfaced and
**re-grounded** from `cx_issues` (never the raw alert); the signature **ranked
feature backlog**, each item citing the CX signal that motivates it; and scope
discipline (a P&L question deferred to the CFO) plus a durable CX note recorded and
recalled.

## Honesty note

The CXO is **analyst-only** — it has no acting levers and writes no bank state. So,
unlike the CTO demo, there are **no tamper-evident ledger acting rows** here; the
point is grounding, the customer voice, and the backlog. The complaint data is
**seeded** as-if-personal-manager filings (`cx/seed_cx_issues.py`), and the
escalation beat is exercised by one scripted `/escalations` ping — the real
`file_cx_issue` + escalate path is built and unit-tested (`agent/`), just not
driven by a live personal manager in this demo (Phase 1 scope).

## Run it

```bash
# one command: deploy cx-mcp + cxo -> seed cx_issues -> escalate -> drive
demos/09-cxo/run-demo.sh

# against an already-deployed stack:
demos/09-cxo/run-demo.sh --no-up

# drive against cx_issues as-is (no reseed / re-escalate):
demos/09-cxo/run-demo.sh --no-seed
```

Prereqs: docker + kind + kubectl + uv; the bank stack up (postgres + bank-api) with
`nano-agent-secrets` minted (coo/cto deploy); and — since behavioural metrics need
activity — run `testing/generator` + a rail simulator first if the estate is empty.
