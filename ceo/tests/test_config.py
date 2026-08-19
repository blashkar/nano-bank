from ceo.config import Settings


def test_defaults_ports_and_peers():
    s = Settings.from_env({})
    assert s.api_port == 8099
    assert s.console_port == 8511
    assert s.coo_url == "http://coo:8093"
    assert s.cto_url == "http://cto:8095"
    reg = s.peer_registry()
    assert set(reg["peers"]) == {"cfo", "coo", "cto", "cxo"}
    assert reg["directable"] == {"coo", "cto"}
    assert reg["peers"]["cfo"] == "http://cfo:8089"


def test_db_and_model_from_env():
    s = Settings.from_env({"DB_HOST": "postgres-service", "CEO_MODEL": "kimi-k2.6"})
    assert s.db["host"] == "postgres-service"
    assert s.db["dbname"] == "nano_bank_db"
    assert s.ceo_model == "kimi-k2.6"


def test_resolve_model_prefers_primary_via_probe():
    from ceo import model_factory as mf
    s = Settings.from_env({"CEO_MODEL": "kimi-k2.6", "CEO_MODEL_FALLBACK": "kimi-k2.6"})
    assert mf.resolve_model(s, probe=lambda m, st: True) == "kimi-k2.6"
