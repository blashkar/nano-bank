from csuite.trace_view import board_contributions, _tool_output_field


# The real trace stores a tool event's output as str(ToolMessage): a
# `content='<json>' name='...' tool_call_id='...'` wrapper, NOT a bare dict/json.
_CONSULT_OUT = (
    "content='{\"officer\": \"cfo\", \"answer\": \"NIM is 3.1% and RAROC 0.14%.\"}'"
    " name='consult_cfo' tool_call_id='abc123'")
_DIRECT_OUT = (
    "content='{\"peer\": \"coo\", \"officer_acted\": true, \"officer_row\": null,"
    " \"officer_response\": \"Refused: no open batch.\"}' name='direct_coo'"
    " tool_call_id='def456'")


def test_tool_output_field_unwraps_toolmessage_content():
    assert _tool_output_field(_CONSULT_OUT, "answer") == "NIM is 3.1% and RAROC 0.14%."
    assert _tool_output_field(_CONSULT_OUT, "officer") == "cfo"


def test_tool_output_field_still_reads_plain_dict():
    assert _tool_output_field({"answer": "hi"}, "answer") == "hi"


def test_board_contributions_from_toolmessage_trace():
    trace = [
        {"kind": "tool", "name": "consult_cfo", "output": _CONSULT_OUT},
        {"kind": "tool", "name": "direct_coo", "output": _DIRECT_OUT},
    ]
    c = board_contributions(trace)
    assert c[0] == {"officer": "cfo", "role": "consult",
                    "text": "NIM is 3.1% and RAROC 0.14%."}
    assert c[1]["officer"] == "coo" and c[1]["role"] == "direct"
    assert c[1]["text"] == "Refused: no open batch." and c[1]["acted"] is True


def test_board_contributions_handles_python_dict_repr():
    # a test-double / stringified plain dict (single quotes, not JSON)
    trace = [{"kind": "tool", "name": "consult_cxo",
              "output": "{'officer': 'cxo', 'answer': 'NPS is 16.'}"}]
    assert board_contributions(trace)[0]["text"] == "NPS is 16."
