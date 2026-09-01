# Demo 4 Gateway Cinematic + Loan-Aware Personal Manager Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the personal manager real loan capability (explain terms, propose a confirm-gated car-loan application), and give `demos/04-external-agent` a two-tone, split-screen animated replay of a run — external agent vs. personal manager, mediated by the mandate gateway.

**Architecture:** Backend (agent/): a `loan` kind added to the existing propose/execute `ActionStore`, a `get_loans` read tool, and a `loan.md` skill — all following the exact patterns `propose_transfer`/`get_cards`/`savings.md` already establish. Frontend (demos/04-external-agent/): `app.py` is restyled in place into a two-tone nav+centre stepper over one run's `events` list (unchanged behavior otherwise) and now saves each run as a recording; a new `present/` directory holds a standalone animated HTML cinematic (`gateway.html`, built from a template by inlining a captured recording — the same capture→build→replay mechanism as `demos/10-ceo/present/boardroom.html`, adapted to a 2-actor split screen instead of a round table).

**Tech Stack:** Python (FastMCP tools, `ActionStore`, Streamlit), vanilla HTML/CSS/JS (no build tooling, Google Fonts only), pytest.

**Spec:** `docs/superpowers/specs/2026-08-29-demo4-gateway-cinematic-and-loans-design.md`

## Global Constraints

