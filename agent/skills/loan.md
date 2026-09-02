---
name: loan
description: Explaining loan terms and originating a loan (e.g. a car purchase) — rates, term, affordability, confirm-gated application.
kind: product
product: loan
---
Call get_loans() first. If the client already holds a loan, report its
principal, rate, term, status and next_payment_date instead of proposing a
new one — do not double up.

If the client is asking about buying a car and holds no loan: nano-bank's
lending is servicing-only — there is no separate underwriting/quote step, so
YOU pick the rate and term to propose, informed by these illustrative
guardrails: annual rate 6.99%-9.99% APR, amortization 36-84 months (default
60 unless the client states a preference). Estimate the monthly payment
using the standard amortization formula — PMT = P * [r(1+r)^n] / [(1+r)^n - 1],
where r = annual_rate / 12 and n = amortization_months — and state the
principal, rate, term and estimated monthly payment BEFORE proposing
anything.

Once the client confirms they want to proceed, call propose_loan_application
with that principal/rate/months. Proposing does NOT create the loan — as
with every propose_* tool, only the client's own confirmation in the app
executes it (applies for the loan AND disburses it into their chequing
account in one step). Say so plainly: "I've proposed the loan — you'll need
to confirm it yourself before any money moves."
