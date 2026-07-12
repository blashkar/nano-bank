from agent.trace import TraceRecorder


def test_tool_start_end_produces_one_event():
    r = TraceRecorder()
    r.on_tool_start({"name": "get_accounts"}, "{}", run_id="a")
    r.on_tool_end("[{'account_id': 'x'}]", run_id="a")
    evs = r.events()
    assert len(evs) == 1
    e = evs[0]
    assert e["kind"] == "tool" and e["name"] == "get_accounts" and e["ok"] is True
    assert "account_id" in e["output"] and isinstance(e["elapsed_ms"], int)
    assert e["seq"] == 0


def test_tool_error_marks_not_ok():
    r = TraceRecorder()
    r.on_tool_start({"name": "propose_transfer"}, "{...}", run_id="b")
    r.on_tool_error(ValueError("nope"), run_id="b")
    e = r.events()[0]
    assert e["ok"] is False and "nope" in e["error"]