- `./nb up --demo 04-external-agent` (`nb:189`) hardcodes `streamlit run demos/$name/app.py` — `demos/04-external-agent/app.py` MUST stay at that exact path with its current behavior (in-process `ExternalAgent.run()` per click, mandate seed/revoke unchanged). Do not relocate it into `present/`.
- Any new MCP tool the personal manager's LLM should be able to call MUST be added to `agent.mcp_server.LLM_TOOL_NAMES` — that frozenset, not tool registration alone, is what `nano_manager.agent_tools()` filters against (`agent/nano_manager.py:68`).
- `execute_action`/`cancel_action` stay confirm-path only by NOT being in `LLM_TOOL_NAMES` — never add a new kind's execution there; execution only happens via the branch's `/actions/{id}/confirm` HTTP path (`agent/api.py` `confirm_fn`).
- A `loan` proposal's cap check uses a **separate** ceiling (`loan_max_principal`, not `act_max_per_tx`) — a car-loan principal will routinely exceed the $1000 default transfer cap. Apply this cap consistently at both `propose()` and `execute()` (the existing `transfer`/`deposit`/etc. code re-checks the cap at `execute()` too — don't miss that second check-site for `loan`).
- Run agent tests from the repo root: `agent/.venv/bin/python -m pytest agent/tests/<file> -v` (create `agent/.venv` per `agent/README.md` if it doesn't exist: `python -m venv agent/.venv && agent/.venv/bin/pip install -r agent/requirements.txt`).
- `agent/tests/test_skills.py::test_seed_skills_load_from_repo` asserts the **exact** set of product skills via `==` (not a subset) — adding `loan.md` as `kind: product` will break it unless that assertion is updated in the same task.
- The HTML cinematic (`present/gateway.template.html`) is a plain static file served by a stdlib `http.server` (`gateway_server.py`), not a Claude Artifact — no artifact publishing tool is used for it, it's a checked-in repo file.

---

## Part A — Loan-aware personal manager

### Task 1: Config — `loan_max_principal`

**Files:**
- Modify: `agent/config.py`
- Test: `agent/tests/test_config.py`

**Interfaces:**
- Produces: `Settings.loan_max_principal: Decimal`, env var `LOAN_MAX_PRINCIPAL` (default `"100000"`).

- [ ] **Step 1: Write the failing test**

Add to `agent/tests/test_config.py`:

```python
def test_loan_max_principal_default_and_override():
    s = Settings.from_env({})
    assert s.loan_max_principal == Decimal("100000")
    s2 = Settings.from_env({"LOAN_MAX_PRINCIPAL": "50000"})
    assert s2.loan_max_principal == Decimal("50000")
```

- [ ] **Step 2: Run test to verify it fails**

Run: `agent/.venv/bin/python -m pytest agent/tests/test_config.py -v`
Expected: FAIL with `AttributeError: 'Settings' object has no attribute 'loan_max_principal'`

- [ ] **Step 3: Implement**

In `agent/config.py`, add a field to the `Settings` dataclass (after `act_max_per_tx: Decimal`):

```python
    act_max_per_tx: Decimal
    loan_max_principal: Decimal
```

And in `from_env`, add (after `act_max_per_tx=Decimal(...)`):

```python
            act_max_per_tx=Decimal(g("ACT_MAX_PER_TX", "1000")),
            loan_max_principal=Decimal(g("LOAN_MAX_PRINCIPAL", "100000")),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `agent/.venv/bin/python -m pytest agent/tests/test_config.py -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /home/bmartins/dev/nano-bank
git add agent/config.py agent/tests/test_config.py
git commit -m "agent: add Settings.loan_max_principal (LOAN_MAX_PRINCIPAL, default 100000)"
```

---

### Task 2: `BankClient` — apply for and disburse a loan

**Files:**
- Modify: `agent/bank.py`
- Test: `agent/tests/test_bank.py`

**Interfaces:**
- Consumes: `BankClient._post(path, json, token=None, idempotency_key=None) -> dict` (existing).
- Produces: `BankClient.apply_for_loan(token, principal_amount, interest_rate, amortization_months) -> dict`, `BankClient.disburse_loan(token, loan_id) -> dict`.

- [ ] **Step 1: Write the failing tests**

Add to `agent/tests/test_bank.py`:

```python
def test_apply_for_loan_posts_principal_rate_and_months():
    seen = {}

    def handler(req: httpx.Request) -> httpx.Response:
        seen["url"] = str(req.url)
        seen["auth"] = req.headers.get("authorization")
        seen["body"] = json.loads(req.content)
        return httpx.Response(201, json={"loan_id": "L1", "status": "pending_disbursement"})

    out = _client(handler).apply_for_loan("jwt", "28000", "0.0799", 60)
    assert out["loan_id"] == "L1"
    assert seen["url"].endswith("/api/v1/loans")
    assert seen["auth"] == "Bearer jwt"
    assert seen["body"] == {"principal_amount": "28000", "interest_rate": "0.0799",
                            "amortization_months": 60}


def test_disburse_loan_posts_to_loan_id():
    seen = {}

    def handler(req: httpx.Request) -> httpx.Response:
        seen["url"] = str(req.url)
        seen["auth"] = req.headers.get("authorization")
        return httpx.Response(200, json={"loan_id": "L1", "status": "active"})

    out = _client(handler).disburse_loan("jwt", "L1")
    assert out["status"] == "active"
    assert seen["url"].endswith("/api/v1/loans/L1/disburse")
    assert seen["auth"] == "Bearer jwt"
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `agent/.venv/bin/python -m pytest agent/tests/test_bank.py -v`
Expected: FAIL with `AttributeError: 'BankClient' object has no attribute 'apply_for_loan'`

- [ ] **Step 3: Implement**

In `agent/bank.py`, add after `create_account`:

```python
    def apply_for_loan(self, token, principal_amount, interest_rate, amortization_months) -> dict:
        return self._post("/api/v1/loans",
                          {"principal_amount": str(principal_amount),
                           "interest_rate": str(interest_rate),
                           "amortization_months": int(amortization_months)},
                          token=token)

    def disburse_loan(self, token, loan_id) -> dict:
        return self._post(f"/api/v1/loans/{loan_id}/disburse", {}, token=token)
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `agent/.venv/bin/python -m pytest agent/tests/test_bank.py -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add agent/bank.py agent/tests/test_bank.py
git commit -m "agent: add BankClient.apply_for_loan / disburse_loan"
```

---

### Task 3: `ClientContext.loans()` — read a customer's loans

**Files:**
- Modify: `agent/db.py`
- Test: `agent/tests/test_db.py`

**Interfaces:**
- Produces: `ClientContext.loans(customer_id: str) -> list[dict]` — rows with keys `loan_id, account_id, principal_amount, interest_rate, amortization_months, monthly_payment, status, next_payment_date`.

- [ ] **Step 1: Write the failing test**

Add to `agent/tests/test_db.py`, extend `FakeCtx._rows` with a `"-- loans"` branch, and add:

```python
    def _rows(self, sql, params):
        # crude router: pick table by a marker in the SQL comment
        if "-- accounts" in sql:
            return self._tables.get("accounts", [])
        if "-- transactions" in sql:
            return self._tables.get("transactions", [])
        if "-- profile" in sql:
            return self._tables.get("profile", [])
        if "-- owns" in sql:
            return self._tables.get("owns", [])
        if "-- loans" in sql:
            self.last = (sql, params)
            return self._tables.get("loans", [])
        if "-- interac_recipients" in sql:
            self.last = (sql, params)
            return self._tables.get("recipients", [])
        if "-- recipient" in sql:
            self.last = (sql, params)
            return self._tables.get("recipient", [])
        return []


def test_loans_query_shape():
    ctx = FakeCtx({"loans": [{"loan_id": "l1", "account_id": "a1",
                              "principal_amount": "28000.00", "interest_rate": "0.0799",
                              "amortization_months": 60, "monthly_payment": "567.89",
                              "status": "active", "next_payment_date": "2026-09-29"}]})
    out = ctx.loans("cust-1")
    assert out[0]["loan_id"] == "l1"
    sql, params = ctx.last
    assert "FROM loans" in sql and params == ("cust-1",)
```

(Note: `_rows` in `FakeCtx` is being modified, not just appended to — replace the whole method as shown, which now includes the new `-- loans` branch.)

- [ ] **Step 2: Run test to verify it fails**

Run: `agent/.venv/bin/python -m pytest agent/tests/test_db.py -v`
Expected: FAIL with `AttributeError: 'FakeCtx' object has no attribute 'loans'` (inherited from `ClientContext`, which doesn't have it yet)

- [ ] **Step 3: Implement**

In `agent/db.py`, add after `cards()`:

```python
    def loans(self, customer_id: str) -> list[dict]:
        return self._rows(
            "-- loans\nSELECT loan_id, account_id, principal_amount, interest_rate, "
            "amortization_months, monthly_payment, status, next_payment_date "
            "FROM loans WHERE customer_id = %s ORDER BY created_at DESC",
            (customer_id,))
```

- [ ] **Step 4: Run test to verify it passes**

Run: `agent/.venv/bin/python -m pytest agent/tests/test_db.py -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add agent/db.py agent/tests/test_db.py
git commit -m "agent: add ClientContext.loans()"
```

---

### Task 4: `ActionStore` — the `loan` kind (propose + execute)

This is the core money-movement change: a `loan` proposal skips the transfer-style ownership/cap checks (no account exists yet) and uses its own `loan_max_principal` cap; execution applies for the loan and immediately disburses it (nano-bank's lending is servicing-only, no separate underwriting step to wait on).

**Files:**
- Modify: `agent/actions.py`
- Test: `agent/tests/test_actions.py`

**Interfaces:**
- Consumes: `bank.apply_for_loan(token, principal, rate, months) -> dict` (Task 2), `bank.disburse_loan(token, loan_id) -> dict` (Task 2).
- Produces: `ActionStore.__init__(..., loan_max_principal: Decimal = Decimal("100000"))`; `ActionStore.propose(..., kind="loan", amount=principal, interest_rate=..., amortization_months=...)`; `ActionStore.execute(action_id, ...)` for `kind == "loan"` returns `{"loan": <apply response>, "disbursement": <disburse response>}`.

- [ ] **Step 1: Write the failing tests**

Add to `agent/tests/test_actions.py`. First, extend `FakeBank` with loan methods and a `_store()` override for the new cap:

```python
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
```

Then add the new tests (near the interac tests, at the end of the file):

```python
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `agent/.venv/bin/python -m pytest agent/tests/test_actions.py -v`
Expected: FAIL — `ActDenied: unknown kind: loan` (or similar) on the new tests

- [ ] **Step 3: Implement**

In `agent/actions.py`:

1. Add `"loan"` to `_KINDS`:

```python
_KINDS = {"transfer", "deposit", "withdraw", "interac", "loan"}
```

2. Add two fields to `PendingAction` (at the end, alongside `status`/`result`, so they keep defaults):

```python
    status: str = "pending"   # pending | executed | cancelled
    result: Optional[dict] = None
    interest_rate: Optional[str] = None
    amortization_months: Optional[int] = None
```

3. Add `loan_max_principal` to `ActionStore.__init__`:

```python
    def __init__(self, db, bank, audit, max_per_tx: Decimal, ttl_s: int,
                 loan_max_principal: Decimal = Decimal("100000"),
                 now: Callable[[], float] = time.time):
        self.db = db
        self.bank = bank
        self.audit = audit
        self.max = max_per_tx
        self.loan_max_principal = loan_max_principal
        self.ttl = ttl_s
        self.now = now
        self._pending: dict[str, PendingAction] = {}
```

4. Update `propose()`: change its signature and the cap check, and add loan-specific validation. Replace:

```python
    def propose(self, customer_id, token, kind, *, amount,
                from_account=None, to_account=None, memo=None, payee_email=None,
                security_question=None, security_answer=None) -> dict:
        if kind not in _KINDS:
            raise ActDenied(f"unknown kind: {kind}")
        a = self._amount(amount)
        if a > self.max:
            self._audit(customer_id, kind, a, "denied", "over cap")
            raise ActDenied(f"amount {a} exceeds per-transaction cap {self.max}")
```

with:

```python
    def propose(self, customer_id, token, kind, *, amount,
                from_account=None, to_account=None, memo=None, payee_email=None,
                security_question=None, security_answer=None,
                interest_rate=None, amortization_months=None) -> dict:
        if kind not in _KINDS:
            raise ActDenied(f"unknown kind: {kind}")
        a = self._amount(amount)
        cap = self.loan_max_principal if kind == "loan" else self.max
        if a > cap:
            self._audit(customer_id, kind, a, "denied", "over cap")
            cap_name = "loan principal" if kind == "loan" else "per-transaction"
            raise ActDenied(f"amount {a} exceeds {cap_name} cap {cap}")
        if kind == "loan":
            try:
                rate = Decimal(str(interest_rate))
            except (InvalidOperation, ValueError, TypeError):
                raise ActDenied(f"invalid interest_rate: {interest_rate!r}")
            if rate < 0 or rate > 1:
                raise ActDenied("interest_rate must be between 0 and 1")
            try:
                months = int(amortization_months)
            except (ValueError, TypeError):
                raise ActDenied(f"invalid amortization_months: {amortization_months!r}")
            if months <= 0:
                raise ActDenied("amortization_months must be positive")
```

   (The ownership loop and `transfer`/`interac` checks below this stay unchanged — for `kind == "loan"`, `from_account`/`to_account` are both `None`, so the existing `for acct in (...)` loop is already a no-op.)

5. In the `PendingAction(...)` construction near the end of `propose()`, add the two new fields:

```python
        pa = PendingAction(id=pid, customer_id=customer_id, kind=kind, amount=str(a),
                           from_account=from_account, to_account=to_account, memo=memo,
                           payee_email=payee_email,
                           security_question=security_question,
                           security_answer=security_answer,
                           created_at=now, expires_at=now + self.ttl,
                           interest_rate=str(Decimal(str(interest_rate))) if kind == "loan" else None,
                           amortization_months=int(amortization_months) if kind == "loan" else None)
```

6. In `execute()`, fix the cap check to be kind-aware (it currently only checks `self.max`, which would wrongly reject an already-approved loan proposal at confirm time) and add the `loan` branch:

```python
        if self.now() > pa.expires_at:
            self._audit(customer_id, pa.kind, Decimal(pa.amount), "expired", "")
            raise ActError("action expired")
        cap = self.loan_max_principal if pa.kind == "loan" else self.max
        if Decimal(pa.amount) > cap:
            raise ActError("over cap")
        try:
            if pa.kind == "transfer":
                res = self.bank.transfer(token, pa.from_account, pa.to_account, pa.amount,
                                         memo=pa.memo, idempotency_key=pa.id)
            elif pa.kind == "deposit":
                res = self.bank.deposit(token, pa.to_account, pa.amount, idempotency_key=pa.id)
            elif pa.kind == "interac":
                res = self.bank.send_etransfer(
                    token, pa.from_account, pa.amount,
                    recipient_handle_value=pa.payee_email,
                    security_question=pa.security_question,
                    security_answer=pa.security_answer,
                    memo=pa.memo, idempotency_key=pa.id)
            elif pa.kind == "loan":
                # nano-bank's lending is servicing-only (no underwriting step to
                # wait on) -- apply and disburse as one confirmed customer action.
                # NOTE: POST /api/v1/loans has no idempotency-key support
                # server-side (unlike transfers), so a raw transport retry of
                # this execute() between apply and disburse could in principle
                # create a second loan. Out of scope for this demo/servicing-only
                # product -- would need an API change in loans.rs to fix.
                applied = self.bank.apply_for_loan(token, pa.amount, pa.interest_rate,
                                                   pa.amortization_months)
                loan_id = applied.get("loan_id")
                disbursed = self.bank.disburse_loan(token, loan_id) if loan_id else None
                res = {"loan": applied, "disbursement": disbursed}
            else:
                res = self.bank.withdraw(token, pa.from_account, pa.amount, idempotency_key=pa.id)
```

7. Add a `loan` branch to `_summary()`, before the final `return f"Withdraw ..."`:

```python
    def _summary(self, pa: PendingAction) -> str:
        if pa.kind == "transfer":
            return f"Transfer {pa.amount} from {pa.from_account} to {pa.to_account}" + \
                   (f" ({pa.memo})" if pa.memo else "")
        if pa.kind == "deposit":
            return f"Deposit {pa.amount} into {pa.to_account}"
        if pa.kind == "interac":
            return f"Interac e-Transfer {pa.amount} from {pa.from_account} to {pa.payee_email}" + \
                   (f" ({pa.memo})" if pa.memo else "")
        if pa.kind == "loan":
            return (f"Apply for a car loan: principal {pa.amount} over "
                    f"{pa.amortization_months} months at {pa.interest_rate} APR")
        return f"Withdraw {pa.amount} from {pa.from_account}"
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `agent/.venv/bin/python -m pytest agent/tests/test_actions.py -v`
Expected: PASS (all tests, including the pre-existing ones — confirm no regression)

- [ ] **Step 5: Commit**

```bash
git add agent/actions.py agent/tests/test_actions.py
git commit -m "agent: add ActionStore 'loan' kind (propose/execute/summary), own cap"
```

---

### Task 5: MCP tools — `get_loans`, `propose_loan_application`

**Files:**
- Modify: `agent/mcp_server.py`
- Test: `agent/tests/test_mcp_binding.py` (or a new `agent/tests/test_loan_tools.py` — see below)

**Interfaces:**
- Consumes: `deps.db.loans(customer_id)` (Task 3), `_propose(kind, **kw)` (existing, in `build_mcp`), `ActionStore.__init__` now accepting `loan_max_principal` (Task 4).
- Produces: MCP tools `get_loans() -> list`, `propose_loan_application(principal_amount: str, interest_rate: str, amortization_months: int) -> dict`; both names added to `LLM_TOOL_NAMES`.

- [ ] **Step 1: Write the failing tests**

Create `agent/tests/test_loan_tools.py`:

```python
from agent.mcp_server import LLM_TOOL_NAMES


def test_loan_tools_are_in_llm_toolset():
    assert {"get_loans", "propose_loan_application"} <= LLM_TOOL_NAMES


def test_build_deps_wires_loan_max_principal(monkeypatch):
    # QdrantMemory/AuditLog.__init__ make a real network call to Qdrant at
    # construction time (collection_exists) -- stub their from_settings so this
    # test doesn't need a live Qdrant. build_deps never calls anything on
    # memory/audit itself, so plain stand-ins are enough.
    from decimal import Decimal
    from agent.config import Settings
    from agent import mcp_server as M

    monkeypatch.setattr(M.QdrantMemory, "from_settings", classmethod(lambda cls, s: object()))
    monkeypatch.setattr(M.AuditLog, "from_settings", classmethod(lambda cls, s: object()))

    s = Settings.from_env({"LOAN_MAX_PRINCIPAL": "75000"})
    deps = M.build_deps(s)
    assert deps.actions.loan_max_principal == Decimal("75000")
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `agent/.venv/bin/python -m pytest agent/tests/test_loan_tools.py -v`
Expected: FAIL — `get_loans` and `propose_loan_application` not in `LLM_TOOL_NAMES`

- [ ] **Step 3: Implement**

In `agent/mcp_server.py`:

1. Extend `LLM_TOOL_NAMES`:

```python
LLM_TOOL_NAMES = frozenset({
    "get_profile", "get_accounts", "get_transactions", "get_cards", "get_loans",
    "recall", "remember", "propose_transfer", "propose_deposit", "propose_withdraw",
    "register_interac_recipient", "list_interac_recipients",
    "remove_interac_recipient", "propose_interac_transfer", "open_account",
    "propose_loan_application"})
```

2. Add the two tools in `build_mcp`, after `get_cards`:

```python
    @mcp.tool()
    def get_loans() -> list:
        """The bound client's loans (principal, rate, term, status, next payment)."""
        return deps.db.loans(current_customer())
```

   and after `propose_interac_transfer` (before the confirm-only tools section):

```python
    @mcp.tool()
    def propose_loan_application(principal_amount: str, interest_rate: str,
                                 amortization_months: int) -> dict:
        """Propose a new loan (e.g. an auto loan) for the bound client: principal
        amount, annual interest_rate as a decimal fraction (e.g. '0.0799' for
        7.99%), and amortization_months. Requires confirmation; the client's own
        confirm both applies for the loan and disburses it into their chequing
        account in one step."""
        return _propose("loan", amount=principal_amount, interest_rate=interest_rate,
                        amortization_months=amortization_months)
```

3. In `build_deps`, wire the new cap through:

```python
def build_deps(settings: Settings) -> Deps:
    db = ClientContext(settings.db)
    memory = QdrantMemory.from_settings(settings)
    audit = AuditLog.from_settings(settings)
    from .bank import BankClient
    bank = BankClient(settings.nano_bank_api)
    actions = ActionStore(db, bank, audit,
                          max_per_tx=settings.act_max_per_tx, ttl_s=settings.confirm_ttl_s,
                          loan_max_principal=settings.loan_max_principal)
    return Deps(db=db, memory=memory, audit=audit, actions=actions, bank=bank,
                cxo_url=settings.cxo_url)
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `agent/.venv/bin/python -m pytest agent/tests/test_loan_tools.py agent/tests/test_mcp_binding.py agent/tests/test_interac_tools.py -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add agent/mcp_server.py agent/tests/test_loan_tools.py
git commit -m "agent: add get_loans / propose_loan_application MCP tools"
```

---

### Task 6: `loan.md` skill

**Files:**
- Create: `agent/skills/loan.md`
- Modify: `agent/tests/test_skills.py`

**Interfaces:**
- Consumes: `SkillRegistry.from_dir` (existing, unchanged) — auto-discovers any `*.md` in `agent/skills/`.
- Produces: a `kind: product`, `product: loan` skill named `loan`.

- [ ] **Step 1: Write the failing test change**

In `agent/tests/test_skills.py`, update `test_seed_skills_load_from_repo` (this MUST change in the same commit as the new skill file, or it breaks):

```python
def test_seed_skills_load_from_repo():
    from pathlib import Path
    reg = SkillRegistry.from_dir(Path(__file__).resolve().parents[1] / "skills")
    names = {s.name for s in reg.all()}
    assert {"chequing", "savings", "credit_card", "loan", "personal-finance", "investment"} <= names
    prod = {s.name: s.product for s in reg.all() if s.kind == "product"}
    assert prod == {"chequing": "chequing", "savings": "savings",
                    "credit_card": "credit_card", "loan": "loan"}
    assert reg.get("investment").kind == "advisory"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `agent/.venv/bin/python -m pytest agent/tests/test_skills.py -v`
Expected: FAIL — `prod` dict doesn't yet have `"loan"` (skill file doesn't exist), so the `==` comparison fails once you've made this edit (there's no `loan.md` yet, so this correctly documents the target shape; it's the create-the-file step below that makes it pass)

