from ceo import claims


def _direct_event(peer, acted):
    return {"kind": "tool", "name": f"direct_{peer}",
            "output": {"peer": peer, "officer_acted": acted, "officer_row": None,
                       "officer_response": "..."}}


def test_flags_completion_claim_when_no_lever_fired():
    trace = [_direct_event("coo", False)]
    out = claims.unsupported_claims("I directed the COO and the batch was cut.", trace)
    assert out and "coo" in out[0].lower()


def test_ok_when_completion_claim_and_lever_fired():
    trace = [_direct_event("coo", True)]
    assert claims.unsupported_claims("The COO cut the batch, done.", trace) == []


def test_ok_when_no_completion_cue_even_if_no_lever():
    trace = [_direct_event("cto", False)]
    assert claims.unsupported_claims(
        "I asked the CTO; it judged no rollback was warranted.", trace) == []


def test_handles_stringified_tool_output():
    trace = [{"kind": "tool", "name": "direct_coo",
              "output": "{'peer': 'coo', 'officer_acted': False}"}]
    out = claims.unsupported_claims("The COO executed the directive successfully.", trace)
    assert out


def test_ok_when_answer_honestly_reports_the_directive_did_not_fire():
    # this is the exact honest-failure phrasing the CEO's own prompt asks for —
    # the bare word "executed"/"cut"/"completed" appears, but negated.
    trace = [_direct_event("coo", False)]
    honest_reports = [
        "I directed the COO; the batch was not executed because no open AFT batch existed.",
        "The COO declined — it did not cut the batch.",
        "No lever fired; the directive was not completed.",
        "The COO failed to execute the directive; no batch was open.",
    ]
    for answer in honest_reports:
        assert claims.unsupported_claims(answer, trace) == [], answer


def test_still_flags_completion_claim_past_a_negation_elsewhere_in_the_answer():
    # a negation earlier in an unrelated clause shouldn't blanket-suppress a
    # later, genuine overclaim.
    trace = [_direct_event("coo", False)]
    out = claims.unsupported_claims(
        "The CFO reported no change in NIM. Separately, the COO cut the batch.", trace)
    assert out and "coo" in out[0].lower()
