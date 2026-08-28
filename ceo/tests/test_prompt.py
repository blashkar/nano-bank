from ceo.agent import CEO_PROMPT


def test_prompt_states_synthesizer_lane_and_attribution():
    p = CEO_PROMPT.lower()
    assert "chief executive" in p
    assert "consult" in p and "synthes" in p
    assert "attribut" in p
    assert "never invent" in p or "do not invent" in p


def test_prompt_names_directable_and_consult_only_seats():
    p = CEO_PROMPT.lower()
    assert "direct_coo" in p and "direct_cto" in p
    assert "cannot direct" in p or "consult-only" in p or "no levers" in p


def test_prompt_demands_honest_directive_reporting():
    p = CEO_PROMPT.lower()
    assert "fired" in p or "acted" in p
    assert "guardrail" in p
