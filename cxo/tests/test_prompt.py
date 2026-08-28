from cxo.agent import CXO_PROMPT


def test_prompt_mentions_surveys():
    low = CXO_PROMPT.lower()
    assert "nps" in low and "csat" in low and "survey" in low
    assert "detractor" in low
