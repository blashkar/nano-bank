from fastapi.testclient import TestClient
from cfo.config import Settings
from cfo.api import create_app


def _client(ask_fn):
    s = Settings.from_env({"OLLAMA_API_KEY": "x"})
    return TestClient(create_app(s, ask_fn=ask_fn))


def test_ask_endpoint_returns_answer():
    async def fake_ask(settings, message, thread_id=None):
        return {"answer": f"echo:{message}", "thread_id": thread_id or "t",
                "trace": []}
    r = _client(fake_ask).post("/ask", json={"message": "hi", "thread_id": "t1"})
    assert r.status_code == 200
    body = r.json()
    assert body["answer"] == "echo:hi"
    assert body["thread_id"] == "t1"


def test_health_endpoint():
    async def fake_ask(*a, **k):
        return {"answer": "", "thread_id": "t", "trace": []}
    r = _client(fake_ask).get("/health")
    assert r.status_code == 200
    assert r.json()["status"] == "ok"
