from cx import seed_surveys as s


def test_campaign_specs_are_nps_and_csat():
    specs = s.campaign_specs()
    insts = {sp["instrument"] for sp in specs}
    assert insts == {"nps", "csat"}
    # CSAT targets customers who filed complaints — a coherent low-CSAT story.
    assert any(sp["segment"] == "has_open_issue" for sp in specs)
    assert all(sp["question"] for sp in specs)
