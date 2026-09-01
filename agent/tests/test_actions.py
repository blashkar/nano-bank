from decimal import Decimal
import pytest
from agent.actions import ActionStore, ActDenied, ActError


class FakeDB:
    def __init__(self, owned): self.owned = set(owned)
    def owns_account(self, customer_id, account_id): return account_id in self.owned
    def interac_recipients(self, customer_id): return getattr(self, "recipients", [])


class FakeBank:
    def __init__(self):
        self.calls = []; self.withdraw_calls = []; self.etransfers = []
        self.loan_applications = []; self.disbursements = []

    def transfer(self, token, from_account, to_account, amount, memo=None, idempotency_key=None):
        self.calls.append(("transfer", idempotency_key, str(amount)))
        return {"transaction_id": "t-" + idempotency_key}

    def withdraw(self, token, account_id, amount, description="Withdrawal", idempotency_key=None):
        self.withdraw_calls.append((idempotency_key, str(amount), description))
        return {"transaction_id": "w-" + (idempotency_key or "x")}

    def send_etransfer(self, token, from_account_id, amount, recipient_handle_value,
                       recipient_handle_type="email", security_question=None,
                       security_answer=None, memo=None, idempotency_key=None):
        self.etransfers.append({"from": from_account_id, "amount": str(amount),
                                "handle": recipient_handle_value, "q": security_question,
                                "a": security_answer, "memo": memo})
        return {"etransfer_id": "e-" + (idempotency_key or "x"), "status": "held"}

    def apply_for_loan(self, token, principal_amount, interest_rate, amortization_months):
        self.loan_applications.append((str(principal_amount), str(interest_rate), amortization_months))
        return {"loan_id": "L1", "status": "pending_disbursement"}

    def disburse_loan(self, token, loan_id):
        self.disbursements.append(loan_id)
        return {"loan_id": loan_id, "status": "active"}


class FakeAudit:
    def __init__(self): self.events = []
    def record(self, e): self.events.append(e); return "a"


def _store(**kw):
    clock = {"t": 1000.0}
    db = kw.get("db", FakeDB(["acc-from", "acc-to"]))
    bank = kw.get("bank", FakeBank())
    audit = kw.get("audit", FakeAudit())
    s = ActionStore(db, bank, audit, max_per_tx=Decimal("1000"), ttl_s=300,
                    now=lambda: clock["t"])
    return s, db, bank, audit, clock


def test_propose_over_cap_denied_and_audited():
    s, _db, _bank, audit, _clock = _store()
    with pytest.raises(ActDenied):
        s.propose("C", "tok", "transfer", amount="5000", from_account="acc-from",
                  to_account="acc-to")
    assert audit.events[-1]["outcome"] == "denied"


def test_propose_foreign_source_denied():
    s, *_ = _store()
    with pytest.raises(ActDenied):
        s.propose("C", "tok", "transfer", amount="10", from_account="acc-STRANGER",
                  to_account="acc-to")


def test_propose_does_not_move_money():
    s, _db, bank, _audit, _clock = _store()
    out = s.propose("C", "tok", "transfer", amount="50", from_account="acc-from",
                    to_account="acc-to")
    assert "id" in out and bank.calls == []


def test_execute_moves_money_once_idempotent():
    s, _db, bank, _audit, _clock = _store()
    pid = s.propose("C", "tok", "transfer", amount="50", from_account="acc-from",
                    to_account="acc-to")["id"]
    r1 = s.execute(pid, "C", "tok")
    r2 = s.execute(pid, "C", "tok")           # duplicate confirm
    assert r1 == r2
    assert len(bank.calls) == 1               # only one bank call
    assert bank.calls[0][1] == pid            # idempotency key == action id


def test_execute_expired_refused():
    s, _db, bank, _audit, clock = _store()
    pid = s.propose("C", "tok", "transfer", amount="50", from_account="acc-from",
                    to_account="acc-to")["id"]
    clock["t"] += 301
    with pytest.raises(ActError):
        s.execute(pid, "C", "tok")
    assert bank.calls == []


def test_execute_foreign_customer_refused():
    s, _db, bank, _audit, _clock = _store()
    pid = s.propose("C", "tok", "transfer", amount="50", from_account="acc-from",
                    to_account="acc-to")["id"]
    with pytest.raises(ActError):
        s.execute(pid, "OTHER", "tok")


def test_interac_propose_requires_registered_payee():
    db = FakeDB(["acc-1"])
    db.recipients = []  # no payees registered
    s, _db, _bank, _audit, _clock = _store(db=db)
    with pytest.raises(ActDenied):
        s.propose("cust-1", "tok", "interac", amount="10",
                  from_account="acc-1", payee_email="x@y.ca")


def test_interac_execute_sends_over_the_real_rail():
    db = FakeDB(["acc-1"])
    db.recipients = [{"email": "x@y.ca", "display_name": "X"}]
    bank = FakeBank()
    s, _db, _bank, _audit, _clock = _store(db=db, bank=bank)
    prop = s.propose("cust-1", "tok", "interac", amount="10",
                     from_account="acc-1", payee_email="x@y.ca", memo="rent",
                     security_question="pet?", security_answer="rex")
    assert s.get(prop["id"], "cust-1")["payee_email"] == "x@y.ca"
    s.execute(prop["id"], "cust-1", "tok")
    call = bank.etransfers[-1]
    assert call["handle"] == "x@y.ca" and call["amount"] == "10"
    assert call["q"] == "pet?" and call["a"] == "rex" and call["memo"] == "rent"
    assert bank.withdraw_calls == []  # money moves via the rail, not a withdrawal


def test_loan_propose_skips_ownership_and_transfer_cap():
    # principal ($28,000) exceeds the default transfer cap ($1000) but is well
    # under the default loan cap ($100,000) -- propose must not use self.max here.
    s, _db, bank, _audit, _clock = _store()
    out = s.propose("C", "tok", "loan", amount="28000", interest_rate="0.0799",
                    amortization_months=60)
    assert out["kind"] == "loan"
    assert bank.loan_applications == []          # propose never calls the bank


def test_loan_propose_over_loan_cap_denied():
    s, _db, _bank, audit, _clock = _store()
    with pytest.raises(ActDenied):
        s.propose("C", "tok", "loan", amount="500000", interest_rate="0.0799",
                  amortization_months=60)
    assert audit.events[-1]["outcome"] == "denied"


def test_loan_propose_rejects_bad_rate_and_term():
    s, *_ = _store()
    with pytest.raises(ActDenied):
        s.propose("C", "tok", "loan", amount="10000", interest_rate="1.5",
                  amortization_months=60)          # rate > 1
    with pytest.raises(ActDenied):
        s.propose("C", "tok", "loan", amount="10000", interest_rate="0.08",
                  amortization_months=0)            # non-positive term


def test_loan_execute_applies_then_disburses():
    s, _db, bank, _audit, _clock = _store()
    pid = s.propose("C", "tok", "loan", amount="28000", interest_rate="0.0799",
                    amortization_months=60)["id"]
    res = s.execute(pid, "C", "tok")
    assert bank.loan_applications == [("28000", "0.0799", 60)]
    assert bank.disbursements == ["L1"]
    assert res["loan"]["loan_id"] == "L1"
    assert res["disbursement"]["status"] == "active"


def test_loan_summary_mentions_principal_rate_and_term():
    s, *_ = _store()
    out = s.propose("C", "tok", "loan", amount="28000", interest_rate="0.0799",
                    amortization_months=60)
    assert "28000" in out["summary"] and "60" in out["summary"] and "0.0799" in out["summary"]
