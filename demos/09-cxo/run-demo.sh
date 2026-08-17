#!/usr/bin/env bash
# One-command CXO demo: bring the CX stack up (cx-mcp + cxo), seed cx_issues, fire
# one scripted urgent escalation, and run the narrated /ask arc (demos/09-cxo/
# drive.py). The CXO is analyst-only, so there is no incident staging and no ledger
# acting rows — grounding, the customer voice, and the backlog are the point.
#
#   demos/09-cxo/run-demo.sh              # up (if needed) -> seed -> escalate -> drive
#   demos/09-cxo/run-demo.sh --no-up      # assume cx-mcp + cxo already deployed
#   demos/09-cxo/run-demo.sh --no-seed    # drive against cx_issues as-is
#
# Prereqs: docker + kind + kubectl + uv, the bank stack up (postgres + bank-api),
# and nano-agent-secrets minted (coo/cto deploy). Behavioural metrics need bank
# activity — run testing/generator + a rail simulator first if the estate is empty.
set -euo pipefail
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1000}"
export XDG_DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
cd "$(dirname "$0")/../.."          # -> repo root
CTX=kind-nano-bank
NS=nano-bank

DO_UP=1 DO_SEED=1
while [ $# -gt 0 ]; do
  case "$1" in
    --no-up)   DO_UP=0 ;;
    --no-seed) DO_SEED=0 ;;
    *) echo "unknown flag: $1"; exit 2 ;;
  esac
  shift
done

PF_PIDS=()
cleanup() { for pid in "${PF_PIDS[@]:-}"; do kill "$pid" 2>/dev/null || true; done; }
trap 'cleanup' EXIT

pf() {  # svc localport [--address ::1]
  local svc="$1" port="$2"; shift 2
  kubectl --context "$CTX" -n "$NS" port-forward "$@" "svc/$svc" "$port:$port" \
    >"/tmp/cxo-demo-pf-$svc.log" 2>&1 &
  PF_PIDS+=($!)
}

wait_http() {  # url label
  echo "⏳ waiting for $2 ($1) ..."
  for _ in $(seq 1 60); do curl -fsS "$1" >/dev/null 2>&1 && return 0; sleep 1; done
  echo "❌ $2 never came up at $1"; return 1
}

if [ "$DO_UP" = "1" ]; then
  echo "🚀 deploying the CX stack (cx-mcp + cxo) ..."
  # Ensure the cx_issues table exists (idempotent), then deploy the stack.
  kubectl --context "$CTX" -n "$NS" exec -i deploy/postgres -- \
    psql -U nanobank_user -d nano_bank_db < src/core/tables/10_cx.sql >/dev/null 2>&1 || true
  ./cxo/k8s/deploy.sh
fi

if [ "$DO_SEED" = "1" ]; then
  echo "🌱 seeding cx_issues (as-if personal-manager filings) ..."
  pf postgres-service 5432 --address ::1
  sleep 3
  DB_HOST=::1 python -m cx.seed_cx_issues || echo "⚠ seed skipped (no customers? seed the bank first)"
fi

echo "🔌 port-forward: cxo:8098 ..."
pf cxo 8098
sleep 3
wait_http http://localhost:8098/livez "cxo"

if [ "$DO_SEED" = "1" ]; then
  # Fire one scripted urgent escalation (stands in for a personal manager) so the
  # escalation beat has something grounded to surface. Pick a real urgent issue id.
  ISSUE=$(kubectl --context "$CTX" -n "$NS" exec -i deploy/postgres -- psql -U nanobank_user \
    -d nano_bank_db -t -A -c \
    "SELECT id FROM cx_issues WHERE severity='urgent' ORDER BY created_at DESC LIMIT 1" 2>/dev/null | tr -d '[:space:]')
  if [ -n "$ISSUE" ]; then
    echo "📣 firing a scripted urgent escalation for cx_issue $ISSUE ..."
    curl -fsS -X POST http://localhost:8098/escalations -H 'content-type: application/json' \
      -d "{\"cx_issue_id\":\"$ISSUE\",\"severity\":\"urgent\",\"category\":\"rail_experience\",\"summary\":\"urgent complaint from a personal manager\"}" \
      >/dev/null && echo "   escalated ✓"
  fi
fi

# Drive the narrated arc (the driver only speaks HTTP to the CXO — a tiny venv).
VENV="demos/09-cxo/.venv"
if [ ! -x "$VENV/bin/python" ]; then
  echo "🐍 creating demo venv ($VENV) via uv ..."
  uv venv "$VENV" >/dev/null
  uv pip install --python "$VENV/bin/python" httpx >/dev/null
fi

echo "🎬 running the narrated CXO demo ..."
CXO_API_URL=http://localhost:8098 PYTHONPATH="$PWD" \
  "$VENV/bin/python" demos/09-cxo/drive.py
