from decimal import Decimal
from agent.mandate_gateway import MandatePEP


class FakeClient:
    def __init__(self, mandates): self._m = mandates
    def list_mandates(self): return self._m


def _m(scopes, cap="100"):
    return {"mandate_id": "M1", "account_id": "A1", "scopes": scopes,
            "max_per_tx": cap, "daily_cap": None, "daily_used": "0",
            "account_last4": "1234", "account_type": "chequing"}


def test_allows_in_scope_under_cap():
    pep = MandatePEP(FakeClient([_m(["transfer:initiate"], cap="100")]))
    d = pep.check("M1", "transfer:initiate", amount=Decimal("50"))
    assert d.allowed and d.mandate["account_id"] == "A1"


def test_denies_missing_scope():
    pep = MandatePEP(FakeClient([_m(["read:balance"])]))
    assert not pep.check("M1", "account:open").allowed


def test_denies_over_cap():
    pep = MandatePEP(FakeClient([_m(["transfer:initiate"], cap="40")]))
    d = pep.check("M1", "transfer:initiate", amount=Decimal("50"))
    assert not d.allowed and "cap" in d.reason.lower()


def test_denies_revoked_absent_mandate():
    pep = MandatePEP(FakeClient([]))   # revoked → not in the live list
    d = pep.check("M1", "read:balance")
    assert not d.allowed and ("revoked" in d.reason.lower() or "not" in d.reason.lower())
