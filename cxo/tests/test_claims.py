from cxo import claims


def test_flags_pnl_without_disclaimer():
    out = claims.unsupported_claims("Our net interest margin improved to 3.2%.", [])
    assert any("CFO" in x for x in out)


def test_flags_reliability_without_disclaimer():
    out = claims.unsupported_claims("The platform had 3 crashlooping pods this week.", [])
    assert any("CTO" in x for x in out)


def test_disclaimed_pnl_is_not_flagged():
    out = claims.unsupported_claims(
        "I cannot speak to net interest margin — that is the CFO's domain.", [])
    assert out == []


def test_clean_cx_answer_is_clean():
    out = claims.unsupported_claims(
        "Card adoption is 25% and 12 rail_experience issues are open.", [])
    assert out == []
