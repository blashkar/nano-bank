import random
from cx import campaigns as c


def test_simulate_score_is_deterministic():
    a = [c.simulate_score("nps", 0, random.Random(1)) for _ in range(20)]
    b = [c.simulate_score("nps", 0, random.Random(1)) for _ in range(20)]
    assert a == b


def test_nps_negative_sentiment_scores_lower_than_positive():
    rng = random.Random(3)
    neg = [c.simulate_score("nps", -1, rng) for _ in range(200)]
    pos = [c.simulate_score("nps", 1, rng) for _ in range(200)]
    assert sum(neg) / 200 < sum(pos) / 200
    assert all(0 <= s <= 10 for s in neg + pos)


def test_csat_ranges_and_correlation():
    rng = random.Random(5)
    neg = [c.simulate_score("csat", -1, rng) for _ in range(200)]
    pos = [c.simulate_score("csat", 1, rng) for _ in range(200)]
    assert all(1 <= s <= 5 for s in neg + pos)
    assert sum(neg) / 200 < sum(pos) / 200
