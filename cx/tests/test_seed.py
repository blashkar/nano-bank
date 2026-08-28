from cx import seed_cx_issues as s


def test_build_issue_rows_is_deterministic_and_varied():
    ids = [f"c{i}" for i in range(10)]
    a = s.build_issue_rows(ids, n=40, seed=7)
    b = s.build_issue_rows(ids, n=40, seed=7)
    assert a == b                              # deterministic
    assert len(a) == 40
    cats = {r["category"] for r in a}
    sevs = {r["severity"] for r in a}
    assert len(cats) >= 4 and len(sevs) >= 3   # varied
    assert any(r["severity"] == "urgent" for r in a)  # at least one escalatable
    assert all(r["customer_id"] in ids for r in a)
