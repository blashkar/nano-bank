# cx/campaigns.py — the deterministic survey campaign runner. simulate_score is
# pure (seeded rng); create_campaign does the DB IO.
from __future__ import annotations
import random

# Weighted score buckets by (instrument, sentiment sign). Sentiment: -1 negative
# (open issue / dormant), +1 positive (active, no issue), 0 neutral.
_NPS_BUCKETS = {
    -1: [0, 1, 2, 3, 4, 5, 6, 6, 7],          # skew detractor
     0: [6, 7, 7, 8, 8, 9],                    # mixed
     1: [7, 8, 9, 9, 10, 10],                  # skew promoter
}
_CSAT_BUCKETS = {
    -1: [1, 2, 2, 3],
     0: [3, 3, 4],
     1: [4, 5, 5],
}


def simulate_score(instrument: str, sentiment: int, rng) -> int:
    sign = -1 if sentiment < 0 else (1 if sentiment > 0 else 0)
    buckets = _NPS_BUCKETS if instrument == "nps" else _CSAT_BUCKETS
    return rng.choice(buckets[sign])


def customer_sentiment(db, targets: list[str], window_days: int) -> dict:
    """-1 for customers with an open issue or dormant, +1 for active-no-issue, else 0."""
    issue = db.open_issue_customers()
    dormant = db.dormant_customers(window_days)
    out = {}
    for c in targets:
        if c in issue or c in dormant:
            out[c] = -1
        else:
            out[c] = 1
    return out


def create_campaign(db, instrument: str, segment: str, question: str,
                    seed: int = 7, window_days: int = 30) -> dict:
    if instrument not in ("nps", "csat"):
        raise ValueError(f"unknown instrument: {instrument}")
    targets = db.resolve_segment(segment, window_days)
    campaign_id = db.insert_campaign(instrument, segment, question)
    sentiment = customer_sentiment(db, targets, window_days)
    rng = random.Random(f"{seed}:{campaign_id}")
    rows = [(c, simulate_score(instrument, sentiment[c], rng)) for c in targets]
    if rows:
        db.insert_responses(campaign_id, rows)
    return {"campaign_id": campaign_id, "instrument": instrument, "segment": segment,
            "responses": len(rows)}