- [ ] **Step 3: Create the skill file**

Create `agent/skills/loan.md`:

```markdown
---
name: loan
description: Explaining loan terms and originating a loan (e.g. a car purchase) — rates, term, affordability, confirm-gated application.
kind: product
product: loan
---
Call get_loans() first. If the client already holds a loan, report its
principal, rate, term, status and next_payment_date instead of proposing a
new one — do not double up.

If the client is asking about buying a car and holds no loan: nano-bank's
lending is servicing-only — there is no separate underwriting/quote step, so
YOU pick the rate and term to propose, informed by these illustrative
guardrails: annual rate 6.99%-9.99% APR, amortization 36-84 months (default
60 unless the client states a preference). Estimate the monthly payment
using the standard amortization formula — PMT = P * [r(1+r)^n] / [(1+r)^n - 1],
where r = annual_rate / 12 and n = amortization_months — and state the
principal, rate, term and estimated monthly payment BEFORE proposing
anything.

Once the client confirms they want to proceed, call propose_loan_application
with that principal/rate/months. Proposing does NOT create the loan — as
with every propose_* tool, only the client's own confirmation in the app
executes it (applies for the loan AND disburses it into their chequing
account in one step). Say so plainly: "I've proposed the loan — you'll need
to confirm it yourself before any money moves."
```

- [ ] **Step 4: Run test to verify it passes**

Run: `agent/.venv/bin/python -m pytest agent/tests/test_skills.py -v`
Expected: PASS

- [ ] **Step 5: Run the full agent test suite (regression check for Part A)**

Run: `agent/.venv/bin/python -m pytest agent -q`
Expected: PASS, no failures (the `--run-live` marked tests are skipped by default per `conftest.py`)

- [ ] **Step 6: Commit**

```bash
git add agent/skills/loan.md agent/tests/test_skills.py
git commit -m "agent: add loan.md skill (car-loan guidance, rate/term bands)"
```

---

## Part B — Demo 4 visual revamp

### Task 7: `present/state.py` + tests

**Files:**
- Create: `demos/04-external-agent/present/state.py`
- Create: `demos/04-external-agent/present/tests/__init__.py` (empty)
- Create: `demos/04-external-agent/present/tests/test_state.py`

**Interfaces:**
- Produces: `read_jsonl(text: str) -> list[dict]`, `save_recording(dir_: str, events: list[dict]) -> str`, `load_recording(path: str) -> dict` (returns `{"events": [...], "captured_at": "..."}`), `latest_recording(dir_: str) -> str | None`, `decision_style(decision: str) -> tuple[str, str]` (label, hex color).

- [ ] **Step 1: Write the failing tests**

Create `demos/04-external-agent/present/tests/test_state.py`:

```python
import os, sys
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import state  # noqa: E402


def test_read_jsonl_skips_blank_and_partial_lines():
    text = '{"kind": "plan"}\n\n{"kind": "result"}\n{bad partial'
    rows = state.read_jsonl(text)
    assert rows == [{"kind": "plan"}, {"kind": "result"}]


def test_save_and_load_recording_roundtrip(tmp_path):
    events = [{"kind": "plan", "instruction": "do the thing"}]
    path = state.save_recording(str(tmp_path), events)
    assert state.latest_recording(str(tmp_path)) == path
    loaded = state.load_recording(path)
    assert loaded["events"] == events
    assert "captured_at" in loaded


def test_latest_recording_none_when_empty(tmp_path):
    assert state.latest_recording(str(tmp_path)) is None


def test_decision_style_known_and_unknown():
    label, color = state.decision_style("allow")
    assert "ALLOW" in label and color.startswith("#")
    label2, _ = state.decision_style("weird")
    assert label2 == "WEIRD"
```

Create empty `demos/04-external-agent/present/tests/__init__.py`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd demos/04-external-agent/present && python3 -m pytest tests/test_state.py -v` (any Python 3 with stdlib only is fine — this module has no third-party deps)
Expected: FAIL — `ModuleNotFoundError: No module named 'state'`

- [ ] **Step 3: Implement**

Create `demos/04-external-agent/present/state.py`:

```python
"""Pure state helpers for the demo-4 gateway console: parse the JSONL event
stream, save/load recordings, and style a gateway decision. No Streamlit
here so it stays unit-testable."""
from __future__ import annotations
import glob
import json
import os
from datetime import datetime, timezone


def read_jsonl(text: str) -> list[dict]:
    out = []
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            out.append(json.loads(line))
        except json.JSONDecodeError:
            continue  # partial trailing line mid-write
    return out


def save_recording(dir_: str, events: list[dict]) -> str:
    os.makedirs(dir_, exist_ok=True)
    ts = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S_%fZ")
    path = os.path.join(dir_, f"{ts}.json")
    with open(path, "w", encoding="utf-8") as f:
        json.dump({"events": events, "captured_at": ts}, f, indent=2)
    return path


