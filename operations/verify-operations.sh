#!/bin/bash
# Live smoke: with the bank up on :8081 and the operations MCP on :8092, call a
# couple of tools over MCP and assert real JSON comes back. Run with the venv
# active. Exits non-zero on failure.
set -euo pipefail
BASE="${OPERATIONS_MCP_URL:-http://localhost:8092/mcp}"
python - "$BASE" <<'PY'
import sys, anyio
from mcp.client.streamable_http import streamablehttp_client
from mcp.client.session import ClientSession

async def main(url):
    async with streamablehttp_client(url) as (r, w, _):
        async with ClientSession(r, w) as s:
            await s.initialize()
            res = await s.call_tool("operations_health", {"window": "30d"})
            print(res.content[0].text[:400])

anyio.run(main, sys.argv[1])
PY
echo "OK"
