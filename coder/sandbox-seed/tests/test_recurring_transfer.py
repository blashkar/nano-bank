import pytest

from helper_service.recurring_transfer import within_monthly_cap


@pytest.mark.skip(reason="within_monthly_cap not implemented; delivery task implements it")
def test_within_cap():
    assert within_monthly_cap(2000, 2000) is True   # $20 + $20 = $40 <= $50 cap
    assert within_monthly_cap(2000, 3500) is False  # $20 + $35 = $55 > $50 cap
    assert within_monthly_cap(500, 4500) is True    # exactly at the $50 cap
