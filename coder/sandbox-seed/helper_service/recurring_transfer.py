def within_monthly_cap(amount_cents: int, spent_this_month_cents: int,
                        cap_cents: int = 5000) -> bool:
    """Pilot guardrail: True if adding amount_cents to this month's spend stays
    within cap_cents (default $50.00, the recurring e-Transfer pilot's monthly
    per-customer cap). STUB -- the delivery task implements it."""
    raise NotImplementedError