def load_recording(path: str) -> dict:
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def latest_recording(dir_: str) -> str | None:
    files = sorted(glob.glob(os.path.join(dir_, "*.json")), key=os.path.getmtime)
    return files[-1] if files else None


_DECISION_STYLE = {
    "allow": ("ALLOW", "#1a7f37"),
    "deny": ("DENY", "#cf222e"),
    "pending_approval": ("PENDING APPROVAL", "#9a6700"),
}


def decision_style(decision: str) -> tuple[str, str]:
    return _DECISION_STYLE.get(decision, (decision.upper(), "#57606a"))
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd demos/04-external-agent/present && python3 -m pytest tests/test_state.py -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /home/bmartins/dev/nano-bank
git add demos/04-external-agent/present/state.py demos/04-external-agent/present/tests/
git commit -m "demos/04: add present/state.py (recording I/O + decision styling) + tests"
```

---

### Task 8: `present/capture.py` — headless single-run capture

**Files:**
- Create: `demos/04-external-agent/present/capture.py`

**Interfaces:**
- Consumes: `agent.external_agent.agent.ExternalAgent`, `agent.external_agent.agent.GatewayHTTP` (existing, unchanged), `agent.model_factory.init_models`/`llm`, `agent.config.Settings.from_env`.
- Produces: a CLI that writes one recording's raw events to a JSONL file at `--emit-jsonl PATH`. Consumed by Task 12 (`gateway_server.py`).

This task has no meaningful unit test on its own (it's a thin CLI wrapper around already-tested pieces — `ExternalAgent` has its own tests in `agent/tests/test_external_agent.py`); it's verified manually against the live stack in Task 13's final check.

- [ ] **Step 1: Create the script**

Create `demos/04-external-agent/present/capture.py`:

```python
#!/usr/bin/env python3
"""Capture one live external-agent run as a JSONL recording, for
gateway_server.py's headless "Capture live" button and build_gateway.py.

Demo 4 has no run-demo.sh/drive.py like the officer demos (05-10) -- app.py
runs the agent in-process directly, which works fine interactively but not
from a headless server with no Streamlit session. This script is that
driver, adapted to ExternalAgent's own shape (plan -> act* -> message* ->
result), not a multi-beat board consult.

One run here is a handful of HTTP calls and returns in well under a second
-- unlike the officer demos' multi-minute debates, events land in the JSONL
file in one batch at the end, not progressively.

    python demos/04-external-agent/present/capture.py --emit-jsonl /tmp/x.jsonl
    python demos/04-external-agent/present/capture.py --emit-jsonl /tmp/x.jsonl \
        --no-seed --instruction "Pay my $50 Epcor utility bill..."
"""
from __future__ import annotations
import argparse
import json
import os
import sys

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..")))

import requests
from agent.external_agent.agent import ExternalAgent, GatewayHTTP

DEFAULT_INSTRUCTION = (
    "Pay my $50 Epcor utility bill and tell me what a loan would look like "
    "if I want to buy a $28,000 car."
)


def _llm():
    from agent import model_factory as mf
    from agent.config import Settings
    s = Settings.from_env()
    mf.init_models(s)
    return mf.llm("fast", temperature=0.0)


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--emit-jsonl", required=True)
    p.add_argument("--instruction", default=DEFAULT_INSTRUCTION)
    p.add_argument("--no-seed", action="store_true",
                   help="reuse an already-seeded mandate instead of re-seeding")
    args = p.parse_args()

    base = os.environ.get("DEMO_BRANCH_BASE", "http://localhost:8086").rstrip("/")
    token = os.environ.get("AGENT_GATEWAY_TOKEN", "")
    hdr = {"Authorization": f"Bearer {token}"}

    if not args.no_seed:
        requests.post(f"{base}/agent-gateway/demo-seed", headers=hdr, timeout=30)

    agent = ExternalAgent(gateway=GatewayHTTP(base, token), llm=_llm())
    events = agent.run(args.instruction)

    with open(args.emit_jsonl, "w", encoding="utf-8") as f:
        for e in events:
            f.write(json.dumps(e) + "\n")
            f.flush()

    print(f"captured {len(events)} events -> {args.emit_jsonl}")


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Sanity-check the script parses and imports (no live stack needed for this check)**

Run: `agent/.venv/bin/python -c "import ast; ast.parse(open('demos/04-external-agent/present/capture.py').read())"`
Expected: no output (valid syntax) — the actual live capture is verified in Task 13.

- [ ] **Step 3: Commit**

```bash
git add demos/04-external-agent/present/capture.py
git commit -m "demos/04: add present/capture.py (headless single-run capture for gateway_server.py)"
```

---

### Task 9: Restyle `app.py` in place (two-tone nav+centre stepper, saves recordings)

**Files:**
- Modify: `demos/04-external-agent/app.py` (full rewrite of the transcript-rendering section; mandate/seed/revoke logic is preserved)

**Interfaces:**
- Consumes: `demos/04-external-agent/present/state.py` (Task 7) — imported via `sys.path.insert`.
- Produces: the same page at the same path (`./nb up --demo 04-external-agent` keeps working unmodified), now saving each run via `state.save_recording` so `present/gateway.html` (Task 10-12) can replay it.

- [ ] **Step 1: Replace the file**

Replace the full contents of `demos/04-external-agent/app.py` with:

```python
"""External mandated agent console.

An autonomous LLM agent operates a customer's bank ONLY through the agentic
branch's /agent-gateway/*, under a customer-granted mandate (scoped, capped,
revocable). It never sees the bank. Seed a demo mandate, give a high-level
instruction, and watch the agent plan -> act (mandate-gated) -> ask the
manager, rendered as a two-tone stepper (external agent vs. personal
manager). Each run is also saved as a recording under present/recordings/
so present/gateway.html can replay it as a standalone animated page.

Config: DEMO_BRANCH_BASE (default http://localhost:8086) + AGENT_GATEWAY_TOKEN.
The demo builds the planner LLM locally (needs OLLAMA_API_KEY).
"""
from __future__ import annotations
import os
import sys

import requests
import streamlit as st

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "present"))
import state  # noqa: E402

from agent.external_agent.agent import ExternalAgent, GatewayHTTP

BASE = os.environ.get("DEMO_BRANCH_BASE", "http://localhost:8086").rstrip("/")
TOKEN = os.environ.get("AGENT_GATEWAY_TOKEN", "")
HDR = {"Authorization": f"Bearer {TOKEN}"}
RECORDINGS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "present", "recordings")
DEFAULT_INSTRUCTION = (
    "Pay my $50 Epcor utility bill and tell me what a loan would look like "
    "if I want to buy a $28,000 car."
)

st.set_page_config(page_title="nano-bank · external agent", layout="wide")
ss = st.session_state
ss.setdefault("events", [])
ss.setdefault("selected", 0)
ss.setdefault("instr", DEFAULT_INSTRUCTION)
ss.setdefault("primed", False)

if not ss.primed and not ss.events:
    latest = state.latest_recording(RECORDINGS)
    if latest:
        try:
            ss.events = state.load_recording(latest)["events"]
        except (OSError, ValueError, KeyError):
            pass
    ss.primed = True

st.title("🛰️ nano-bank — external mandated agent")
st.caption(f"Gateway: `{BASE}/agent-gateway` · the agent's ONLY door — mandate-gated, capped, revocable")


@st.cache_resource(show_spinner=False)
def _llm():
    from agent import model_factory as mf
    from agent.config import Settings
    s = Settings.from_env()
    mf.init_models(s)
    return mf.llm("fast", temperature=0.0)


def _gw_post(path):
    return requests.post(f"{BASE}{path}", headers=HDR, timeout=180)


# --- mandate panel ----------------------------------------------------------
top = st.columns([3, 1, 1])
with top[0]:
    r = requests.get(f"{BASE}/agent-gateway/mandate", headers=HDR, timeout=30)
    if r.status_code == 200:
        m = r.json()
        st.success(f"**Mandate active** · account `{m.get('account_id','')[:8]}` "
                   f"({m.get('account_type','')}) · scopes: {', '.join(m.get('scopes', []))} "
                   f"· cap/tx: ${m.get('max_per_tx','—')} · expires {m.get('expires_at','')[:19]}")
    else:
        st.warning("No active mandate — click **Seed mandate** to register an agent + grant consent.")
with top[1]:
    if st.button("🌱 Seed mandate"):
        _gw_post("/agent-gateway/demo-seed")
        ss.events, ss.selected = [], 0
        st.rerun()
with top[2]:
    if st.button("⛔ Revoke"):
        _gw_post("/agent-gateway/revoke")
        st.rerun()

st.divider()

# --- instruction + run -------------------------------------------------------
ss.instr = st.text_area("High-level instruction to the autonomous agent", ss.instr, height=70)
c1, c2 = st.columns([1, 1])
if c1.button("▶ Run agent", type="primary"):
    try:
        agent = ExternalAgent(gateway=GatewayHTTP(BASE, TOKEN), llm=_llm())
        with st.spinner("agent planning + acting through the gateway…"):
            ss.events = agent.run(ss.instr)
        ss.selected = 0
        state.save_recording(RECORDINGS, ss.events)
    except Exception as e:  # noqa: BLE001
        st.error(f"agent run failed: {e}")
    st.rerun()
if c2.button("⏮ Replay last recording"):
    latest = state.latest_recording(RECORDINGS)
    if latest:
        ss.events = state.load_recording(latest)["events"]
        ss.selected = 0
    else:
        st.toast("No recording yet — run the agent once.")

_ICON = {"plan": "🧠", "act": "🤖", "message": "💬", "result": "✅"}


def _label(i: int, e: dict) -> str:
    kind = e["kind"]
    if kind == "act":
        return f"{_ICON[kind]} {i}. act · {e['operation']}"
    if kind == "message":
        return f"{_ICON[kind]} {i}. ask the manager"
    if kind == "plan":
        return f"{_ICON[kind]} {i}. plan"
    return f"{_ICON[kind]} {i}. done"


def _event_card(e: dict) -> None:
    kind = e["kind"]
    if kind == "plan":
        st.markdown("#### 🧠 Agent plan")
        st.caption(e["instruction"])
        return
    if kind == "act":
        left, right = st.columns(2)
        with left, st.container(border=True):
            st.markdown("🛰️ **External agent → act**")
            st.markdown(f"`{e['operation']}` {e.get('params', {})}")
        res = e.get("result", {})
        dec = res.get("decision", "?")
        label, color = state.decision_style(dec)
        with right, st.container(border=True):
            st.markdown("🏦 **Gateway → mandate check**")
            st.markdown(
                f"<span style='background:{color};color:white;padding:2px 10px;"
                f"border-radius:10px;font-weight:700'>{label}</span>", unsafe_allow_html=True)
            if dec == "pending_approval":
                st.info(f"⏸ over the daily cap — parked for the customer to approve "
                        f"(approval `{str(res.get('approval_id'))[:8]}`). Not paid yet.")
            else:
                st.write(res.get("reason") or (res.get("result") if dec == "allow" else res))
        return
    if kind == "message":
        left, right = st.columns(2)
        with left, st.container(border=True):
            st.markdown("🛰️ **External agent → asks the manager**")
            st.write(e.get("text", ""))
        with right, st.container(border=True):
            st.markdown("🏦 **Personal manager**")
            st.write(e.get("answer", ""))
            trace = e.get("trace")
            if trace:
                st.caption("trace: " + "  ·  ".join(
                    f"{'🔧' if t['kind'] == 'tool' else '🧠'}{'✅' if t.get('ok') else '❌'} "
                    f"{t['name']} {t['elapsed_ms']}ms" for t in trace))
        return
    st.success(f"✅ done — {e['steps']} step(s). Try **Revoke** then **Run agent** again: "
               "the next act is denied at the gateway.")


# --- stepper: nav + centre ---------------------------------------------------
st.divider()
nav, centre = st.columns([1.6, 5])
with nav:
    st.subheader("Run")
    st.caption("Click a step to show it.")
    for i, e in enumerate(ss.events):
        sel = "▶ " if ss.selected == i else ""
        if st.button(f"{sel}{_label(i, e)}", key=f"ev-{i}", use_container_width=True):
            ss.selected = i
    if not ss.events:
        st.info("No run yet. Click ▶ Run agent.")
with centre:
    if ss.events:
        idx = min(ss.selected, len(ss.events) - 1)
        _event_card(ss.events[idx])
```

