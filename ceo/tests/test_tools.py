from ceo.config import Settings
from ceo.tools import get_tools


class _Audit:
    def latest_actor_seq(self, a): return 0
    def rows_since(self, a, s): return []
    def direct(self, p, pa, e): return {"seq": 1, "entry_hash": "x"}


def test_get_tools_has_four_consults_and_two_directs():
    tools = get_tools(Settings.from_env({}), audit=_Audit())
    names = {t.name for t in tools}
    assert {"consult_cfo", "consult_coo", "consult_cto", "consult_cxo"} <= names
    assert {"direct_coo", "direct_cto"} <= names
    assert "direct_cfo" not in names and "direct_cxo" not in names
    assert len(tools) == 6
