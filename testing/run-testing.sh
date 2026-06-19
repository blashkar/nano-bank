#!/bin/bash
# Build and run the nano-bank test harness: a mock-customer generator (input)
# and a Streamlit activity viewer (output). Both use podman host networking so
# they reach the host's API (:8081) and Postgres port-forward (:5432) directly.
#
# Prereqs: nano-bank must already be running (./start-nano-bank.sh), i.e. the
# API on :8081 and the Postgres port-forward on :5432.
set -euo pipefail

cd "$(dirname "$0")"

API_BASE_URL="${API_BASE_URL:-http://localhost:8081}"
INTERVAL_SECONDS="${INTERVAL_SECONDS:-3}"
COUNT="${COUNT:-0}"
DB_HOST="${DB_HOST:-::1}"   # kubectl port-forward binds IPv6 loopback here
DB_PORT="${DB_PORT:-5432}"
VIEWER_PORT="${VIEWER_PORT:-8504}"

echo "🔨 Building images …"
podman build -t localhost/nano-bank-viewer:latest    viewer
podman build -t localhost/nano-bank-generator:latest generator

echo "🧹 Removing any existing containers …"
podman rm -f nano-bank-viewer nano-bank-generator >/dev/null 2>&1 || true

echo "📊 Starting viewer on http://localhost:${VIEWER_PORT} …"
podman run -d --name nano-bank-viewer \
  --network=host --restart unless-stopped \
  -e DB_HOST="$DB_HOST" -e DB_PORT="$DB_PORT" \
  localhost/nano-bank-viewer:latest

echo "👥 Starting customer generator (interval=${INTERVAL_SECONDS}s, count=${COUNT}) …"
podman run -d --name nano-bank-generator \
  --network=host --restart unless-stopped \
  -e API_BASE_URL="$API_BASE_URL" \
  -e INTERVAL_SECONDS="$INTERVAL_SECONDS" \
  -e COUNT="$COUNT" \
  localhost/nano-bank-generator:latest

echo ""
echo "✅ Up. Viewer: http://localhost:${VIEWER_PORT}"
echo "   Logs:  podman logs -f nano-bank-generator"
echo "          podman logs -f nano-bank-viewer"
echo "   Stop:  ./stop-testing.sh"
