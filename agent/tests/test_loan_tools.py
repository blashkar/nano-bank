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
