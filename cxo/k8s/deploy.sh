#!/usr/bin/env bash
# Deploy the CX stack (cx metrics MCP + CXO analyst seat) into the kind nano-bank
# cluster. Mirrors cto/k8s/deploy.sh. Prereqs already up in the cluster:
#   - nano-agent-secrets — provides OLLAMA_API_KEY (minted by coo/cto deploy)
#   - agent-qdrant       — CXO durable memory (best-effort)
#   - postgres-service   — the bank DB the cx MCP reads (incl. the cx_issues table;
#                          apply src/core/tables/10_cx.sql if the DB predates it)
set -euo pipefail
cd "$(dirname "$0")/../.."          # -> repo root
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1000}"
export XDG_DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
CTX=kind-nano-bank

echo "🐳 Building + loading images..."
docker build -f cx/Dockerfile  -t nano-cx:dev  cx        # context = cx/ (self-contained)
docker build -f cxo/Dockerfile -t nano-cxo:dev .         # context = repo root (needs csuite)
kind load docker-image nano-cx:dev nano-cxo:dev --name nano-bank

if ! kubectl --context "$CTX" -n nano-bank get secret nano-agent-secrets >/dev/null 2>&1; then
  echo "❌ nano-agent-secrets missing — run coo/k8s/deploy.sh first (mints OLLAMA_API_KEY)."
  exit 1
fi

echo "📦 Applying manifests..."
kubectl --context "$CTX" apply -f cx/k8s/cx-mcp.yaml
kubectl --context "$CTX" apply -f cxo/k8s/cxo.yaml
kubectl --context "$CTX" -n nano-bank rollout status deploy/cx-mcp --timeout=180s
kubectl --context "$CTX" -n nano-bank rollout status deploy/cxo    --timeout=240s

echo "✅ CX stack up. Health:"
POD=$(kubectl --context "$CTX" get pod -n nano-bank -l app=cxo -o jsonpath='{.items[0].metadata.name}')
kubectl --context "$CTX" exec -n nano-bank "$POD" -- \
  python -c 'import urllib.request,json; print(json.dumps(json.load(urllib.request.urlopen("http://localhost:8098/health"))))'
