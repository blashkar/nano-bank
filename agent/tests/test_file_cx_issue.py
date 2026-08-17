from agent import cx_issue_action as cia


class _FakeDB:
    def __init__(self):
        self.inserted = None

    def insert_cx_issue(self, customer_id, category, severity, summary, detail):
        self.inserted = (customer_id, category, severity, summary, detail)
        return "issue-123"


def test_low_severity_files_but_does_not_escalate():
    db = _FakeDB()
    calls = []
    res = cia.file_and_maybe_escalate(db, "c1", "http://cxo:8098", "fees", "low",
                                      "surprised by fee", "detail",
                                      http_post=lambda url, json: calls.append((url, json)))
    assert res["cx_issue_id"] == "issue-123" and db.inserted[0] == "c1"
    assert calls == []


def test_urgent_severity_escalates():
    db = _FakeDB()
    calls = []
    cia.file_and_maybe_escalate(db, "c1", "http://cxo:8098", "rail_experience", "urgent",
                                "e-transfer expired", "detail",
                                http_post=lambda url, json: calls.append((url, json)))
    assert calls and calls[0][0].endswith("/escalations")
    assert calls[0][1]["cx_issue_id"] == "issue-123" and calls[0][1]["severity"] == "urgent"


def test_escalate_failure_is_swallowed():
    db = _FakeDB()

    def boom(url, json):
        raise RuntimeError("cxo down")

    res = cia.file_and_maybe_escalate(db, "c1", "http://cxo:8098", "app_ux", "high",
                                      "x", "y", http_post=boom)
    assert res["cx_issue_id"] == "issue-123"  # filing still succeeds
