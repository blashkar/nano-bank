from coo.config import Settings


def test_defaults():
    s = Settings.from_env({})
    assert s.coo_model == "glm-5.2"
    assert s.operations_mcp_url == "http://localhost:8092/mcp"
    assert s.memory_namespace == "coo"
    assert s.memory_collection == "coo_memory"
    assert s.api_port == 8093
    assert s.console_port == 8507
    assert s.subagent_max_depth == 2


def test_env_override():
    s = Settings.from_env({"COO_MODEL": "glm-x", "API_PORT": "9999",
                           "OPERATIONS_MCP_URL": "http://ops:1/mcp"})
    assert s.coo_model == "glm-x"
    assert s.api_port == 9999
    assert s.operations_mcp_url == "http://ops:1/mcp"
