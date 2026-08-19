# Demo 10 — Agent CEO (a C-suite meeting)

The **Chief Executive Officer** agent chairs the board. It holds no domain data of
its own: it **consults** the four officers (CFO / COO / CTO / CXO) over their `/ask`
A2A endpoints, **synthesizes** a grounded cross-functional executive brief (every
figure attributed to the officer who reported it), and makes one **decision** — a
**directive** to an acting officer (the COO), whose own agent self-verifies and acts
through its audited lever. Same `csuite` harness as the other seats, plus the shared
`csuite/collab.py` consult/direct primitive.

## What it shows

A 6-beat C-suite-meeting arc (`drive.py`): call to order (the agenda); round the
table (CFO+COO, then CTO+CXO) with every figure attributed to its officer; the CEO's
cross-functional synthesis (priorities + risks); a **directive** to the COO to cut a
pending AFT batch — the COO's *own* lever fires (or refuses); and **verified minutes**
where the CEO reads back the officer's fresh `agent_action_ledger` row to prove a
lever actually fired, then records its own `append_agent_action('ceo', …)` directive
row. Nothing is faked: if no cuttable batch exists the COO refuses and the CEO
honestly reports `officer_acted=false`.

## Run it

```bash
# one command: bring the C-suite up -> (best-effort) seed a batch -> drive
demos/10-ceo/run-demo.sh

# against an already-deployed C-suite:
demos/10-ceo/run-demo.sh --no-up

# drive against the estate as-is (no seed):
demos/10-ceo/run-demo.sh --no-seed
```

Prereqs: docker + kind + kubectl + uv; the bank stack up (postgres + bank-api); the
four officer seats (`cfo`/`coo`/`cto`/`cxo`) deployed; `nano-agent-secrets` minted.
For the directive to fire a lever, an **open AFT batch with entries** must exist
(seed via `testing/aft`); otherwise the demo shows the honest no-lever read-back.

## Honesty note

The CEO **bypasses no guardrail**. A directive is an imperative to the officer's own
`/ask`; the officer decides and acts (or refuses) via its existing self-verifying,
ledger-audited lever. The CEO proves what happened by reading back the officer's
ledger row — it never claims an action completed when no lever fired.

Present console (a C-suite-meeting view): `demos/10-ceo/present/` on `:8511`.
