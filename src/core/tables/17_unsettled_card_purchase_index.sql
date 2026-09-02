-- ---------------------------------------------------------------------------
-- Make the Visa settlement batch stop scanning every card purchase ever made.
-- ---------------------------------------------------------------------------
--
-- `settle_batch` (api/src/handlers/cards.rs) tags the captures a batch covers:
--
--     UPDATE transactions
--     SET metadata = jsonb_set(COALESCE(metadata,'{}'::jsonb), '{settled}', 'true')
--     WHERE transaction_type = 'card_purchase'
--       AND (metadata->>'settled') IS DISTINCT FROM 'true'
--
-- Settling all so-far-unsettled purchases is the intended semantics of a batch
-- and is not what this changes. The problem is that nothing indexes the
-- predicate. `idx_transactions_type` narrows to card_purchase and Postgres then
-- evaluates the jsonb test row by row over all of them.
--
-- That cost nothing while the table was small. Measured on 2026-09-02 against a
-- database holding two full simulated corpora — 4,316,233 transactions, of them
-- 644,536 card_purchase — one settlement took **10.65 seconds** and updated
-- **zero** rows, because everything was already settled. The world-model
-- harness calls settle once per card authorization, so the cost is quadratic in
-- corpus size: it is what made a 650k-intent run take 43.5 hours at 4.2
-- decisions/second, and it eventually exceeded the client timeout outright and
-- aborted the run.
--
-- A partial index over exactly the predicate fixes it. Almost every row is
-- already settled, so the index stays small and holds only the rows a batch
-- actually has to touch:
--
--     before   10,650 ms
--     after         2 ms       (Index Only Scan, ~5000x)
--
-- Same corpus then ran at 19.7 decisions/second.
--
-- CONCURRENTLY so applying this to a live database does not take an
-- ACCESS EXCLUSIVE lock on the busiest table in the bank. That means it cannot
-- run inside a transaction block — if this file is applied by a runner that
-- wraps each script in BEGIN/COMMIT, drop the keyword rather than the index.
-- ---------------------------------------------------------------------------

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_transactions_unsettled_card_purchase
    ON transactions (transaction_id)
    WHERE transaction_type = 'card_purchase'
      AND (metadata->>'settled') IS DISTINCT FROM 'true';
