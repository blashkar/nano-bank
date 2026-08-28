import asyncio
import pytest
from csuite import collab


class FakeResp:
    def __init__(self, payload): self._p = payload
    def raise_for_status(self): pass
    def json(self): return self._p


class FakeClient:
    """Records posts; returns a scripted /ask payload."""
    def __init__(self, payload): self.payload = payload; self.posts = []
    async def post(self, url, json=None):
        self.posts.append((url, json))
        return FakeResp(self.payload)


def test_consult_relays_answer_attributed():
    client = FakeClient({"answer": "NIM was 3.1% last month."})
    tool = collab.consult_tool("cfo", "http://cfo:8089", client=client)
    assert tool.name == "consult_cfo"
    out = asyncio.run(tool.ainvoke({"question": "What was NIM?"}))
    assert out == {"officer": "cfo", "answer": "NIM was 3.1% last month."}
    assert client.posts == [("http://cfo:8089/ask", {"message": "What was NIM?"})]


def test_consult_surfaces_down_peer_as_error():
    class Boom:
        async def post(self, url, json=None): raise RuntimeError("connection refused")
    tool = collab.consult_tool("coo", "http://coo:8093", client=Boom())
    with pytest.raises(RuntimeError):
        asyncio.run(tool.ainvoke({"question": "status?"}))


class FakeAudit:
    """A fake ledger: `new_rows` are the peer rows that 'appear' during the POST."""
    def __init__(self, before_seq=10, new_rows=None):
        self.before_seq = before_seq
        self.new_rows = new_rows or []
        self.direct_calls = []

    def latest_actor_seq(self, actor): return self.before_seq
    def rows_since(self, actor, seq): return list(self.new_rows)
    def direct(self, peer, params, effect):
        self.direct_calls.append((peer, params, effect))
        return {"seq": 999, "entry_hash": "abc"}


def test_direct_records_officer_row_when_lever_fires():
    audit = FakeAudit(before_seq=10, new_rows=[
        {"seq": 11, "action": "cut_aft_batch", "effect": {"batch": "B7", "entries": 3}}])
    client = FakeClient({"answer": "Cut batch B7 (3 entries)."})
    tool = collab.direct_tool("coo", "http://coo:8093", audit, client=client)
    assert tool.name == "direct_coo"

    out = asyncio.run(tool.ainvoke({"directive": "Cut the pending AFT batch.",
                                    "rationale": "COO reported a stuck batch."}))
    assert out["peer"] == "coo"
    assert out["officer_acted"] is True
    assert out["officer_row"] == {"seq": 11, "action": "cut_aft_batch",
                                  "effect": {"batch": "B7", "entries": 3}}
    assert out["officer_response"] == "Cut batch B7 (3 entries)."
    assert len(audit.direct_calls) == 1
    peer, params, effect = audit.direct_calls[0]
    assert peer == "coo"
    assert params == {"directive": "Cut the pending AFT batch.",
                      "rationale": "COO reported a stuck batch."}
    assert effect["officer_acted"] is True
    assert effect["officer_row"]["action"] == "cut_aft_batch"


def test_direct_reports_no_lever_when_no_new_row():
    audit = FakeAudit(before_seq=10, new_rows=[])   # officer only talked / refused
    client = FakeClient({"answer": "I reviewed it; no action was warranted."})
    tool = collab.direct_tool("cto", "http://cto:8095", audit, client=client)

    out = asyncio.run(tool.ainvoke({"directive": "Roll back the deploy."}))
    assert out["officer_acted"] is False
    assert out["officer_row"] is None
    assert out["ambiguous"] is False
    assert audit.direct_calls[0][2]["officer_acted"] is False


def test_direct_flags_ambiguous_when_more_than_one_row_lands_in_the_window():
    # a second writer (another directive, or any other trigger of the officer's
    # lever) landed in the same before/after window — can't be honestly pinned
    # on this call, so it must not silently pick the last row as "the" result.
    audit = FakeAudit(before_seq=10, new_rows=[
        {"seq": 11, "action": "cut_aft_batch", "effect": {"batch": "B6"}},
        {"seq": 12, "action": "cut_aft_batch", "effect": {"batch": "B7"}}])
    client = FakeClient({"answer": "Cut batch B7."})
    tool = collab.direct_tool("coo", "http://coo:8093", audit, client=client)

    out = asyncio.run(tool.ainvoke({"directive": "Cut the pending AFT batch."}))
    assert out["officer_acted"] is True
    assert out["ambiguous"] is True
    assert out["officer_row"] is None   # can't honestly single one out
    assert out["candidate_rows"] == audit.new_rows
    assert audit.direct_calls[0][2]["ambiguous"] is True


def test_build_tools_wires_consults_for_all_and_directs_for_directable():
    audit = FakeAudit()
    registry = {"peers": {"cfo": "http://cfo:8089", "coo": "http://coo:8093",
                          "cto": "http://cto:8095", "cxo": "http://cxo:8098"},
                "directable": {"coo", "cto"}}
    tools = collab.build_tools(registry, audit)
    names = [t.name for t in tools]
    assert names[:4] == ["consult_cfo", "consult_coo", "consult_cto", "consult_cxo"]
    assert set(names[4:]) == {"direct_coo", "direct_cto"}
    assert "direct_cfo" not in names and "direct_cxo" not in names
