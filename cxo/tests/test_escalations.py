from cxo import escalations as e


def test_record_and_pending_roundtrip():
    e.clear()
    e.record({"cx_issue_id": "i1", "customer_id": "c1", "severity": "urgent",
              "category": "rail_experience", "summary": "expired"})
    p = e.pending()
    assert len(p) == 1 and p[0]["cx_issue_id"] == "i1"
    e.clear()
    assert e.pending() == []


def test_pending_is_capped():
    e.clear()
    for i in range(60):
        e.record({"cx_issue_id": f"i{i}", "severity": "high"})
    assert len(e.pending()) <= 50   # bounded, newest kept
    assert e.pending()[-1]["cx_issue_id"] == "i59"
    e.clear()
