#!/usr/bin/env bash
# Build the CEO image, load it into Kind, and apply the manifest.
#   ceo/k8s/deploy.sh
# Prereqs: the bank stack up (postgres + bank-api), the four officer seats
# (cfo/coo/cto/cxo) deployed, and nano-agent-secrets minted.
set -euo pipefail
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1000}"
export XDG_DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
cd "$(dirname "$0")/../.."          # -> repo root
CTX=kind-nano-bank
NS=nano-bank

docker build -f ceo/Dockerfile -t nano-ceo:dev .
kind load docker-image nano-ceo:dev --name nano-bank
kubectl --context "$CTX" -n "$NS" apply -f ceo/k8s/ceo.yaml
kubectl --context "$CTX" -n "$NS" rollout status deploy/ceo --timeout=120s
