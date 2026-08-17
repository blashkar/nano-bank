-- src/core/tables/11_cx_surveys.sql — NPS/CSAT survey campaigns + responses.
DO $$ BEGIN
  CREATE TYPE survey_instrument AS ENUM ('nps','csat');
EXCEPTION WHEN duplicate_object THEN null; END $$;

CREATE TABLE IF NOT EXISTS survey_campaigns (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    instrument  survey_instrument NOT NULL,
    segment     TEXT NOT NULL,
    question    TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'open',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS survey_responses (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    campaign_id UUID NOT NULL REFERENCES survey_campaigns(id),
    customer_id UUID NOT NULL REFERENCES customers(customer_id),
    score       INT NOT NULL,
    comment     TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_survey_responses_campaign ON survey_responses(campaign_id);
CREATE INDEX IF NOT EXISTS idx_survey_campaigns_instrument ON survey_campaigns(instrument);
