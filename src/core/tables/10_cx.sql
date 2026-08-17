-- src/core/tables/10_cx.sql — customer-experience issues filed by personal managers.
DO $$ BEGIN
  CREATE TYPE cx_issue_category AS ENUM
    ('onboarding','declines_friction','fees','rail_experience','app_ux','feature_request','other');
EXCEPTION WHEN duplicate_object THEN null; END $$;
DO $$ BEGIN
  CREATE TYPE cx_issue_severity AS ENUM ('low','medium','high','urgent');
EXCEPTION WHEN duplicate_object THEN null; END $$;
DO $$ BEGIN
  CREATE TYPE cx_issue_status AS ENUM ('open','acknowledged','resolved');
EXCEPTION WHEN duplicate_object THEN null; END $$;

CREATE TABLE IF NOT EXISTS cx_issues (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    customer_id  UUID NOT NULL REFERENCES customers(customer_id),
    category     cx_issue_category NOT NULL,
    severity     cx_issue_severity NOT NULL,
    summary      TEXT NOT NULL,
    detail       TEXT,
    status       cx_issue_status NOT NULL DEFAULT 'open',
    source       TEXT NOT NULL DEFAULT 'personal_manager',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at  TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_cx_issues_status   ON cx_issues(status);
CREATE INDEX IF NOT EXISTS idx_cx_issues_severity ON cx_issues(severity);
CREATE INDEX IF NOT EXISTS idx_cx_issues_category ON cx_issues(category);
CREATE INDEX IF NOT EXISTS idx_cx_issues_created  ON cx_issues(created_at);