- [ ] **Step 2: Smoke-check it imports cleanly**

Run: `agent/.venv/bin/python -c "import ast; ast.parse(open('demos/04-external-agent/app.py').read())"`
Expected: no output (valid syntax)

- [ ] **Step 3: Manual verification against the live stack**

This step needs the deployed stack (`./nb up --demo 04-external-agent` or the manual port-forward + env vars from `demos/README.md`). Run it, click **Seed mandate**, then **▶ Run agent**, and confirm: the nav lists `plan`, `act · transfer_out`, `ask the manager`, `done`; clicking each shows the two-tone card; `demos/04-external-agent/present/recordings/` now contains a new `*.json` file. This is a manual check — no automated test asserts against the live bank/gateway/manager stack (consistent with how `demos/04-external-agent`'s original `app.py` had no automated UI test either; `agent/tests/test_external_agent.py` and `agent/tests/test_agent_gateway_api.py` already cover the underlying logic with fakes).

- [ ] **Step 4: Commit**

```bash
git add demos/04-external-agent/app.py
git commit -m "demos/04: restyle app.py into a two-tone nav+centre stepper, save recordings"
```

---

### Task 10: `present/gateway.template.html` — the split-screen cinematic

**Files:**
- Create: `demos/04-external-agent/present/gateway.template.html`

**Interfaces:**
- Consumes: a JS array `EVENTS` inlined by Task 11's `build_gateway.py` at the marker `/*__EVENTS__*/` — the raw `events` list a recording carries (same shape `app.py` renders: `{kind: "plan"|"act"|"message"|"result", ...}`).
- Produces: a self-contained static HTML page with no external JS/CSS dependencies besides Google Fonts.

- [ ] **Step 1: Create the template**

Create `demos/04-external-agent/present/gateway.template.html`:

```html
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>nano-bank · external agent gateway</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Geist:wght@400;500;600;700;800&family=Geist+Mono:wght@500;600;700&display=swap" rel="stylesheet">
<style>
  :root{
    --ink:#023047; --edge:rgba(255,255,255,.18); --line:rgba(255,255,255,.12);
    --text:#ffffff; --muted:#cbd5e1; --faint:#8ea3b8;
    /* bank/manager side -- same brand as the C-suite boardroom */
    --bank:#219ebc; --bank2:#ffb703;
    /* external-agent side -- deliberately foreign, not nano-bank brand */
    --ext:#8338ec;
    --allow:#2a9d3f; --deny:#e5383b; --pending:#f4a300;
    --disp:'Geist',system-ui,sans-serif; --mono:'Geist Mono',ui-monospace,monospace;
  }
  *{box-sizing:border-box}
  html,body{margin:0;height:100%}
  body{
    background:
      radial-gradient(ellipse 60% 50% at 50% 0%, rgba(12,43,62,.6) 0%, transparent 70%),
      linear-gradient(to right, #0c2b3e 1px, transparent 1px),
      linear-gradient(to bottom, #0c2b3e 1px, transparent 1px),
      var(--ink);
    background-size: auto, 4rem 4rem, 4rem 4rem, auto;
    color:var(--text); font-family:var(--disp);
    -webkit-font-smoothing:antialiased; overflow:hidden;
  }
  .wrap{display:flex;flex-direction:column;height:100vh;max-width:1500px;margin:0 auto;padding:0 clamp(12px,3vw,32px)}

  .top{display:flex;align-items:center;gap:18px;padding:16px 4px 12px;border-bottom:1px solid var(--line)}
  .live{display:flex;align-items:center;gap:8px;font-family:var(--mono);font-size:12px;letter-spacing:.14em;color:#ff5d6c;font-weight:600}
  .live .dot{width:9px;height:9px;border-radius:50%;background:#ff5d6c;animation:pulse 1.8s infinite}
  @keyframes pulse{0%{box-shadow:0 0 0 0 rgba(255,93,108,.6)}70%{box-shadow:0 0 0 10px rgba(255,93,108,0)}100%{box-shadow:0 0 0 0 rgba(255,93,108,0)}}
  .brand{font-family:var(--disp);font-weight:700;letter-spacing:.02em;font-size:clamp(15px,1.5vw,19px)}
  .brand b{color:var(--bank2)}
  .capbtn{margin-left:auto;font-family:var(--mono);font-size:11px;letter-spacing:.07em;color:var(--bank2);background:transparent;border:1px solid var(--edge);
    border-radius:999px;padding:7px 14px;cursor:pointer;text-transform:uppercase;transition:.15s}
  .capbtn:hover{background:rgba(255,255,255,.08)} .capbtn:disabled{opacity:.45;cursor:default}
  .clock{font-family:var(--mono);font-size:13px;color:var(--muted);min-width:52px;text-align:right}
  .capstatus{position:fixed;top:16px;left:50%;transform:translateX(-50%);z-index:60;background:rgba(5,42,64,.9);backdrop-filter:blur(12px);border:1px solid var(--edge);
    border-radius:12px;padding:12px 18px;display:none;align-items:center;gap:12px;box-shadow:0 20px 50px rgba(0,0,0,.5);
    font-family:var(--mono);font-size:12px;color:var(--text);max-width:80vw}
  .capstatus.show{display:flex}
  .capstatus .sp{width:14px;height:14px;border:2px solid var(--edge);border-top-color:var(--bank2);border-radius:50%;animation:spin .8s linear infinite;flex:none}
  @keyframes spin{to{transform:rotate(360deg)}}

  .agenda{display:flex;align-items:baseline;gap:12px;padding:10px 4px;border-bottom:1px solid var(--line)}
  .agenda .eyebrow{font-family:var(--mono);font-size:11px;letter-spacing:.16em;color:var(--faint);text-transform:uppercase}
  .agenda .q{font-family:var(--disp);font-weight:600;font-size:clamp(14px,1.5vw,18px);color:var(--text);line-height:1.25;
    overflow:hidden;text-overflow:ellipsis;white-space:nowrap}

  .stage{flex:1 1 auto;min-height:320px;display:grid;grid-template-columns:1fr 220px 1fr;gap:0;align-items:stretch;padding:18px 0;position:relative}
  .side{border-radius:18px;padding:20px;display:flex;flex-direction:column;gap:14px;border:1px solid var(--edge);
    background:linear-gradient(180deg,rgba(255,255,255,.06),rgba(255,255,255,.02));transition:.35s;position:relative;overflow:hidden}
  .side.ext{margin-right:14px;--ac:var(--ext)} .side.bank{margin-left:14px;--ac:var(--bank)}
  .side .head{display:flex;align-items:center;gap:10px}
  .side .avatar{width:52px;height:52px;border-radius:16px;display:flex;align-items:center;justify-content:center;font-size:26px;
    background:linear-gradient(180deg,rgba(255,255,255,.14),rgba(255,255,255,.04));border:1px solid var(--edge)}
  .side .who{font-family:var(--disp);font-weight:700;font-size:16px;letter-spacing:.02em}
  .side .role{font-family:var(--mono);font-size:10px;letter-spacing:.1em;color:var(--faint);text-transform:uppercase}
  .side.ext .avatar,.side.ext .who{color:var(--ext)}
  .side.bank .avatar,.side.bank .who{color:var(--bank)}
  .side.active{box-shadow:0 0 0 1px var(--ac),0 0 40px -8px var(--ac);border-color:var(--ac)}
  .side.dim{opacity:.55;filter:saturate(.7)}

  .bubble{flex:1 1 auto;background:#f2f9fc;color:var(--ink);border-radius:16px;padding:18px 20px;
    font-family:var(--disp);font-size:16px;line-height:1.5;overflow:auto;min-height:1.4em;
    box-shadow:0 14px 34px rgba(0,0,0,.35);border:1px solid #fff}
  .bubble .hdr{font-family:var(--mono);font-size:11px;letter-spacing:.1em;text-transform:uppercase;color:#556;margin-bottom:8px;font-weight:700}
  .side.ext .bubble .hdr{color:var(--ext)} .side.bank .bubble .hdr{color:var(--bank)}
  .bubble .txt{white-space:pre-wrap;word-break:break-word;min-height:1.2em}

  .rail{display:flex;flex-direction:column;align-items:center;justify-content:center;gap:10px}
  .rail .conduit{width:2px;flex:1 1 auto;background:linear-gradient(var(--ext),var(--bank));opacity:.35;min-height:40px}
  .rail .pill{font-family:var(--mono);font-size:11px;letter-spacing:.08em;font-weight:700;padding:8px 14px;border-radius:999px;
    background:rgba(255,255,255,.08);border:1px solid var(--edge);color:var(--muted);text-transform:uppercase;text-align:center;
    transition:.3s;opacity:0;transform:scale(.85)}
  .rail .pill.show{opacity:1;transform:scale(1)}
  .rail .pill.allow{background:var(--allow);color:#fff;border-color:transparent;box-shadow:0 0 24px -4px var(--allow)}
  .rail .pill.deny{background:var(--deny);color:#fff;border-color:transparent;box-shadow:0 0 24px -4px var(--deny)}
  .rail .pill.pending{background:var(--pending);color:#231a00;border-color:transparent;box-shadow:0 0 24px -4px var(--pending)}
  .rail .arrow{font-size:22px;color:var(--faint);opacity:0;transition:.3s}
  .rail .arrow.show{opacity:1}
  .rail .gwlabel{font-family:var(--mono);font-size:10px;letter-spacing:.14em;color:var(--faint);text-transform:uppercase}

  .banner{position:absolute;inset:18px 0;display:flex;align-items:center;justify-content:center;text-align:center;
    background:rgba(2,48,71,.85);backdrop-filter:blur(6px);border-radius:18px;opacity:0;pointer-events:none;transition:.4s;
    font-family:var(--disp);font-size:clamp(16px,2vw,22px);font-weight:700;padding:24px;z-index:5}
  .banner.show{opacity:1}

  .transport{display:flex;align-items:center;gap:16px;padding:12px 4px 18px;border-top:1px solid var(--line)}
  .tbtn{background:rgba(255,255,255,.06);color:var(--text);border:1px solid var(--edge);border-radius:12px;height:44px;min-width:44px;padding:0 14px;
    font-size:16px;cursor:pointer;display:flex;align-items:center;justify-content:center;gap:8px;transition:.15s}
  .tbtn:hover{border-color:var(--bank)}
  .tbtn.play{background:linear-gradient(90deg,var(--ext),var(--bank),var(--bank2));background-size:200% auto;color:#031420;
    border-color:transparent;font-weight:700;min-width:130px;font-family:var(--disp);box-shadow:0 0 20px rgba(33,158,188,.35)}
  .tbtn.play:hover{background-position:right}
  .progress{flex:1;height:8px;background:rgba(255,255,255,.08);border-radius:8px;overflow:hidden;cursor:pointer;border:1px solid var(--line)}
  .progress .fill{height:100%;width:0;background:linear-gradient(90deg,var(--ext),var(--bank),var(--bank2));transition:width .3s linear}
  .meta{font-family:var(--mono);font-size:12px;color:var(--muted);min-width:190px;text-align:right}
  .meta b{color:var(--text)}
  .speed{display:flex;align-items:center;gap:8px;font-family:var(--mono);font-size:11px;color:var(--faint)}
  .speed input{accent-color:var(--bank2)}

  @media (max-width:900px){
    .stage{grid-template-columns:1fr;gap:14px}
    .side.ext,.side.bank{margin:0}
    .rail{flex-direction:row;padding:6px 0}
    .rail .conduit{width:auto;height:2px;flex:1 1 auto;background:linear-gradient(90deg,var(--ext),var(--bank))}
    .meta{display:none}
  }
  @media (prefers-reduced-motion:reduce){ *{animation:none!important;transition:none!important} }
</style>
</head>
<body>
<div class="wrap">
  <div class="top">
    <span class="live"><span class="dot"></span>LIVE</span>
    <span class="brand">nano-bank · <b>GATEWAY</b> — external mandated agent</span>
    <button class="capbtn" id="captureBtn" title="Record a fresh run live against the deployed gateway, then play it">⦿ Capture live</button>
    <span class="clock" id="clock">00:00</span>
  </div>
  <div class="capstatus" id="capStatus"><span class="sp"></span><span id="capMsg">Starting…</span></div>

  <div class="agenda">
    <span class="eyebrow" id="eyebrow">STEP</span>
    <span class="q" id="agendaQ">—</span>
  </div>

  <div class="stage" id="stage">
    <div class="side ext" id="sideExt">
      <div class="head"><div class="avatar">🛰️</div><div><div class="who">External Agent</div><div class="role">customer-mandated · outside the bank</div></div></div>
      <div class="bubble"><div class="hdr" id="extHdr">—</div><div class="txt" id="extTxt">Press play to run the agent.</div></div>
    </div>

    <div class="rail">
      <div class="gwlabel">Gateway</div>
      <div class="conduit"></div>
      <div class="arrow" id="arrow">→</div>
      <div class="pill" id="pill">—</div>
      <div class="conduit"></div>
    </div>

    <div class="side bank" id="sideBank">
      <div class="head"><div class="avatar">🏦</div><div><div class="who">Personal Manager</div><div class="role">nano-bank · A2A over the branch</div></div></div>
      <div class="bubble"><div class="hdr" id="bankHdr">—</div><div class="txt" id="bankTxt">Waiting…</div></div>
    </div>

    <div class="banner" id="banner"></div>
  </div>

  <div class="transport">
    <button class="tbtn" id="prev" title="Previous step">⏮</button>
    <button class="tbtn play" id="play">▶ Run</button>
    <button class="tbtn" id="next" title="Next step">⏭</button>
    <div class="progress" id="progress"><div class="fill" id="fill"></div></div>
    <div class="speed">SPEED <input id="speed" type="range" min="0.5" max="2" step="0.25" value="1"><span id="speedv">1.0×</span></div>
    <div class="meta" id="meta">—</div>
  </div>
</div>

<script>
const EVENTS = /*__EVENTS__*/ [];

function clean(md){
  return (md||'').replace(/`{1,3}/g,'').replace(/\*\*/g,'').replace(/[*_]/g,'')
    .replace(/^#{1,6}\s*/gm,'').replace(/^\s*[-•]\s*/gm,'• ').replace(/\n{3,}/g,'\n\n').trim();
}
function traceLine(trace){
  if(!trace || !trace.length) return '';
  return 'trace: ' + trace.map(t=>(t.kind==='tool'?'🔧':'🧠')+(t.ok?'✅':'❌')+' '+t.name+' '+t.elapsed_ms+'ms').join('  ·  ');
}

