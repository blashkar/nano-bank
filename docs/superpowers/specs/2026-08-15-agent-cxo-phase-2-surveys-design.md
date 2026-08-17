# Agent CXO — Phase 2: surveys (NPS + CSAT)

**Status:** design approved 2026-08-15 (brainstorming). Extends Phase 1
(`2026-08-15-agent-cxo-phase-1-design.md`) with proactive Voice-of-Customer
measurement. Builds on the `cx` metrics service + the `cxo` analyst seat.

## Goal

Give the CXO a **proactive** customer-experience signal to complement Phase 1's
behavioural signals and reactive complaints: **survey campaigns** measuring **NPS**
(0–10 loyalty) and **CSAT** (1–5 satisfaction). Campaigns are created
deterministically (seed/demo/operator), responses are **simulated** with a
behavioural correlation (so detractors cluster where friction/issues are), and the
CXO folds the grounded NPS/CSAT into its posture and ranked feature backlog.

## Non-goals (Phase 2)

- **No autonomous survey agent.** Campaigns are created by a deterministic runner
  triggered externally; nothing launches a campaign on its own. (A future Phase 3
  could add an audited cxm acting loop.)
- **No live personal-manager end-to-end** (still deferred — a separate track).
- **No real delivery.** There is no email/SMS/UI; responses are simulated, exactly
  as the seeded `cx_issues` were in Phase 1.
- **CXO stays analyst-only** — it reads survey results; it does not create campaigns.

## Decisions (locked in brainstorming)

| Question | Decision |
|---|---|
| Instruments | **NPS + CSAT** (two complementary industry-standard scales). |
| Campaign creation | **Deterministic runner** (seed/demo/operator); simulated responses correlated with behaviour. cxm stays a metrics/runner service. |
| Scope | **Surveys only**; live-PM deferred. |
| Response storage | One generic `survey_responses` table; the instrument lives on the campaign. |

## Architecture

Everything lands in the existing `cx` service (mirrors how Phase 1 added metrics);
the CXO picks up the new tools automatically through its MCP client — only its
prompt changes.

```
 seed/demo/operator ── create_campaign(instrument, segment, question) ─► cx/campaigns.py
                                                                          │ resolve segment (bank data)
                                                                          │ simulate_score per target
                                                                          │   (sentiment-correlated, seeded)
                                                                          ▼
                                            survey_campaigns / survey_responses (bank PG)
                                                                          │
   cxo (analyst) ── MCP ──►  cx MCP tools: nps_score, csat_score,  ◄──────┘  (pure nps()/csat()
                             survey_results, list_campaigns, cx_summary        aggregators)
```

### 1. Data model — `src/core/tables/11_cx_surveys.sql`

```
enum: survey_instrument ('nps' | 'csat')

survey_campaigns(
  id          UUID PK default gen_random_uuid(),
  instrument  survey_instrument NOT NULL,
  segment     TEXT NOT NULL,               -- named target segment (resolved by the runner)
  question    TEXT NOT NULL,
  status      TEXT NOT NULL DEFAULT 'open',-- 'open' | 'closed'
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
)

survey_responses(
  id          UUID PK default gen_random_uuid(),
  campaign_id UUID NOT NULL REFERENCES survey_campaigns(id),
  customer_id UUID NOT NULL REFERENCES customers(customer_id),
  score       INT NOT NULL,                -- NPS 0–10, CSAT 1–5
  comment     TEXT,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
)
+ indexes on survey_responses(campaign_id), survey_campaigns(instrument)
```

One generic responses table keeps it DRY; aggregation keys off the campaign's
`instrument`. `score` is the raw scale value (validated by the runner, not a DB
CHECK, since the two instruments have different ranges).

### 2. Segments

Named segments the runner resolves to a set of `customer_id`s from the bank data
(reusing Phase-1 SQL patterns — `transactions.initiated_by`, `cx_issues`):

- `all_active` — transacted within the window.
- `product:<card|deposit|payment|interac>` — used that product in the window.
- `has_open_issue` — has an open `cx_issue`.
- `dormant` — no transaction within the window.

### 3. The campaign runner — `cx/campaigns.py`

- `resolve_segment(db, segment, window_days) -> list[str]` — one SQL per segment.
- `customer_sentiment(db, window_days) -> dict[str, int]` — per customer: **−1**
  if they have an open `cx_issue` or are dormant, **+1** if active with no issue,
  else **0**. (Bulk-computed once per campaign.)
