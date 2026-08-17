import datetime as dt
from cx import metrics as m


def test_pct_guards_zero_denominator():
    assert m.pct(3, 0) == 0.0
    assert m.pct(1, 4) == 25.0


def test_onboarding_funnel_shapes_counts():
    r = m.onboarding_funnel(
        [{"total": 100, "kyc_completed": 80, "kyc_pending": 20}],
        [{"total": 130, "active": 110, "pending_activation": 20}])
    assert r["customers"] == 100 and r["kyc_completed"] == 80
    assert r["kyc_completion_rate"] == 80.0
    assert r["accounts_pending_activation"] == 20


def test_product_adoption_rate_per_product():
    r = m.product_adoption([{"product": "card", "customers": 30},
                            {"product": "payment", "customers": 60}],
                           [{"active_customers": 120}])
    by = {d["product"]: d for d in r["products"]}
    assert by["card"]["adoption_rate"] == 25.0
    assert by["payment"]["customers"] == 60
    assert r["active_customers"] == 120
    assert r["products"][0]["product"] == "payment"   # sorted by customers desc


def test_friction_metrics_txn_and_interac():
    r = m.friction_metrics(
        [{"product": "card", "total": 200, "failed": 10}],
        [{"status": "deposited", "n": 70},
         {"status": "expired", "n": 20}, {"status": "declined", "n": 10}])
    card = {d["product"]: d for d in r["transaction_failure"]}["card"]
    assert card["failure_rate"] == 5.0
    assert r["interac"]["expired_rate"] == 20.0
    assert r["interac"]["completed"] == 70


def test_engagement_active_vs_dormant():
    now = dt.datetime(2026, 8, 15, tzinfo=dt.timezone.utc)
    rows = [{"customer_id": "a", "last_txn": now - dt.timedelta(days=3)},
            {"customer_id": "b", "last_txn": now - dt.timedelta(days=40)},
            {"customer_id": "c", "last_txn": None}]
    r = m.engagement_metrics(rows, window_days=30, now=now)
    assert r["active"] == 1 and r["dormant"] == 2
    assert r["active_rate"] == round(100 / 3, 2)


def test_issue_summary_by_category_severity_and_trend():
    now = dt.datetime(2026, 8, 15, tzinfo=dt.timezone.utc)
    rows = [
        {"category": "rail_experience", "severity": "high", "status": "open",
         "created_at": now - dt.timedelta(days=2), "resolved_at": None},
        {"category": "rail_experience", "severity": "urgent", "status": "open",
         "created_at": now - dt.timedelta(days=1), "resolved_at": None},
        {"category": "fees", "severity": "low", "status": "resolved",
         "created_at": now - dt.timedelta(days=3), "resolved_at": now}]
    r = m.issue_summary(rows, now=now)
    assert r["open"] == 2 and r["by_category"]["rail_experience"] == 2
    assert r["by_severity"]["urgent"] == 1
    assert r["top_theme"] == "rail_experience"


def test_notable_issues_high_severity_first_limited():
    rows = [
        {"id": "1", "severity": "low", "summary": "x", "category": "fees",
         "customer_id": "c1", "created_at": "2026-08-10"},
        {"id": "2", "severity": "urgent", "summary": "y", "category": "rail_experience",
         "customer_id": "c2", "created_at": "2026-08-14"}]
    out = m.notable_issues(rows, limit=1)
    assert len(out) == 1 and out[0]["id"] == "2"
    assert "customer_id" in out[0]  # scoped id retained for re-grounding


def test_nps_score_and_buckets():
    # 5 promoters(9,10,9,10,9), 2 passives(7,8), 3 detractors(0,3,6)
    scores = [9, 10, 9, 10, 9, 7, 8, 0, 3, 6]
    r = m.nps(scores)
    assert r["responses"] == 10
    assert r["promoters"] == 5 and r["passives"] == 2 and r["detractors"] == 3
    assert r["score"] == 20   # round(50% - 30%)


def test_nps_empty_is_zero():
    r = m.nps([])
    assert r["responses"] == 0 and r["score"] == 0


def test_csat_rate_and_mean():
    scores = [5, 4, 3, 4, 1]           # satisfied(>=4) = 3 of 5
    r = m.csat(scores)
    assert r["responses"] == 5 and r["satisfied"] == 3
    assert r["csat_rate"] == 60.0
    assert r["mean"] == 3.4
