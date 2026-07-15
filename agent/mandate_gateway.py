from __future__ import annotations
from dataclasses import dataclass
from decimal import Decimal
from typing import Optional
import httpx


class GatewayError(Exception):
    def __init__(self, status: int, body):
        super().__init__(f"{status}: {body}")
        self.status = status


@dataclass
class Decision:
    allowed: bool
    mandate: Optional[dict]
    reason: str


class MandateClient:
    """Branch-side client for the bank's mandate + agent plane. Holds the agent
    credentials; the external agent never sees these."""

    def __init__(self, base_url: str, agent_id: str, agent_secret: str,
                 http: Optional[httpx.Client] = None):
        self.base = base_url.rstrip("/")
        self.agent_id = agent_id
        self.agent_secret = agent_secret
        self.http = http or httpx.Client(timeout=30)

    def _json(self, r):
        if r.status_code // 100 != 2:
            raise GatewayError(r.status_code, _safe(r))
        return _safe(r)

    def list_mandates(self) -> list:
        r = self.http.post(f"{self.base}/api/v1/auth/agent-mandates",
                           json={"agent_id": self.agent_id, "agent_secret": self.agent_secret})
        return self._json(r)

    def mint_token(self, mandate_id: str) -> str:
        r = self.http.post(f"{self.base}/api/v1/auth/agent-token",
                           json={"agent_id": self.agent_id, "agent_secret": self.agent_secret,
                                 "mandate_id": mandate_id})
        return self._json(r)["access_token"]

    def agent_transfer(self, token, to_account_id, amount, description, idempotency_key):
        r = self.http.post(f"{self.base}/api/v1/agent/transfers",
                           headers={"Authorization": f"Bearer {token}"},
                           json={"to_account_id": to_account_id, "amount": str(amount),
                                 "description": description, "idempotency_key": idempotency_key})
        return r.status_code, _safe(r)

    def register_agent(self, display_name, description="external demo agent") -> dict:
        r = self.http.post(f"{self.base}/api/v1/agents",
                           json={"display_name": display_name, "description": description})
        return self._json(r)

    def create_mandate(self, customer_token, payload: dict) -> dict:
        r = self.http.post(f"{self.base}/api/v1/mandates",
                           headers={"Authorization": f"Bearer {customer_token}"}, json=payload)
        return self._json(r)

    def revoke(self, customer_token, mandate_id) -> None:
        r = self.http.request("DELETE", f"{self.base}/api/v1/mandates/{mandate_id}",
                              headers={"Authorization": f"Bearer {customer_token}"})
        if r.status_code // 100 != 2:
            raise GatewayError(r.status_code, _safe(r))


class MandatePEP:
    """Re-reads the live mandate every check → immediate revocation."""

    def __init__(self, client):
        self.client = client

    def check(self, mandate_id: str, scope: str, amount: Optional[Decimal] = None) -> Decision:
        try:
            live = self.client.list_mandates()
        except Exception as e:  # noqa: BLE001
            return Decision(False, None, f"mandate lookup failed: {e}")
        m = next((x for x in live if x.get("mandate_id") == mandate_id), None)
        if m is None:
            return Decision(False, None, "mandate revoked or expired (not in live set)")
        if scope not in (m.get("scopes") or []):
            return Decision(False, m, f"scope '{scope}' not granted")
        if amount is not None and m.get("max_per_tx") is not None:
            if Decimal(str(amount)) > Decimal(str(m["max_per_tx"])):
                return Decision(False, m, f"amount exceeds per-tx cap {m['max_per_tx']}")
        return Decision(True, m, "ok")


def _safe(r):
    try:
        return r.json()
    except Exception:  # noqa: BLE001
        return {"raw": r.text}
