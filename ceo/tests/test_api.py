from fastapi.testclient import TestClient
from ceo.config import Settings
from ceo.api import create_app


async def _fake_ask(settings, message, thread_id=None):
    return {"answer": "brief: the CFO reports NIM 3.1%; no directive taken."}


def _client():
    app = create_app(Settings.from_env({}), ask_fn=_fake_ask, probes={})
    return TestClient(app)


def test_livez_ok():
    assert _client().get("/livez").json()["service"] == "ceo"


def test_health_reports_service():
    body = _client().get("/health").json()
    assert body["service"] == "ceo" and body["status"] == "ok"


def test_ask_delegates_to_ask_fn():
    r = _client().post("/ask", json={"message": "state of the bank?"})
    assert "CFO reports" in r.json()["answer"]