function eventParts(e){
  if(e.kind==='plan') return [{side:'ext', header:'🧠 AGENT PLAN', text:e.instruction||''}];
  if(e.kind==='result') return [{side:'mid', header:'✅ RUN COMPLETE',
    text:(e.steps||0)+' step(s) executed. Try Revoke then Run again — the next act is denied at the gateway.'}];
  if(e.kind==='act'){
    const res=e.result||{}, dec=res.decision||'unknown';
    let detail;
    if(dec==='pending_approval'){
      detail='⏸ over the daily cap — parked for the customer to approve (approval '+
             String(res.approval_id||'').slice(0,8)+'). Not paid yet.';
    } else if(dec==='allow'){
      detail=clean(typeof res.reason==='string' && res.reason ? res.reason
                   : (typeof res.result==='string' ? res.result : JSON.stringify(res.result||res)));
    } else {
      detail=clean(res.reason || JSON.stringify(res));
    }
    return [
      {side:'ext', header:'🛰️ EXTERNAL AGENT → ACT', text:(e.operation||'')+' '+JSON.stringify(e.params||{})},
      {side:'bank', header:'🏦 GATEWAY → MANDATE CHECK', text:detail, decision:dec},
    ];
  }
  if(e.kind==='message'){
    return [
      {side:'ext', header:'🛰️ EXTERNAL AGENT → ASKS THE MANAGER', text:e.text||''},
      {side:'bank', header:'🏦 PERSONAL MANAGER', text:clean(e.answer||'')+
        (traceLine(e.trace) ? '\n\n'+traceLine(e.trace) : '')},
    ];
  }
  return [];
}

const STEPS = [];
EVENTS.forEach((e,ei)=>{ eventParts(e).forEach(p=>STEPS.push({...p, ei, ekind:e.kind})); });

let idx=0, playing=false, timer=null, speed=1, clock=0, clockTimer=null, typeTimer=null;
const REDUCE = window.matchMedia && window.matchMedia('(prefers-reduced-motion:reduce)').matches;
const $=id=>document.getElementById(id);

