import json
from ceo.audit import Audit


class FakeCursor:
    def __init__(self, script): self.script = script; self.executed = []; self._last = None
    def execute(self, sql, params=None):
        self.executed.append((" ".join(sql.split()), params))
        for key, rows in self.script.items():
            if key in " ".join(sql.split()):
                self._last = rows
                return
        self._last = []
    def fetchone(self): return self._last[0] if self._last else None
    def fetchall(self): return list(self._last)
    def __enter__(self): return self
    def __exit__(self, *a): return False


class FakeConn:
    def __init__(self, script): self._script = script; self.cur = FakeCursor(script)
    def set_session(self, **k): pass
    def cursor(self, **k): return self.cur
    def __enter__(self): return self
    def __exit__(self, *a): return False
    def close(self): pass


def _audit(script):
    conn = FakeConn(script)
    return Audit({"host": "x"}, connect=lambda: conn), conn


def test_latest_actor_seq_returns_int():
    audit, conn = _audit({"MAX(seq)": [(42,)]})
    assert audit.latest_actor_seq("coo") == 42
    sql, params = conn.cur.executed[-1]
    assert "FROM agent_action_ledger WHERE actor=%s" in sql and params == ("coo",)


def test_rows_since_shapes_rows():
    audit, conn = _audit({"seq>%s": [(11, "cut_aft_batch", {"batch": "B7"})]})
    rows = audit.rows_since("coo", 10)
    assert rows == [{"seq": 11, "action": "cut_aft_batch", "effect": {"batch": "B7"}}]
    sql, params = conn.cur.executed[-1]
    assert params == ("coo", 10)


def test_direct_appends_ceo_row_with_json_params():
    audit, conn = _audit({"append_agent_action": [(999, "deadbeef")]})
    out = audit.direct("coo", {"directive": "cut it", "rationale": "stuck"},
                       {"officer_acted": True})
    assert out == {"seq": 999, "entry_hash": "deadbeef"}
    sql, params = conn.cur.executed[-1]
    assert "append_agent_action('ceo', %s, %s::jsonb, %s::jsonb)" in sql
    assert params[0] == "direct_coo"
    assert json.loads(params[1]) == {"directive": "cut it", "rationale": "stuck"}
    assert json.loads(params[2]) == {"officer_acted": True}
