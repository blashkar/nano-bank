from agent.external_agent.agent import ExternalAgent, _idem_key


class FakeGW:
    def __init__(self): self.calls = []
    def act(self, op, params): self.calls.append((op, params)); return {"decision": "allow", "result": {}}
    def message(self, msg): self.calls.append(("message", msg)); return {"answer": "savings are good", "trace": []}


def test_planned_steps_call_the_gateway():
    gw = FakeGW()
    a = ExternalAgent.from_plan([("act", "transfer_out", {"amount": "50"}),
                                 ("message", "benefits of a savings account?")], gateway=gw)
    events = a.run("pay my Epcor bill and tell me about savings")
    ops = [c[0] for c in gw.calls]
    assert "transfer_out" in ops and "message" in ops
    assert any(e["kind"] == "result" for e in events)
    assert events[0]["kind"] == "plan"


class _RecordingGW:
    def __init__(self): self.acts = []
    def act(self, op, params): self.acts.append((op, dict(params))); return {"decision": "allow"}
    def message(self, msg): return {"answer": "ok", "trace": []}


def test_idem_key_is_stable_for_same_op_and_params():
    a = _idem_key("transfer_out", {"amount": "50"})
    b = _idem_key("transfer_out", {"amount": "50"})
    c = _idem_key("transfer_out", {"amount": "60"})
    assert a == b and a != c


def test_run_attaches_stable_idempotency_key_to_act_steps():
    gw = _RecordingGW()
    agent = ExternalAgent.from_plan([("act", "transfer_out", {"amount": "50"})], gw)
    agent.run("pay the bill")
    assert gw.acts[0][1]["idempotency_key"] == _idem_key("transfer_out", {"amount": "50"})
