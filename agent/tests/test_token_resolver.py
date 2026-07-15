from agent.api import SeedTokenResolver


class _S:
    nano_bank_api = "http://bank"


def test_resolver_relogins_after_ttl():
    calls = []
    clock = {"t": 1000.0}
    def fake_login(base, cred): calls.append(cred); return f"tok{len(calls)}"
    r = SeedTokenResolver(_S(), {"C1": ("e@x.ca", "pw")}, ttl_seconds=600,
                          now=lambda: clock["t"], login=fake_login)
    assert r.resolve("C1") == "tok1"      # first login
    clock["t"] += 300
    assert r.resolve("C1") == "tok1"      # still fresh -> cached
    clock["t"] += 400                     # now 700s > ttl 600
    assert r.resolve("C1") == "tok2"      # re-login
    assert len(calls) == 2


def test_resolver_unknown_customer_is_none():
    r = SeedTokenResolver(_S(), {}, login=lambda *a: "x")
    assert r.resolve("nope") is None
