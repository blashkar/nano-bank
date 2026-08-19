#!/usr/bin/env bash
# One-command CEO demo: bring the C-suite up (cfo+coo+cto+cxo+ceo), optionally seed
# a pending AFT batch so the directive has something real to cut, and run the
# narrated C-suite-meeting arc (demos/10-ceo/drive.py). The CEO consults all four
# officers, synthesizes a brief, and directs the COO — whose lever cuts the batch;
# the CEO's directive row reads back the COO's ledger row.
#
#   demos/10-ceo/run-demo.sh            # up (if needed) -> seed -> drive
#   demos/10-ceo/run-demo.sh --no-up    # assume the C-suite is already deployed
#   demos/10-ceo/run-demo.sh --no-seed  # drive against the estate as-is
#
# Prereqs: docker + kind + kubectl + uv, the bank stack up (postgres + bank-api),
# the four officer seats deployed, and nano-agent-secrets minted.
set -euo pipefail
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1000}"
export XDG_DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
cd "$(dirname "$0")/../.."          # -> repo root
CTX=kind-nano-bank
NS=nano-bank

DO_UP=1 DO_SEED=1
EMIT_ARG=""
while [ $# -gt 0 ]; do
  case "$1" in
    --no-up)      DO_UP=0 ;;
    --no-seed)    DO_SEED=0 ;;
    --emit-jsonl) EMIT_ARG="--emit-jsonl $2"; shift ;;
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
    >"/tmp/ceo-demo-pf-$svc.log" 2>&1 &
  PF_PIDS+=("$!")
}

if [ "$DO_UP" = 1 ]; then
  ceo/k8s/deploy.sh
fi

# Seed a pending AFT batch (the COO's cut_aft_batch lever needs an open batch with
# entries). Best-effort via the AFT simulator's originate path; if it isn't wired
# here, seed one manually (testing/aft) or run with --no-seed — the directive beat
# then honestly reports officer_acted=false (a valid read-back outcome).
if [ "$DO_SEED" = 1 ]; then
  echo "ℹ  seeding a cuttable AFT batch is a manual/best-effort step; if none exists"
  echo "   the COO will refuse and the CEO will honestly report no lever fired."
  if [ -x testing/.venv/bin/python ] && [ -f testing/seed-demo.sh ]; then
    echo "   (attempting testing/seed-demo.sh best-effort ...)"
    SEED_AFT_ONLY=1 timeout 90 testing/seed-demo.sh >/tmp/ceo-demo-seed.log 2>&1 || \
      echo "   seed skipped (see /tmp/ceo-demo-seed.log)"
  fi
fi

pf ceo 8099
sleep 2
CEO_API_URL="http://localhost:8099" python demos/10-ceo/drive.py $EMIT_ARG
