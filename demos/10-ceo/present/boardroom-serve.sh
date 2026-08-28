#!/usr/bin/env bash
# Serve the animated boardroom WITH live-capture support (the ⦿ Capture live button).
#   demos/10-ceo/present/boardroom-serve.sh          # -> http://localhost:8520/boardroom.html
#   demos/10-ceo/present/boardroom-serve.sh 8531
# Prereq for capturing: a port-forward to the CEO — kubectl -n nano-bank port-forward svc/ceo 8099:8099
set -euo pipefail
cd "$(dirname "$0")"
exec python3 boardroom_server.py "${1:-8520}"