function typeInto(el, text){
  clearInterval(typeTimer);
  if(REDUCE){ el.textContent=text; return; }
  el.textContent=''; let i=0; const total=text.length;
  const per=Math.max(9, Math.min(28, 18/speed));
  typeTimer=setInterval(()=>{
    i=Math.min(total, i + (total>240?2:1));
    el.textContent=text.slice(0,i);
    if(i>=total) clearInterval(typeTimer);
  }, per);
}
function dwell(step){
  const words=(step.text||'').split(/\s+/).length;
  return Math.min(6500, 2200 + words*120) / speed;
}

const DEC_LABEL={allow:'🟢 ALLOW', deny:'🔴 DENY', pending_approval:'🟡 PENDING APPROVAL'};
const DEC_CLASS={allow:'allow', deny:'deny', pending_approval:'pending'};

function render(){
  const s=STEPS[idx]; if(!s) return;
  $('eyebrow').textContent = 'EVENT '+(s.ei+1)+'/'+EVENTS.length+' · '+s.ekind.toUpperCase();
  $('agendaQ').textContent = s.header;
  $('meta').innerHTML = 'Step <b>'+(idx+1)+'</b>/'+STEPS.length;
  $('fill').style.width = (STEPS.length ? ((idx+1)/STEPS.length*100) : 0)+'%';

  $('banner').classList.remove('show');
  $('sideExt').classList.remove('active','dim');
  $('sideBank').classList.remove('active','dim');
  $('arrow').classList.remove('show');
  $('pill').className='pill';

  if(s.side==='mid'){
    $('sideExt').classList.add('dim'); $('sideBank').classList.add('dim');
    const b=$('banner'); b.textContent=s.text; b.classList.add('show');
  } else if(s.side==='ext'){
    $('sideExt').classList.add('active'); $('sideBank').classList.add('dim');
    $('extHdr').textContent=s.header;
    requestAnimationFrame(()=>typeInto($('extTxt'), s.text));
  } else { // bank
    $('sideBank').classList.add('active'); $('sideExt').classList.add('dim');
    $('bankHdr').textContent=s.header;
    requestAnimationFrame(()=>typeInto($('bankTxt'), s.text));
    if(s.decision){
      $('arrow').classList.add('show');
      const cls=DEC_CLASS[s.decision]||'';
      $('pill').textContent = DEC_LABEL[s.decision] || s.decision.toUpperCase();
      requestAnimationFrame(()=>$('pill').classList.add('show', cls));
    }
  }
}

function go(n){ idx=Math.max(0,Math.min(STEPS.length-1,n)); render(); }
function step(dir){ go(idx+dir); if(playing) schedule(); }
function schedule(){
  clearTimeout(timer);
  timer=setTimeout(()=>{
    if(idx>=STEPS.length-1){ stop(); return; }
    idx++; render(); schedule();
  }, dwell(STEPS[idx]));
}
function play(){ if(!STEPS.length) return; playing=true; $('play').innerHTML='⏸ Pause'; startClock(); render(); schedule(); }
function stop(){ playing=false; $('play').innerHTML='▶ Run'; clearTimeout(timer); stopClock(); }
function toggle(){ playing?stop():play(); }
function startClock(){ stopClock(); clockTimer=setInterval(()=>{clock++;renderClock();},1000); }
function stopClock(){ clearInterval(clockTimer); }
function renderClock(){ const m=String(Math.floor(clock/60)).padStart(2,'0'),s=String(clock%60).padStart(2,'0'); $('clock').textContent=m+':'+s; }

$('play').onclick=toggle;
$('next').onclick=()=>step(1);
$('prev').onclick=()=>step(-1);
$('speed').oninput=e=>{speed=parseFloat(e.target.value);$('speedv').textContent=speed.toFixed(1)+'×';if(playing)schedule();};
$('progress').onclick=e=>{const r=e.currentTarget.getBoundingClientRect();go(Math.round((e.clientX-r.left)/r.width*STEPS.length));};
document.addEventListener('keydown',e=>{
  if(e.code==='Space'){e.preventDefault();toggle();}
  if(e.code==='ArrowRight')step(1); if(e.code==='ArrowLeft')step(-1);
});

async function pollCapture(){
  let s;
  try{ s=await (await fetch('/api/capture/status')).json(); }
  catch(_){ $('capMsg').textContent='⚠ capture server not reachable'; $('captureBtn').disabled=false; return; }
  $('capMsg').textContent = s.message || 'Capturing…';
  if(s.running){ setTimeout(pollCapture, 1200); return; }
  if(s.ok){ $('capMsg').textContent='✓ '+s.message; setTimeout(()=>location.reload(), 900); }
  else{
    $('capMsg').textContent='⚠ '+(s.message||'capture failed');
    $('captureBtn').disabled=false;
    setTimeout(()=>$('capStatus').classList.remove('show'), 5000);
  }
}
if(location.protocol==='file:'){
  $('captureBtn').disabled=true;
  $('captureBtn').title='Serve via gateway_server.py to capture live';
}
$('captureBtn').onclick=async ()=>{
  if(!confirm('Record a fresh run live against the deployed gateway?\n\nThis drives the real external agent and personal manager.')) return;
  stop(); $('captureBtn').disabled=true;
  $('capStatus').classList.add('show'); $('capMsg').textContent='Starting…';
  try{
    const r=await fetch('/api/capture',{method:'POST'});
    if(r.status===409) $('capMsg').textContent='A capture is already running…';
  }catch(_){ $('capMsg').textContent='⚠ capture server not reachable (serve via gateway_server.py)'; $('captureBtn').disabled=false; return; }
  pollCapture();
};

if(STEPS.length){ render(); } else { $('agendaQ').textContent='No recording yet — capture one live.'; }
</script>
</body>
</html>
```

- [ ] **Step 2: Validate the marker exists exactly once and is well-formed**

Run:
```bash
grep -c "const EVENTS = /\*__EVENTS__\*/ \[\];" demos/04-external-agent/present/gateway.template.html
```
Expected: `1`

- [ ] **Step 3: Commit**

```bash
git add demos/04-external-agent/present/gateway.template.html
git commit -m "demos/04: add gateway.template.html (split-screen cinematic template)"
```

---

### Task 11: `present/build_gateway.py` — inline a recording into the template

**Files:**
- Create: `demos/04-external-agent/present/build_gateway.py`
- Test: `demos/04-external-agent/present/tests/test_build_gateway.py`

**Interfaces:**
- Consumes: `demos/04-external-agent/present/recordings/canonical.json` (if present — `{"events": [...], "captured_at": "..."}`, Task 7's `save_recording` shape), `gateway.template.html`'s `/*__EVENTS__*/` marker (Task 10).
- Produces: `build() -> str` (path to the written `gateway.html`), a CLI entry point.

- [ ] **Step 1: Write the failing test**

Create `demos/04-external-agent/present/tests/test_build_gateway.py`:

```python
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import build_gateway  # noqa: E402


def test_build_inlines_events_into_the_template(tmp_path, monkeypatch):
    template = tmp_path / "gateway.template.html"
    template.write_text("<html><script>\nconst EVENTS = /*__EVENTS__*/ [];\n</script></html>")
    rec_dir = tmp_path / "recordings"
    rec_dir.mkdir()
    (rec_dir / "canonical.json").write_text(json.dumps(
        {"events": [{"kind": "plan", "instruction": "hi"}], "captured_at": "t"}))
    out = tmp_path / "gateway.html"

    monkeypatch.setattr(build_gateway, "TEMPLATE", str(template))
    monkeypatch.setattr(build_gateway, "OUT", str(out))
    monkeypatch.setattr(build_gateway, "REC", str(rec_dir / "canonical.json"))

    result_path = build_gateway.build()
    assert result_path == str(out)
    html = out.read_text()
    assert '"kind": "plan"' in html or "'kind': 'plan'" in html or '"kind":"plan"' in html
    assert "hi" in html


def test_build_writes_empty_array_when_no_recording(tmp_path, monkeypatch):
    template = tmp_path / "gateway.template.html"
    template.write_text("<html><script>\nconst EVENTS = /*__EVENTS__*/ [];\n</script></html>")
    out = tmp_path / "gateway.html"

    monkeypatch.setattr(build_gateway, "TEMPLATE", str(template))
    monkeypatch.setattr(build_gateway, "OUT", str(out))
    monkeypatch.setattr(build_gateway, "REC", str(tmp_path / "recordings" / "canonical.json"))

    build_gateway.build()
    assert "const EVENTS = [];" in out.read_text()
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `python3 -m pytest demos/04-external-agent/present/tests/test_build_gateway.py -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'build_gateway'`

- [ ] **Step 3: Implement**

Create `demos/04-external-agent/present/build_gateway.py`:

