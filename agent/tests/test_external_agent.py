from agent.external_agent.agent import ExternalAgent


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
