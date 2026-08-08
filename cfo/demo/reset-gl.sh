#!/usr/bin/env bash
# Reset the demo's general ledger so each run starts from a clean, reproducible
# book. Clears the modern core's journal and nano-bank's period snapshots, then
# re-applies the core's GL chart seed (idempotent — it also installs the
# bank-economics accounts if the running core predates spec #1).
#
# Scope: the GL core's journal + gl_snapshots ONLY. nano-bank's own Postgres —
# customers, accounts, transactions, mandates, rail history — is NOT touched.
# Stale transactions.metadata.gl_entry ids are left dangling; nothing in the
# reporting path reads them.
#
#   bash cfo/demo/reset-gl.sh
set -euo pipefail

cd "$(dirname "$0")/../.."
CORE_CTX="${CORE_KUBE_CONTEXT:-kind-modern-core}"
CORE_NS="${CORE_NAMESPACE:-modern-core}"
CORE_DB_DEPLOY="${CORE_DB_DEPLOY:-deploy/modern-core-db}"
CORE_SEED="${CORE_SEED:-$HOME/dev/nano-bank-modern-core/resources/seed.sql}"

psql_core() { kubectl --context "$CORE_CTX" -n "$CORE_NS" exec -i "$CORE_DB_DEPLOY" \
                -- psql -qtA -U core -d modern_core "$@"; }

echo "== clearing the core journal ($CORE_CTX/$CORE_NS)"
# journal_line -> journal_entry, and clearing/dunning_notice reference lines.
psql_core -c "TRUNCATE clearing, dunning_notice, journal_line, journal_entry RESTART IDENTITY CASCADE;" \
  >/dev/null
echo "   journal cleared"

if [ -f "$CORE_SEED" ]; then
  psql_core < "$CORE_SEED" >/dev/null
  echo "   GL chart re-seeded from $CORE_SEED"
else
  echo "   ! core seed not found at $CORE_SEED — skipping chart re-seed"
fi
echo "   accounts on the chart: $(psql_core -c 'SELECT count(*) FROM gl_account;')"

echo "== clearing period snapshots"
source finance/.venv/bin/activate
python - <<'PY'
from finance.config import Settings
from finance.db import FinanceDB
db = FinanceDB(Settings.from_env().db)
db.ensure_schema()
db._exec("DELETE FROM gl_snapshots", ())
print("   gl_snapshots cleared")
PY

echo "GL reset. Run: bash cfo/demo/seed-demo-bank.sh"
