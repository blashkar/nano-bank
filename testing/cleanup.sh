#!/bin/bash
# Wipe nano-bank test data.
#
# Runs psql *inside* the Postgres pod (kubectl exec), so it needs no host psql
# client — just kubectl and a running cluster. Truncating `customers` cascades
# through every FK dependent (addresses, kyc_documents, accounts, transactions,
# sessions, devices, …), giving a clean slate for the next test run.
#
# Usage:
#   ./cleanup.sh              # stop input containers, show counts, TRUNCATE, show counts
#   ./cleanup.sh --dry-run    # only show row counts, change nothing
#   ./cleanup.sh --keep-generator   # don't stop the input containers first
#
# Env overrides: NS, DB_NAME, DB_USER, DB_PASSWORD, ROOT_TABLE (default customers).
set -euo pipefail
export PATH="$HOME/bin:$PATH"

NS="${NS:-nano-bank}"
DB_NAME="${DB_NAME:-nano_bank_db}"
DB_USER="${DB_USER:-nanobank_user}"
DB_PASSWORD="${DB_PASSWORD:-secure_nano_password_2024!}"
ROOT_TABLE="${ROOT_TABLE:-customers}"

DRY_RUN=0
STOP_GENERATOR=1
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    --keep-generator) STOP_GENERATOR=0 ;;
    -h|--help) sed -n '2,18p' "$0"; exit 0 ;;
    *) echo "unknown arg: $arg" >&2; exit 2 ;;
  esac
done

POD="$(kubectl get pods -n "$NS" -l app=postgres -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || true)"
[ -n "$POD" ] || { echo "❌ no running postgres pod in namespace '$NS'"; exit 1; }

psql_exec() {  # $1 = SQL
  kubectl exec -n "$NS" "$POD" -- env PGPASSWORD="$DB_PASSWORD" \
    psql -U "$DB_USER" -d "$DB_NAME" -At -c "$1"
}

show_counts() {
  echo "  customers:    $(psql_exec 'SELECT count(*) FROM customers')"
  echo "  accounts:     $(psql_exec 'SELECT count(*) FROM accounts')"
  echo "  transactions: $(psql_exec 'SELECT count(*) FROM transactions')"
}

echo "📊 Before:"; show_counts

if [ "$DRY_RUN" = "1" ]; then
  echo "🔎 --dry-run: nothing changed."; exit 0
fi

# Stop the input containers (generator + Visa rails) first so the wipe actually
# sticks — otherwise they keep inserting rows (and the Visa sim would FK-fail on
# the just-wiped system customer until it self-heals). Best-effort; needs podman.
if [ "$STOP_GENERATOR" = "1" ] && command -v podman >/dev/null 2>&1; then
  for c in nano-bank-generator nano-bank-visa; do
    if podman ps --format '{{.Names}}' 2>/dev/null | grep -qx "$c"; then
      echo "⏸  stopping ${c} …"
      podman stop "$c" >/dev/null 2>&1 || true
      STOPPED_CONTAINERS="${STOPPED_CONTAINERS:+$STOPPED_CONTAINERS }$c"
    fi
  done
fi

echo "🧹 TRUNCATE ${ROOT_TABLE} RESTART IDENTITY CASCADE …"
psql_exec "TRUNCATE TABLE ${ROOT_TABLE} RESTART IDENTITY CASCADE" >/dev/null

echo "📊 After:"; show_counts
echo "✅ Done."
if [ -n "${STOPPED_CONTAINERS:-}" ]; then
  echo "ℹ  stopped: ${STOPPED_CONTAINERS} — restart with: podman start ${STOPPED_CONTAINERS}"
fi
exit 0