```python
#!/usr/bin/env python3
"""Build the self-contained animated gateway page: inline the canonical
recording's events into gateway.template.html and write gateway.html.

    python demos/04-external-agent/present/build_gateway.py
"""
from __future__ import annotations
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
TEMPLATE = os.path.join(HERE, "gateway.template.html")
OUT = os.path.join(HERE, "gateway.html")
REC = os.path.join(HERE, "recordings", "canonical.json")
MARKER = "/*__EVENTS__*/"


def _load() -> list:
    if not os.path.exists(REC):
        return []
    with open(REC, encoding="utf-8") as f:
        return json.load(f).get("events", [])


def build() -> str:
    with open(TEMPLATE, encoding="utf-8") as f:
        html = f.read()
    payload = json.dumps(_load(), ensure_ascii=False)
    idx = html.index(MARKER)
    end = html.index(";", idx)
    html = html[:idx] + payload + html[end:]
    with open(OUT, "w", encoding="utf-8") as f:
        f.write(html)
    return OUT


if __name__ == "__main__":
    out = build()
    events = json.loads(open(out, encoding="utf-8").read()
                        .split("const EVENTS = ", 1)[1].split(";\n", 1)[0])
    print(f"  {len(events)} events")
    print("wrote", out)
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `python3 -m pytest demos/04-external-agent/present/tests/test_build_gateway.py -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add demos/04-external-agent/present/build_gateway.py demos/04-external-agent/present/tests/test_build_gateway.py
git commit -m "demos/04: add build_gateway.py (inline a recording into the cinematic template)"
```

---

### Task 12: `present/gateway_server.py` — serve + headless capture

**Files:**
- Create: `demos/04-external-agent/present/gateway_server.py`
- Create: `demos/04-external-agent/present/recordings/.gitignore`

**Interfaces:**
- Consumes: `demos/04-external-agent/present/capture.py` (Task 8, invoked as a subprocess), `state.save_recording`/`state.read_jsonl` (Task 7), `build_gateway.build` (Task 11, invoked as a subprocess via its `__main__`).
- Produces: an HTTP server exposing `GET /gateway.html` (static) and `POST /api/capture` / `GET /api/capture/status`.

This task's server-loop/threading logic isn't unit tested (it mirrors `demos/10-ceo/present/boardroom_server.py`, which also has no test — it's an operational script, verified by running it); Task 13 does the end-to-end manual check.

- [ ] **Step 1: Create the recordings `.gitignore`**

Create `demos/04-external-agent/present/recordings/.gitignore`:

```
# Ad-hoc live-run recordings are local; the canonical one is force-added.
*.json
!canonical.json
```

- [ ] **Step 2: Create the server**

Create `demos/04-external-agent/present/gateway_server.py`:

```python
#!/usr/bin/env python3
"""Serve the animated gateway cinematic AND capture new runs from the UI.

    demos/04-external-agent/present/gateway_server.py            # http://localhost:8521/gateway.html
    demos/04-external-agent/present/gateway_server.py 8531

Endpoints:
    POST /api/capture           -> starts a capture (one external-agent run)
    GET  /api/capture/status    -> {running, phase, ok, message}

Drives present/capture.py against $DEMO_BRANCH_BASE (default
http://localhost:8086 — port-forward svc/agent-api first).
"""
from __future__ import annotations
import json
import os
import subprocess
import sys
import threading
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, HERE)
import state  # noqa: E402

CAPTURE_PY = os.path.join(HERE, "capture.py")

_LOCK = threading.Lock()
CAP = {"running": False, "phase": "idle", "ok": None, "message": ""}


def _set(**kw):
    with _LOCK:
        CAP.update(kw)


def _rebuild():
    subprocess.run([sys.executable, os.path.join(HERE, "build_gateway.py")],
                   cwd=REPO, check=False)


def _capture_worker():
    _set(running=True, phase="capturing", ok=None, message="Running the external agent…")
    jsonl = os.path.join("/tmp", "gateway-run.jsonl")
    open(jsonl, "w").close()
    env = dict(os.environ,
               XDG_RUNTIME_DIR=os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"),
               XDG_DATA_HOME=os.environ.get("XDG_DATA_HOME", os.path.expanduser("~/.local/share")))
    proc = subprocess.run([sys.executable, CAPTURE_PY, "--emit-jsonl", jsonl],
                          cwd=REPO, env=env, capture_output=True, text=True)
    if proc.returncode == 0:
        events = state.read_jsonl(open(jsonl).read())
        d = os.path.join(HERE, "recordings")
        os.makedirs(d, exist_ok=True)
        p = state.save_recording(d, events)
        import shutil
        shutil.copy(p, os.path.join(d, "canonical.json"))
        _rebuild()
        _set(phase="done", ok=True, message=f"Captured {len(events)} events — reloading.")
    else:
        msg = (proc.stderr or proc.stdout or "capture.py failed").strip().splitlines()[-1:]
        _set(phase="failed", ok=False,
             message=f"Capture failed — kept the previous recording. {msg[0] if msg else ''}")
    _set(running=False)


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *a, **kw):
        super().__init__(*a, directory=HERE, **kw)

    def log_message(self, *a):  # quiet
        pass

    def _json(self, code, obj):
        body = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path.startswith("/api/capture/status"):
            with _LOCK:
                return self._json(200, dict(CAP))
        return super().do_GET()

    def do_POST(self):
        if self.path.startswith("/api/capture"):
            with _LOCK:
                if CAP["running"]:
                    return self._json(409, {"error": "a capture is already running"})
            threading.Thread(target=_capture_worker, daemon=True).start()
            return self._json(202, {"started": True})
        self._json(404, {"error": "not found"})


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8521
    _rebuild()
    httpd = ThreadingHTTPServer(("0.0.0.0", port), Handler)
    print(f"▶ gateway cinematic: http://localhost:{port}/gateway.html")
    httpd.serve_forever()


if __name__ == "__main__":
    main()
```

- [ ] **Step 3: Sanity-check syntax**

Run: `agent/.venv/bin/python -c "import ast; ast.parse(open('demos/04-external-agent/present/gateway_server.py').read())"`
Expected: no output (valid syntax)

- [ ] **Step 4: Commit**

```bash
git add demos/04-external-agent/present/gateway_server.py demos/04-external-agent/present/recordings/.gitignore
git commit -m "demos/04: add gateway_server.py (serve + headless capture, mirrors boardroom_server.py)"
```

---

### Task 13: Docs + end-to-end manual verification

**Files:**
- Create: `demos/04-external-agent/present/README.md`
- Modify: `demos/README.md` (demo 4's table row)

**Interfaces:** none (documentation + final manual check).

- [ ] **Step 1: Create `present/README.md`**

Create `demos/04-external-agent/present/README.md`:

```markdown
# External mandated agent — presentation extras

`app.py` (one level up) is the live demo — unchanged in behavior, now
restyled into a two-tone nav+centre stepper and saving each run here as a
recording. This directory adds a standalone **animated replay**.

## 🎬 Animated gateway cinematic (standalone)

A split-screen view of a recorded run: the external agent on the left (an
"outsider" palette — it never touches the bank directly), the personal
manager on the right (nano-bank's own brand palette), and a gateway rail
between them that lights 🟢/🔴/🟡 for each mandate-gated act and animates
the A2A hand-off for each message. Zero model delay — it replays a captured
recording.

    demos/04-external-agent/present/gateway_server.py    # -> http://localhost:8521/gateway.html

Or build once and open the file directly (uses whatever
`recordings/canonical.json` is already checked in / captured):

    python3 demos/04-external-agent/present/build_gateway.py

Controls: ▶ Run / Pause (Space), ⏮ ⏭ step (arrows), a speed slider, a
progress scrubber. The **⦿ Capture live** button (only works when served via
`gateway_server.py`, needs `DEMO_BRANCH_BASE` + `AGENT_GATEWAY_TOKEN` +
`OLLAMA_API_KEY` in the environment, and a port-forward to `svc/agent-api`)
re-runs `capture.py` against the deployed stack, saves the result as the
canonical recording, and rebuilds `gateway.html`.
```

- [ ] **Step 2: Update `demos/README.md`'s demo 4 row**

In `demos/README.md`, change:

```
| 4 | External mandated agent | `04-external-agent/` | An **autonomous LLM agent** operating a customer's bank **only through the agentic branch** (`/agent-gateway/*`), under a customer-granted **mandate** (scoped, capped, revocable): a high-level instruction → plan → mandate-gated acts (bill payment to Epcor) + A2A to the manager, with a Revoke button. |
```

to:

```
| 4 | External mandated agent | `04-external-agent/` | An **autonomous LLM agent** operating a customer's bank **only through the agentic branch** (`/agent-gateway/*`), under a customer-granted **mandate** (scoped, capped, revocable): a high-level instruction → plan → mandate-gated acts (bill payment to Epcor) + A2A to the manager for a car-loan question, with a Revoke button, shown as a two-tone stepper. A standalone **animated split-screen cinematic** replays a captured run — see `04-external-agent/present/README.md`. |
```

- [ ] **Step 3: Full offline regression check (Part A + Part B)**

Run:
```bash
agent/.venv/bin/python -m pytest agent -q
python3 -m pytest demos/04-external-agent/present/tests -v
```
Expected: PASS, no failures.

- [ ] **Step 4: End-to-end manual verification against the live stack**

Needs the deployed stack (`./nb up --demo 04-external-agent`, or the manual
port-forward + `DEMO_BRANCH_BASE`/`AGENT_GATEWAY_TOKEN`/`OLLAMA_API_KEY` env
vars per `demos/README.md`). With the stack up:

1. In `app.py` (the live console), click **Seed mandate**, then **▶ Run agent**
   with the default instruction. Confirm the bill-payment `act` shows
   allow/deny styling and the loan `message` step shows a real answer from
   the manager mentioning a rate, term, and estimated monthly payment (the
   `loan.md` skill firing) — the manager should also state it has proposed
   the loan and that confirmation happens in the app, not through this A2A
   channel.
2. Run `demos/04-external-agent/present/gateway_server.py`, open
   `http://localhost:8521/gateway.html`, click **⦿ Capture live**, and once
   it reloads, click **▶ Run** and confirm the split-screen plays through
   all steps with the gateway pill lighting for the `act` event.
3. Separately (not through demo 4 — via demo 3's manager console or a
   direct `/branch/clients/{cid}/message` call), confirm a
   `propose_loan_application` proposal's `id` can be executed via
   `POST /branch/clients/{cid}/actions/{id}/confirm` and that it lands two
   bank calls (`apply_for_loan` then `disburse_loan`) — i.e. the loan
   actually gets disbursed into chequing.

- [ ] **Step 5: Commit**

```bash
git add demos/04-external-agent/present/README.md demos/README.md
git commit -m "demos/04: document the gateway cinematic + update the demos table"
```
