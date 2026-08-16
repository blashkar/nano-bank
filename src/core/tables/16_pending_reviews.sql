-- ---------------------------------------------------------------------------
-- Held customer movements, awaiting a fraud reviewer's verdict.
-- ---------------------------------------------------------------------------
--
-- A `hold_review` from the fraud engine used to be a dead end: the bank
-- declined, the money went home, and a reviewer clearing the case changed
-- nothing for the customer, who had to retry from scratch into a rule that
-- would hold them again.
--
-- FRAUD_ENGINE_ARCHITECTURE_V2 §7 says the opposite — the movement is held
-- with an explicit expiry, and **on pass, execute**; §14 puts a 15-minute SLA
-- on held transactions "because money is waiting". This table is where a held
-- movement waits.
--
-- It deliberately does NOT reuse `pending_approvals` (11_agents.sql), which
-- parks an agent transfer awaiting its owner's step-up approval. That is the
-- same *mechanism* and a different *authorization model*: there a principal
-- grants permission, here a reviewer adjudicates risk. Sharing a row shape
-- would mean one CHECK constraint and one status vocabulary serving two
-- meanings.
--
-- KNOWN GAP, deliberate: this parks the *intent*, not the funds. §7 wants the
-- funds reserved and unmoved; the bank has no balance-reservation primitive
-- today and `pending_approvals` has the same limitation. So a cleared review
-- re-checks funds when it executes and may fail there. See the issue filed
-- alongside this table.
CREATE TABLE pending_reviews (
    review_id       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    customer_id     UUID NOT NULL REFERENCES customers(customer_id) ON DELETE CASCADE,
    account_id      UUID NOT NULL REFERENCES accounts(account_id),

    -- Which rail parked it. Determines how `movement` is read back on release.
    rail            TEXT NOT NULL CHECK (rail IN ('transfer', 'interac_etransfer')),
    amount          DECIMAL(15,2) NOT NULL CHECK (amount > 0),
    idempotency_key VARCHAR(128),
    -- The rail-specific payload needed to execute the movement later, shaped by
    -- `rail`. JSONB rather than a column per rail: two rails already disagree on
    -- what a destination is (an account id vs a handle), and a nullable column
    -- per field per rail would encode that disagreement in the schema.
    --
    -- Never contains a plaintext secret: the e-Transfer security answer is
    -- hashed BEFORE parking, exactly as the send path hashes it, so a parked row
    -- is no more sensitive than the `interac_etransfers` row it becomes.
    movement        JSONB NOT NULL,

    -- How we ask what became of it: the engine's
    -- GET /v1/decisions/{operation_id}/disposition.
    operation_id    UUID NOT NULL UNIQUE,
    decision_id     UUID,

    -- 'executing' is the transient claim while the released movement posts,
    -- mirroring pending_approvals: a crash mid-execution leaves the row
    -- reclaimable once claimed_at ages out, and the rail's own idempotency key
    -- stops a reclaim from double-paying.
    status          VARCHAR(20) NOT NULL DEFAULT 'held'
                    CHECK (status IN ('held', 'executing', 'executed', 'refused', 'expired')),
    transaction_id  UUID,
    -- Why it ended the way it did: the reviewer's verdict ('confirmed_fraud'),
    -- 'expired', or the error a release hit when it tried to execute.
    resolution_note TEXT,

    claimed_at      TIMESTAMP WITH TIME ZONE,
    created_at      TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP NOT NULL,
    expires_at      TIMESTAMP WITH TIME ZONE NOT NULL,
    resolved_at     TIMESTAMP WITH TIME ZONE
);

-- A customer retry of the same request maps onto the SAME open park instead of
-- stacking a second one. "Open" = held OR executing, for the same reason
-- pending_approvals counts executing as open: a row being released right now
-- must still swallow retries, or a duplicate parked during the executing window
-- could be released concurrently and pay twice.
CREATE UNIQUE INDEX idx_pending_reviews_open
    ON pending_reviews (customer_id, idempotency_key)
    WHERE status IN ('held', 'executing') AND idempotency_key IS NOT NULL;

CREATE INDEX idx_pending_reviews_customer ON pending_reviews (customer_id, created_at);
-- The §14 SLA query: how long has money been waiting, oldest first.
CREATE INDEX idx_pending_reviews_waiting
    ON pending_reviews (created_at) WHERE status = 'held';
