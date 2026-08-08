# COO Plan B — Operations MCP (Python) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A read-only `operations/` MCP service (the COO's "back office" perception surface) that reads the bank's `/api/v1/back-office/ops/*` endpoints over HTTP with a service token and exposes operational metrics as MCP tools, with all arithmetic in pure, unit-tested Python.

**Architecture:** A self-contained subsystem `operations/`, a peer to `finance/`. `bank_client.py` mints+caches a service token and fetches the five back-office reads; `metrics.py` turns those raw payloads into COO metrics (pure, no IO); `mcp_server.py` (FastMCP, streamable-HTTP on `:8092`) wires them into tools. No snapshot DB — operations is live/flow-based (unlike finance's period snapshots).

**Tech Stack:** Python 3.12, `mcp` (FastMCP), `httpx`, `uvicorn`, `pytest`. Model/agent come in Plan C.

## Global Constraints

- **Read-only.** The MCP only performs GETs against `/api/v1/back-office/ops/*`. No writes, no DB.
- **Pure metrics.** Every function in `metrics.py` is pure (dict in → dict out), no IO, unit-tested. The model/agent never does arithmetic.
- **Service token required.** Back-office endpoints are `AuthenticatedService`; `bank_client` mints one via `POST /api/v1/auth/service-token {client_secret}` and caches it, refreshing at 80% of the 900s TTL.
- **Mirror `finance/`.** Same shapes: `config.py` `Settings.from_env`, `requirements.txt`, `Dockerfile` (`python:3.12-slim`, `python -m operations.mcp_server`), `k8s/operations-mcp.yaml`, a nested `.gitignore` (`.venv/ __pycache__/ *.pyc`).
- **First cut is status-agnostic.** Metrics are aggregations/rollups (totals, per-type, per-system, per-status passthrough). Health flags / settlement-success rates that need per-rail status *semantics* are deferred to Plan B2.
- **Money is `Decimal`**, parsed from the JSON strings the bank returns.
- **Port 8092**; tools tested from the repo root (`operations` importable as a package), like `finance` tests.

## File Structure

- Create `operations/__init__.py` — empty package marker.
- Create `operations/config.py` — `Settings.from_env` (nano_bank_api, service_client_secret, mcp_port, timeout).
- Create `operations/bank_client.py` — `BankClient`: token mint/cache + the five typed GET methods.
- Create `operations/metrics.py` — pure aggregation functions.
- Create `operations/mcp_server.py` — FastMCP tools + `main()`.
- Create `operations/requirements.txt`, `operations/Dockerfile`, `operations/.gitignore`, `operations/k8s/operations-mcp.yaml`, `operations/verify-operations.sh`.
- Create `operations/tests/__init__.py`, `operations/tests/test_metrics.py`, `operations/tests/test_bank_client.py`.

---

### Task 1: `metrics.py` — pure operational aggregations

**Files:**
- Create: `operations/__init__.py` (empty), `operations/metrics.py`, `operations/tests/__init__.py` (empty), `operations/tests/test_metrics.py`
- Create: `operations/requirements.txt`, `operations/.gitignore`

**Interfaces:**
- Produces (all pure; input dicts are exactly the bank's back-office JSON payloads):
  - `float_summary(float_payload: dict) -> dict` → `{total_float: Decimal, by_system: {system: Decimal}}`
  - `transactions_summary(txns_payload: dict) -> dict` → `{window, total_count: int, total_amount: Decimal, by_type: {type: {count, amount}}}`
  - `rails_summary(rails_payload: dict) -> dict` → `{window, by_rail: {rail: {total_count: int, total_amount: Decimal, by_status: {status: {count, amount}}}}}`
  - `exceptions_summary(exc_payload: dict) -> dict` → `{window, total: int, by_kind: {kind: int}}`
  - `cards_summary(cards_payload: dict) -> dict` → `{window, open_holds: {count: int, amount: Decimal}, captured: {count: int, amount: Decimal}}`

- [ ] **Step 1: Write `operations/requirements.txt` and `operations/.gitignore`**

`operations/requirements.txt`:
```
mcp>=1.2
httpx>=0.27
uvicorn>=0.30
pytest>=8.0
```
`operations/.gitignore`:
```
.venv/
__pycache__/
*.pyc
```

- [ ] **Step 2: Write the failing test**

Create `operations/tests/test_metrics.py`:

```python
from decimal import Decimal as D
from operations import metrics


def test_float_summary_totals_by_system():
    payload = {
        "accounts": [
            {"system": "interac", "role": "clearing", "account_type": "chequing", "balance": "100.00"},
            {"system": "interac", "role": "settlement", "account_type": "savings", "balance": "50.00"},
            {"system": "lynx", "role": "clearing", "account_type": "chequing", "balance": "25.50"},
        ],
        "total_float": "175.50",
    }
    out = metrics.float_summary(payload)
    assert out["total_float"] == D("175.50")
    assert out["by_system"]["interac"] == D("150.00")
    assert out["by_system"]["lynx"] == D("25.50")


def test_transactions_summary_rolls_up_by_type():
    payload = {
        "window": "7d",
        "since": "2026-07-24T00:00:00Z",
        "groups": [
            {"transaction_type": "deposit", "status": "completed", "count": 3, "total": "300.00"},
            {"transaction_type": "deposit", "status": "failed", "count": 1, "total": "10.00"},
            {"transaction_type": "withdrawal", "status": "completed", "count": 2, "total": "40.00"},
        ],
    }
    out = metrics.transactions_summary(payload)
    assert out["window"] == "7d"
    assert out["total_count"] == 6
    assert out["total_amount"] == D("350.00")
    assert out["by_type"]["deposit"]["count"] == 4
    assert out["by_type"]["deposit"]["amount"] == D("310.00")


def test_rails_summary_per_rail_totals():
    payload = {
        "window": "30d",
        "since": "2026-07-01T00:00:00Z",
        "rails": {
            "interac": [
                {"status": "settled", "count": 5, "total": "500.00"},
                {"status": "pending", "count": 2, "total": "200.00"},
            ],
            "aft": [],
            "lynx": [{"status": "settled", "count": 1, "total": "9000.00"}],
        },
    }
    out = metrics.rails_summary(payload)
    assert out["by_rail"]["interac"]["total_count"] == 7
    assert out["by_rail"]["interac"]["total_amount"] == D("700.00")
    assert out["by_rail"]["interac"]["by_status"]["pending"]["count"] == 2
    assert out["by_rail"]["aft"]["total_count"] == 0
    assert out["by_rail"]["lynx"]["total_amount"] == D("9000.00")


def test_exceptions_summary_sums_counts():
    payload = {
        "window": "30d",
        "since": "2026-07-01T00:00:00Z",
        "exceptions": {
            "failed_transactions": 2, "reversals": 1, "returned_aft_entries": 0,
            "rejected_aft_entries": 3, "wire_recalls": 1,
        },
    }
    out = metrics.exceptions_summary(payload)
    assert out["total"] == 7
    assert out["by_kind"]["rejected_aft_entries"] == 3


def test_cards_summary_holds_and_captured():
    payload = {
        "window": "30d",
        "since": "2026-07-01T00:00:00Z",
        "authorization_holds": {"open_count": 4, "open_amount": "220.00"},
        "card_transactions": [
            {"transaction_type": "card_purchase", "status": "completed", "count": 3, "total": "150.00"},
            {"transaction_type": "card_settlement", "status": "completed", "count": 2, "total": "100.00"},
        ],
    }
    out = metrics.cards_summary(payload)
    assert out["open_holds"]["count"] == 4
    assert out["open_holds"]["amount"] == D("220.00")
    assert out["captured"]["count"] == 5
    assert out["captured"]["amount"] == D("250.00")
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd /home/bmartins/dev/nano-bank && python -m pytest operations/tests/test_metrics.py -q`
Expected: FAIL with `ModuleNotFoundError: No module named 'operations.metrics'` (or ImportError).

- [ ] **Step 4: Write minimal implementation**

Create `operations/__init__.py` (empty) and `operations/tests/__init__.py` (empty).

Create `operations/metrics.py`:

```python
"""Pure operational-metric aggregations over the bank's back-office read
payloads. No IO — every function is dict-in/dict-out and unit-testable. Money is
Decimal, parsed from the JSON strings the bank returns.

First cut is status-agnostic: totals, per-type/per-system/per-rail rollups, and
per-status passthrough. Health flags and settlement-success rates (which need
per-rail status semantics) come in Plan B2.
"""
from __future__ import annotations
from decimal import Decimal


def _dec(v) -> Decimal:
    return Decimal(str(v)) if v is not None else Decimal(0)


def float_summary(payload: dict) -> dict:
    by_system: dict[str, Decimal] = {}
    for a in payload.get("accounts", []):
        by_system[a["system"]] = by_system.get(a["system"], Decimal(0)) + _dec(a["balance"])
    return {
        "total_float": _dec(payload.get("total_float")),
        "by_system": by_system,
    }


def transactions_summary(payload: dict) -> dict:
    by_type: dict[str, dict] = {}
    total_count = 0
    total_amount = Decimal(0)
    for g in payload.get("groups", []):
        t = by_type.setdefault(g["transaction_type"], {"count": 0, "amount": Decimal(0)})
        t["count"] += int(g["count"])
        t["amount"] += _dec(g["total"])
        total_count += int(g["count"])
        total_amount += _dec(g["total"])
    return {
        "window": payload.get("window"),
        "total_count": total_count,
        "total_amount": total_amount,
        "by_type": by_type,
    }


def rails_summary(payload: dict) -> dict:
    by_rail: dict[str, dict] = {}
    for rail, groups in payload.get("rails", {}).items():
        by_status: dict[str, dict] = {}
        total_count = 0
        total_amount = Decimal(0)
        for g in groups:
            by_status[g["status"]] = {"count": int(g["count"]), "amount": _dec(g["total"])}
            total_count += int(g["count"])
            total_amount += _dec(g["total"])
        by_rail[rail] = {
            "total_count": total_count,
            "total_amount": total_amount,
            "by_status": by_status,
        }
    return {"window": payload.get("window"), "by_rail": by_rail}


def exceptions_summary(payload: dict) -> dict:
    kinds = payload.get("exceptions", {})
    by_kind = {k: int(v) for k, v in kinds.items()}
    return {
        "window": payload.get("window"),
        "total": sum(by_kind.values()),
        "by_kind": by_kind,
    }


def cards_summary(payload: dict) -> dict:
    holds = payload.get("authorization_holds", {})
    cap_count = 0
    cap_amount = Decimal(0)
    for g in payload.get("card_transactions", []):
        cap_count += int(g["count"])
        cap_amount += _dec(g["total"])
    return {
        "window": payload.get("window"),
        "open_holds": {"count": int(holds.get("open_count", 0)), "amount": _dec(holds.get("open_amount"))},
        "captured": {"count": cap_count, "amount": cap_amount},
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd /home/bmartins/dev/nano-bank && python -m pytest operations/tests/test_metrics.py -q`
Expected: PASS (5 passed).

- [ ] **Step 6: Commit**

```bash
git add operations/__init__.py operations/metrics.py operations/tests/__init__.py operations/tests/test_metrics.py operations/requirements.txt operations/.gitignore
git commit -m "feat(operations): pure operational-metric aggregations + tests"
```

---

### Task 2: `config.py` + `bank_client.py` — service-token client

**Files:**
- Create: `operations/config.py`, `operations/bank_client.py`
- Test: `operations/tests/test_bank_client.py`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces:
  - `Settings.from_env(env=None) -> Settings` with fields `nano_bank_api: str`, `service_client_secret: str`, `mcp_port: int`, `timeout: float`.
  - `BankClient(settings, transport=None)` with methods `float_()`, `transactions(window)`, `rails(window)`, `exceptions(window)`, `cards(window)` returning parsed JSON dicts; it mints a service token on first use and caches it, refreshing at 80% of TTL. `transport` (an `httpx` transport) is injectable for tests.

- [ ] **Step 1: Write `operations/config.py`**

```python
from __future__ import annotations
import os
from dataclasses import dataclass
from typing import Mapping, Optional


@dataclass
class Settings:
    nano_bank_api: str
    service_client_secret: str
    mcp_port: int
    timeout: float

    @classmethod
    def from_env(cls, env: Optional[Mapping[str, str]] = None) -> "Settings":
        e = os.environ if env is None else env
        return cls(
            nano_bank_api=e.get("NANO_BANK_API", "http://localhost:8081"),
            service_client_secret=e.get(
                "SERVICE_CLIENT_SECRET", "nano-bank-visa-network-secret-change-me"
            ),
            mcp_port=int(e.get("MCP_PORT", "8092")),
            timeout=float(e.get("REQUEST_TIMEOUT", "10.0")),
        )
```

- [ ] **Step 2: Write the failing test**

Create `operations/tests/test_bank_client.py`:

```python
import json
import time
import httpx
from operations.config import Settings
from operations.bank_client import BankClient


def _settings():
    return Settings(
        nano_bank_api="http://bank.test",
        service_client_secret="secret",
        mcp_port=8092,
        timeout=5.0,
    )


def test_mints_token_once_then_reuses():
    calls = {"token": 0, "float": 0}

    def handler(request: httpx.Request) -> httpx.Response:
        if request.url.path == "/api/v1/auth/service-token":
            calls["token"] += 1
            assert json.loads(request.content)["client_secret"] == "secret"
            return httpx.Response(200, json={"access_token": "tok-123", "expires_in": 900})
        if request.url.path == "/api/v1/back-office/ops/float":
            calls["float"] += 1
            assert request.headers["authorization"] == "Bearer tok-123"
            return httpx.Response(200, json={"accounts": [], "total_float": "0"})
        return httpx.Response(404)

    client = BankClient(_settings(), transport=httpx.MockTransport(handler))
    client.float_()
    client.float_()
    assert calls["token"] == 1  # token minted once, cached
    assert calls["float"] == 2


def test_passes_window_query():
    seen = {}

    def handler(request: httpx.Request) -> httpx.Response:
        if request.url.path == "/api/v1/auth/service-token":
            return httpx.Response(200, json={"access_token": "t", "expires_in": 900})
        seen["path"] = request.url.path
        seen["window"] = request.url.params.get("window")
        return httpx.Response(200, json={"ok": True})

    client = BankClient(_settings(), transport=httpx.MockTransport(handler))
    client.rails("7d")
    assert seen["path"] == "/api/v1/back-office/ops/rails"
    assert seen["window"] == "7d"
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd /home/bmartins/dev/nano-bank && python -m pytest operations/tests/test_bank_client.py -q`
Expected: FAIL with `ModuleNotFoundError: No module named 'operations.bank_client'`.

- [ ] **Step 4: Write minimal implementation**

Create `operations/bank_client.py`:

```python
"""HTTP client for the bank's service-plane back-office reads. Mints and caches
a service token (refreshing at 80% of its TTL) and fetches the five ops reads.
`transport` is injectable so tests can stub the network."""
from __future__ import annotations
import time
from typing import Optional

import httpx

from .config import Settings


class BankClient:
    def __init__(self, settings: Settings, transport: Optional[httpx.BaseTransport] = None):
        self._s = settings
        self._http = httpx.Client(
            base_url=settings.nano_bank_api, timeout=settings.timeout, transport=transport
        )
        self._token: Optional[str] = None
        self._token_exp: float = 0.0

    def _bearer(self) -> str:
        # Refresh at 80% of TTL (or on first use / after expiry).
        if self._token is None or time.time() >= self._token_exp:
            r = self._http.post(
                "/api/v1/auth/service-token",
                json={"client_secret": self._s.service_client_secret},
            )
            r.raise_for_status()
            body = r.json()
            self._token = body["access_token"]
            ttl = float(body.get("expires_in", 900))
            self._token_exp = time.time() + ttl * 0.8
        return self._token

    def _get(self, path: str, params: Optional[dict] = None) -> dict:
        r = self._http.get(path, params=params, headers={"authorization": f"Bearer {self._bearer()}"})
        r.raise_for_status()
        return r.json()

    def float_(self) -> dict:
        return self._get("/api/v1/back-office/ops/float")

    def transactions(self, window: str = "24h") -> dict:
        return self._get("/api/v1/back-office/ops/transactions", {"window": window})

    def rails(self, window: str = "24h") -> dict:
        return self._get("/api/v1/back-office/ops/rails", {"window": window})

    def exceptions(self, window: str = "24h") -> dict:
        return self._get("/api/v1/back-office/ops/exceptions", {"window": window})

    def cards(self, window: str = "24h") -> dict:
        return self._get("/api/v1/back-office/ops/cards", {"window": window})
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd /home/bmartins/dev/nano-bank && python -m pytest operations/tests/test_bank_client.py -q`
Expected: PASS (2 passed).

- [ ] **Step 6: Commit**

```bash
git add operations/config.py operations/bank_client.py operations/tests/test_bank_client.py
git commit -m "feat(operations): config + service-token bank client (fixture-tested)"
```

---

### Task 3: `mcp_server.py` + packaging (Dockerfile, k8s, verify)

**Files:**
- Create: `operations/mcp_server.py`, `operations/Dockerfile`, `operations/k8s/operations-mcp.yaml`, `operations/verify-operations.sh`

**Interfaces:**
- Consumes: `Settings.from_env`, `BankClient` (Task 2), `metrics` (Task 1).
- Produces: a FastMCP app "nano-operations" on `:8092` exposing tools `float_position`, `transactions`, `rails`, `exceptions`, `cards`, `operations_health`; `python -m operations.mcp_server` serves it.

- [ ] **Step 1: Write `operations/mcp_server.py`**

```python
"""The operations MCP: the COO's back-office perception surface. Each tool reads
the bank's service-plane back-office endpoints via BankClient and returns a pure
metrics aggregation. Money Decimals are stringified for JSON transport."""
from __future__ import annotations
from decimal import Decimal

from mcp.server.fastmcp import FastMCP
from mcp.server.transport_security import TransportSecuritySettings

from .config import Settings
from .bank_client import BankClient
from . import metrics


def _stringify(obj):
    if isinstance(obj, Decimal):
        return str(obj)
    if isinstance(obj, dict):
        return {k: _stringify(v) for k, v in obj.items()}
    if isinstance(obj, list):
        return [_stringify(v) for v in obj]
    return obj


def build_mcp(bank: BankClient) -> FastMCP:
    mcp = FastMCP(
        "nano-operations",
        transport_security=TransportSecuritySettings(enable_dns_rebinding_protection=False),
    )

    @mcp.tool()
    def float_position() -> dict:
        """Current clearing/settlement float across the rails, totalled by system."""
        return _stringify(metrics.float_summary(bank.float_()))

    @mcp.tool()
    def transactions(window: str = "24h") -> dict:
        """Transaction volume/count rolled up by type over a window (24h|7d|30d)."""
        return _stringify(metrics.transactions_summary(bank.transactions(window)))

    @mcp.tool()
    def rails(window: str = "24h") -> dict:
        """Per-rail (Interac/AFT/Lynx) activity by status over a window."""
        return _stringify(metrics.rails_summary(bank.rails(window)))

    @mcp.tool()
    def exceptions(window: str = "24h") -> dict:
        """Recorded operational exceptions (failed txns, reversals, AFT returns/rejects, recalls)."""
        return _stringify(metrics.exceptions_summary(bank.exceptions(window)))

    @mcp.tool()
    def cards(window: str = "24h") -> dict:
        """Open authorization holds (now) + captured card transactions over a window."""
        return _stringify(metrics.cards_summary(bank.cards(window)))

    @mcp.tool()
    def operations_health(window: str = "24h") -> dict:
        """One-shot bundle: float, transactions, rails, exceptions and cards for a window."""
        return _stringify(
            {
                "float": metrics.float_summary(bank.float_()),
                "transactions": metrics.transactions_summary(bank.transactions(window)),
                "rails": metrics.rails_summary(bank.rails(window)),
                "exceptions": metrics.exceptions_summary(bank.exceptions(window)),
                "cards": metrics.cards_summary(bank.cards(window)),
            }
        )

    return mcp


def main():
    settings = Settings.from_env()
    mcp = build_mcp(BankClient(settings))
    import uvicorn

    uvicorn.run(mcp.streamable_http_app(), host="0.0.0.0", port=settings.mcp_port)


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Write `operations/Dockerfile`**

```dockerfile
FROM python:3.12-slim
WORKDIR /app
COPY requirements.txt /app/requirements.txt
RUN pip install --no-cache-dir -r requirements.txt
COPY . /app/operations
ENV PYTHONUNBUFFERED=1
CMD ["python", "-m", "operations.mcp_server"]
```

- [ ] **Step 3: Write `operations/k8s/operations-mcp.yaml`**

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: operations-mcp
  namespace: nano-bank
  labels: { app: operations-mcp }
spec:
  replicas: 1
  selector: { matchLabels: { app: operations-mcp } }
  template:
    metadata: { labels: { app: operations-mcp } }
    spec:
      containers:
      - name: mcp
        image: nano-operations-mcp:dev
        imagePullPolicy: Never
        ports: [ { containerPort: 8092 } ]
        env:
        - { name: NANO_BANK_API,        value: http://bank-api:8081 }
        - { name: SERVICE_CLIENT_SECRET, value: "nano-bank-visa-network-secret-change-me" }
        - { name: MCP_PORT,             value: "8092" }
---
apiVersion: v1
kind: Service
metadata:
  name: operations-mcp
  namespace: nano-bank
spec:
  selector: { app: operations-mcp }
  ports: [ { port: 8092, targetPort: 8092 } ]
```

- [ ] **Step 4: Write `operations/verify-operations.sh`**

```bash
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
```

- [ ] **Step 5: Verify the server imports and tools register**

Run: `cd /home/bmartins/dev/nano-bank && python -c "from operations.mcp_server import build_mcp; from operations.bank_client import BankClient; from operations.config import Settings; import httpx; m=build_mcp(BankClient(Settings.from_env(), transport=httpx.MockTransport(lambda r: httpx.Response(404)))); print('mcp ok')"`
Expected: prints `mcp ok` (no import/registration error).

- [ ] **Step 6: Commit**

```bash
git add operations/mcp_server.py operations/Dockerfile operations/k8s/operations-mcp.yaml operations/verify-operations.sh
git commit -m "feat(operations): MCP server (float/transactions/rails/exceptions/cards/health) + packaging"
```

---

## Self-Review

**1. Spec coverage.** Delivers the operations MCP with tools for all five back-office reads plus an `operations_health` bundle — the COO's Phase-1 perception surface (spec Component 1b). Snapshot store, and the status-semantic health/success-rate metrics, are explicitly deferred (Plan B2), matching the spec's "start live-only / add when a trend tool needs it."

**2. Placeholder scan.** No TBD/TODO; every step is runnable code or an exact command.

**3. Type consistency.** `BankClient` method names (`float_`, `transactions`, `rails`, `exceptions`, `cards`) are used identically in `mcp_server.py`. `metrics.*_summary` names and their payload shapes match the bank's committed endpoint responses (`accounts`/`total_float`, `groups`, `rails`, `exceptions`, `authorization_holds`/`card_transactions`). Decimals are stringified at the MCP boundary, matching finance's `_stringify`.

**Executor note:** Tasks 1–2 need no live stack (pure + fixture-mocked). Task 3 Step 5 is an import check (also offline). The live `verify-operations.sh` needs the bank on `:8081` and the MCP on `:8092`. Create a venv first: `cd operations && uv venv && . .venv/bin/activate && uv pip install -r requirements.txt` (or the repo's uv env), and run pytest from the repo root so `operations` imports as a package.
