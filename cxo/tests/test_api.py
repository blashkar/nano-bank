from fastapi.testclient import TestClient
from cxo.config import Settings
from cxo.api import create_app
from cxo import escalations


async def _fake_ask(settings, message, thread_id=None):
    return {"answer": "ok"}


def _client():
    app = create_app(Settings.from_env({}), ask_fn=_fake_ask, probes={})
    return TestClient(app)


def test_livez_ok():
    assert _client().get("/livez").json()["service"] == "cxo"


def test_escalations_intake_records_pending():
    escalations.clear()
    c = _client()
    r = c.post("/escalations", json={"cx_issue_id": "i1", "customer_id": "c1",
                                     "severity": "urgent", "category": "rail_experience",
                                     "summary": "expired"})
    assert r.status_code == 200 and r.json()["recorded"] is True
    assert escalations.pending()[0]["cx_issue_id"] == "i1"
    escalations.clear()


def test_ask_delegates_to_ask_fn():
    assert _client().post("/ask", json={"message": "posture?"}).json()["answer"] == "ok"
