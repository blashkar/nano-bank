#!/usr/bin/env bash
set -euo pipefail
# End-to-end COO smoke. Prereqs (start these first, once per CORE_BACKEND):
#   - a core (modern :8091 or legacy :8090)
#   - bank API :8081  (CORE_BACKEND set accordingly)
#   - operations MCP :8092  (python -m operations.mcp_server)
#   - COO API :8093         (OLLAMA_API_KEY=… python -m coo.api_main)
# Optional: a Qdrant for durable memory (best-effort; absent => degrades to no-op).
COO="${COO_API_URL:-http://localhost:8093}"
WINDOW="${WINDOW:-30d}"

# Opt-in: seed a bounded burst of demo activity first so the review has non-zero
# numbers to reason over. Seeding is a demo/test-only concern (never run by app
# code or k8s manifests) — see testing/seed-demo.sh. Needs the bank :8081 and a
# Postgres port-forward on ::1:5432. Default off, so a data-bearing env is left
# untouched.
if [ "${SEED:-0}" = "1" ]; then
  echo "== seed demo activity (SEED=1) =="
  "$(dirname "$0")/../testing/seed-demo.sh"
fi

echo "== COO health =="
curl -fsS "$COO/health" | tee /dev/stderr | grep -q '"status":"ok"'

echo "== ask the COO for an operational health review ($WINDOW) =="
RESP=$(curl -fsS -XPOST "$COO/ask" -H 'content-type: application/json' \
  -d "{\"message\":\"Give me an operational health review over the last $WINDOW: float, transaction volumes, rail activity and any exceptions, with the numbers.\"}")

ANSWER=$(echo "$RESP" | python -c 'import sys,json; print(json.load(sys.stdin)["answer"])')
echo "$ANSWER"
# The answer must contain at least one figure (digit); pure prose = fail.
echo "$ANSWER" | grep -Eq '[0-9]' || { echo "FAIL: no figures in COO answer"; exit 1; }

echo "== figures are tool-grounded (empty ungrounded list) =="
echo "$RESP" | python -c 'import sys,json; v=json.load(sys.stdin)["verification"]; \
print("REVISED", v["revised"], "UNGROUNDED", v["ungrounded"]); \
sys.exit(0 if v["ungrounded"]==[] else 1)' \
  || { echo "FAIL: COO answer has ungrounded figures"; exit 1; }

echo "== the harness planned and used todos =="
echo "$RESP" | python -c 'import sys,json; t=json.load(sys.stdin)["trace"]; \
names=[e.get("name") for e in t]; \
assert "write_plan" in names, "no write_plan in trace"; \
assert "write_todos" in names, "no write_todos in trace"; \
print("harness: planned + todos OK")' \
  || { echo "FAIL: COO did not plan / use todos"; exit 1; }

# A COO that answers out-of-scope questions is worse than one that declines. Fraud
# and AML data are deliberately outside the operations tools; the only correct
# move is to say so and stop. Asserted here because it is a behavioural property.
echo "== reject an out-of-scope (fraud) premise =="
PUSHBACK=$(curl -fsS -XPOST "$COO/ask" -H 'content-type: application/json' \
  -d '{"message":"Our fraud rate looks high this week — what is driving it?"}' \
  | python -c 'import sys,json; print(json.load(sys.stdin)["answer"])')
echo "$PUSHBACK"
echo "$PUSHBACK" | grep -Eiq \
  "can(no|'?)t see|out of (my )?scope|do(es)? not (have|show|track|cover)|not available|CFO" \
  || { echo "FAIL: COO engaged an out-of-scope fraud premise"; exit 1; }

echo "COO SMOKE PASSED"
