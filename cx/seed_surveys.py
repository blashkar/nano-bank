# cx/seed_surveys.py — create the demo survey campaigns + simulate responses.
from __future__ import annotations
from . import campaigns as _campaigns
from .db import CxDB


def campaign_specs() -> list[dict]:
    return [
        {"instrument": "nps", "segment": "all_active",
         "question": "How likely are you to recommend nano-bank to a friend?"},
        # CSAT among customers who raised a complaint — correlates to low satisfaction,
        # a coherent "close the loop on detractors" story for the CXO.
        {"instrument": "csat", "segment": "has_open_issue",
         "question": "After your recent issue, how satisfied are you with the resolution?"},
    ]


def seed(db_params: dict) -> list[dict]:
    db = CxDB(db_params)
    # clear prior demo campaigns so the seed is reproducible — scoped to
    # source='demo_seed' so a campaign created through any other path is
    # never touched by a re-seed.
    import psycopg2
    conn = psycopg2.connect(**db_params)
    try:
        with conn, conn.cursor() as cur:
            cur.execute(
                "DELETE FROM survey_responses WHERE campaign_id IN"
                " (SELECT id FROM survey_campaigns WHERE source = 'demo_seed')")
            cur.execute("DELETE FROM survey_campaigns WHERE source = 'demo_seed'")
    finally:
        conn.close()
    return [_campaigns.create_campaign(db, sp["instrument"], sp["segment"], sp["question"])
            for sp in campaign_specs()]


if __name__ == "__main__":
    from .config import Settings
    for r in seed(Settings.from_env().db):
        print("seeded campaign", r)
