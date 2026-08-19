#!/usr/bin/env bash
# Build the animated boardroom (inlining the latest recordings) and serve it.
#   demos/10-ceo/present/boardroom-serve.sh          # -> http://localhost:8520/boardroom.html
#   demos/10-ceo/present/boardroom-serve.sh 8531     # a different port
# The page is self-contained (recordings inlined); you can also just open the
# built boardroom.html directly in a browser (file://).
set -euo pipefail
cd "$(dirname "$0")"
PORT="${1:-8520}"
python3 build_boardroom.py
echo "▶ boardroom: http://localhost:${PORT}/boardroom.html   (Ctrl-C to stop)"
exec python3 -m http.server "$PORT" --bind 0.0.0.0
