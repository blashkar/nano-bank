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