- `simulate_score(instrument, sentiment, rng) -> int` — **pure**, deterministic
  given a seeded `rng`. Weighted buckets:
  - **NPS:** sentiment<0 → mostly 0–6 (detractors); >0 → mostly 9–10 (promoters);
    0 → 6–9.
  - **CSAT:** sentiment<0 → 1–3; >0 → 4–5; 0 → 3–4.
- `create_campaign(db, instrument, segment, question, seed=7, window_days=30) -> dict`
  — insert the campaign, resolve targets, compute sentiment, simulate one response
  per target with an rng seeded by `(seed, campaign_id or index)`, bulk-insert
  `survey_responses`, return `{campaign_id, instrument, segment, responses, score}`.

### 4. Aggregators — `cx/metrics.py` (pure, unit-tested)

- `nps(scores) -> {responses, promoters, passives, detractors, score}` — promoter
  9–10, passive 7–8, detractor 0–6; `score = round(%promoters − %detractors)`.
- `csat(scores) -> {responses, satisfied, csat_rate, mean}` — satisfied = score ≥ 4.

### 5. cx MCP tools + reads

`cx/db.py` gains: `campaigns() -> list[dict]`, `survey_scores(campaign_id=None,
instrument=None) -> list[int]`. `cx/mcp_server.py` gains tools:

- `list_campaigns()` — campaigns with instrument/segment/status + response count.
- `nps_score(campaign_id: str = "")` — `nps()` over that campaign (or all NPS).
- `csat_score(campaign_id: str = "")` — `csat()` over that campaign (or all CSAT).
- `survey_results(campaign_id: str = "")` — a combined summary per campaign
  (instrument, segment, responses, score).
- `cx_summary` gains a `surveys` block (latest NPS + CSAT headline).

### 6. CXO integration

The CXO's tools come from the cx MCP client, so the survey tools are available with
**no `cxo` code change except the prompt**. `CXO_PROMPT` is extended to: read NPS +
CSAT as first-class CX signals, describe the promoter/detractor split, and fold
detractor clusters (by segment) into the ranked feature backlog.

### 7. Demo — `demos/09-cxo/`

- `cx/seed_surveys.py` — creates two campaigns and simulates responses: **NPS on
  `all_active`** and **CSAT on `product:interac`** (which correlates with the seeded
  `rail_experience` issues → low CSAT, a coherent story).
- `run-demo.sh` — adds a survey-seed step after the `cx_issues` seed.
- `drive.py` — one new beat between the customer-voice and backlog beats: *"What's
  our NPS, and where do detractors cluster?"* The backlog beat then folds the
  survey signal.

## Testing

- **`nps` / `csat`** — pure aggregation over fixtures (bucketing + score formula,
  incl. empty-scores guard).
- **`simulate_score`** — determinism (same seed → same result) and directional
  correlation (a negative-sentiment sample scores lower on average than a
  positive-sentiment sample) over a seeded batch.
- **`resolve_segment` / `create_campaign`** — end-to-end against the seeded live DB
  (a known campaign yields a deterministic response count + a score in range).
- **`seed_surveys`** — creates the two campaigns; `list_campaigns` returns them.
- **CXO prompt** — asserts it mentions NPS/CSAT/surveys.
- **One live smoke** — seed surveys → CXO NPS/CSAT posture grounded → backlog folds
  the detractor cluster.

## Ports & names

No new deployables — Phase 2 extends the `cx` MCP (`:8097`) and the `cxo` seat
(`:8098`). New DDL `src/core/tables/11_cx_surveys.sql`.

## Phase line

- **Phase 2 (this spec):** NPS + CSAT tables, the deterministic campaign runner +
  correlated simulation, the survey aggregators + cx tools, CXO prompt integration,
  `seed_surveys`, and the demo beat.
- **Phase 3 (future):** an autonomous survey-campaign agent (cxm decides + launches,
  audited) and the live personal-manager end-to-end.

## Open provisioning steps (one-time)

1. Load `src/core/tables/11_cx_surveys.sql` into the bank DB.
2. Rebuild + roll out `cx-mcp` and `cxo` (new tools + prompt) via `cxo/k8s/deploy.sh`.
