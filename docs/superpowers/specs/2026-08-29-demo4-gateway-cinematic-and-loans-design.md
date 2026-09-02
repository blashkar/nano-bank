# Demo 4 revamp: gateway cinematic + loan-aware personal manager

Date: 2026-08-29
Status: approved for planning

## Context

`demos/04-external-agent/` is the "external mandated agent" demo: an
autonomous LLM agent operates a customer's bank *only* through the branch's
`/agent-gateway/*` (mandate-gated, capped, revocable). Today it's a single
Streamlit page — a mandate panel, an instruction box, a "Run agent" button,
and a stacked left/right chat transcript.

The C-suite demos (`05-coo` .. `10-ceo`) got a two-layer presentation
treatment: a Streamlit stepper console (`present/app.py`) plus, for the CEO,
a standalone animated cinematic (`present/boardroom.html`) built from
captured recordings. The user wants demo 4 to get comparable visual
treatment, explicitly as a **split-screen** (not a round table — demo 4 is a
two-party exchange, external agent vs. personal manager, mediated by the
gateway, not a five-seat board), with a distinct color language per side.

Separately: nano-bank now has a real loans product (`/api/v1/loans` —
apply/disburse/repay, PR #23) but the personal manager has no visibility
into it — no MCP tool, no skill. The user wants the manager to meaningfully
handle "I want to buy a car" — explain terms, and (confirmed) actually let
the customer originate the loan through the same confirm-gated propose/
execute flow used for transfers.

These are two independent pieces of work; the loan piece is what the demo's
new "message" beat showcases.

## Approach

### A. Visual revamp — split-screen gateway cinematic

Reuse the C-suite presentation *pattern* (capture → recording JSON → build
script inlines it into a static HTML page; a Streamlit stepper for
live/replay), not its round-table *mechanic*. Demo 4's natural unit is
already the `events` list `ExternalAgent.run()` produces — `plan`, `act`,
`message`, `result` — so there's no need to invent a "beats" abstraction;
the recording *is* that events list.

**Stage layout** (`gateway.html`):
- Left panel — 🛰️ **External Agent**. An "outsider" palette, distinct from
  nano-bank's brand (e.g. violet/slate), because this agent explicitly never
  touches the bank directly — visually it should read as *foreign*.
- Right panel — 🏦 **Personal Manager**. The nano-bank brand palette lifted
  from `boardroom.template.html`'s `:root` vars (`--ink`, `--cfo` teal,
  `--ceo` amber, glass panels) so it visually matches the existing C-suite
  work.
- Center **gateway rail** — a thin vertical conduit between the two panels.
  For each `act` event it shows the mandate decision as a lit pill: 🟢
  allow / 🔴 deny / 🟡 pending_approval (over cap). For each `message` event
  it animates a hand-off arrow left→right (question) and right→left
  (answer) through the rail, labelling it "A2A".
- Playback mechanics reused from the boardroom: typewriter-reveal speech
  bubbles, ▶ Convene/Pause (Space), ⏮/⏭ step (arrows), speed slider,
  progress scrubber, click-to-read-full overlay. `prefers-reduced-motion`
  still disables animation.
- A `plan` event renders as the external agent's opening card (the
  instruction). A `result` event renders as a closing summary card.

**Constraint discovered during planning:** `./nb up --demo 04-external-agent`
(`nb:189`) hardcodes `streamlit run demos/$name/app.py` for every demo in its
`01-onboarding|02-simulator|03-manager-chat|04-external-agent` bucket, and
`nb:167-178` special-cases demo 4's *dependency resolution* (installs
`agent/requirements.txt` into its venv, not the demo's own thin one) but
still launches `demos/04-external-agent/app.py` by that exact path. Moving
the console into `present/app.py` (the C-suite demos' layout) would silently
break that command. So demo 4's restyle stays **in place** at
`demos/04-external-agent/app.py` — same path, same behavior (one in-process
`ExternalAgent.run()` per click, mandate seed/revoke unchanged) — just
restyled into a two-tone nav+centre stepper, and now also saving each run as
a recording so the cinematic can replay it later. `present/` holds only the
cinematic-specific, capture-and-replay pieces, following the C-suite
`present/` *pattern* without literally relocating the live app.

**New files:**
- `demos/04-external-agent/present/state.py` — `read_jsonl`,
  `save_recording(dir_, events)`, `load_recording`, `latest_recording`
  (same generic shape as the other `state.py` helpers, adapted to a plain
  `events` list — no beat-catalog parsing, since there's no `BEATS` list to
  introspect), plus a small `decision_style(decision)` mapping
  (`allow`/`deny`/`pending_approval` → label + color) that both `app.py`
  and the cinematic use for the gateway's decision badge.
- `demos/04-external-agent/present/capture.py` — demo 4 has no
  `run-demo.sh`/`drive.py` today (its run is a direct in-process
  `ExternalAgent.run()` call from Streamlit, not a driver script against a
  deployed service). This script exists solely for `gateway_server.py`'s
  headless capture (there's no Streamlit session for the server to run
  inline in): it optionally seeds the mandate (`--no-seed` to reuse an
  existing one), builds the planner LLM, calls `agent.run(instruction)`,
  and writes the resulting events to `--emit-jsonl PATH` (one run is a
  handful of HTTP calls and returns in well under a second, so — unlike the
  officer demos' multi-minute debates — this writes in one batch at the
  end, not progressively).
- `demos/04-external-agent/present/gateway.template.html` +
  `build_gateway.py` — the static cinematic template + inliner, same
  mechanism as `build_boardroom.py` (marker-replace a JS const with the
  captured JSON — here `EVENTS`, the recording's raw event list, needing no
  beats-shaped transform).
- `demos/04-external-agent/present/gateway_server.py` — serves the
  `present/` dir statically plus `POST /api/capture` /
  `GET /api/capture/status`, running `capture.py` in a background thread
  and rebuilding `gateway.html` on success — mirrors `boardroom_server.py`,
  minus its multi-session (meeting/debate/build) plumbing since demo 4 has
  one recording, not three.
- `demos/04-external-agent/present/recordings/` — captured JSON,
  `.gitignore`'d like the others.

`app.py` itself gets a nav+centre stepper (nav = one button per event,
centre = the selected event's card, both sides colored per the stage
palette below) in place of the current stacked transcript; the mandate
panel/Seed/Revoke controls stay as they are today. A separate "mandate
status" rail isn't added — the existing top banner already covers it, and
duplicating it would be redundant with a 2-actor (not 5-officer) layout.

**Not building**: a second recording/tab for the "revoke then re-run"
interaction shown in the live app. That stays a live-only interaction in
`app.py` (via the existing Seed/Revoke buttons); the cinematic replays one
captured run.

### B. Loan-aware personal manager (informational + apply)

The manager gains full visibility into loans and can originate one through
the same confirm-gated propose/execute pattern already used for transfers —
propose happens on the LLM's own initiative (reachable via the external
agent's A2A `message`, same as any other `propose_*` tool today); **execute
is confirm-path only**, exactly like every other proposal — the external
agent cannot itself execute it, only the actual customer can confirm it (via
the existing manager console). That asymmetry is a good demo beat in its
own right: the agent can inform and propose on the customer's behalf, but
committing new debt still needs the human.

**Read side**
- `agent/db.py`: add `loans(customer_id)` — mirrors `cards()`:
  `SELECT loan_id, account_id, principal_amount, interest_rate, amortization_months, monthly_payment, status, next_payment_date FROM loans WHERE customer_id = %s ORDER BY created_at DESC`.
- `agent/mcp_server.py`: add `get_loans()` tool mirroring `get_cards()`.

**Propose/execute side**
- `agent/bank.py`: add `apply_for_loan(token, principal_amount, interest_rate, amortization_months)` (`POST /api/v1/loans`) and `disburse_loan(token, loan_id)` (`POST /api/v1/loans/{id}/disburse`).
- `agent/actions.py`:
  - `_KINDS` gains `"loan"`.
  - `PendingAction` gains two optional fields: `interest_rate: Optional[str]`,
    `amortization_months: Optional[int]`.
  - `propose(..., kind="loan", amount=principal_amount, interest_rate=..., amortization_months=...)`:
    - **skips** the ownership checks (no account exists yet — it's created
      on apply) and the `from_account`/`to_account` requirement.
    - **does not** check `amount > self.max` (`ACT_MAX_PER_TX`, default
      $1000) — that cap models a single money-movement transaction, not
      loan origination, and a car loan will routinely exceed it. Instead
      it's checked against a new, loan-specific ceiling:
      `Settings.loan_max_principal` (env `LOAN_MAX_PRINCIPAL`, default
      `100000`), to keep a responsible-lending-style upper bound without
      conflating it with the transfer cap.
    - validates `0 <= interest_rate <= 1` and `amortization_months > 0`
      the same way the Rust handler does (fail fast with `ActDenied` rather
      than a round trip that 400s).
  - `execute(..., kind="loan")`: calls `bank.apply_for_loan(...)`, then
    immediately `bank.disburse_loan(token, loan_id)` — nano-bank's lending
    is servicing-only (no underwriting step to wait on), so apply+disburse
    happen as one confirmed customer action, landing the principal in the
    customer's chequing account. Returns
    `{"loan": <apply response>, "disbursement": <disburse response>}`.
    **Known limitation** (documented in a code comment, not solved here):
    `POST /api/v1/loans` has no idempotency-key support server-side (unlike
    transfers), so a transport retry of `execute()` after `apply_for_loan`
    succeeds but before `disburse_loan` returns could in principle create a
    second loan on a raw retry. Out of scope — would require an API change
    in `loans.rs`; acceptable for a demo/servicing-only product.
  - `_summary()`: `"Apply for a car loan: principal $X over N months at R.RR% APR (~$M.MM/mo)"`.
- `agent/config.py`: add `loan_max_principal: Decimal` (env
  `LOAN_MAX_PRINCIPAL`, default `"100000"`).
- `agent/mcp_server.py`: add
  `propose_loan_application(principal_amount: str, interest_rate: str, amortization_months: int) -> dict`,
  mirroring `propose_transfer`. Both new tools (`get_loans`,
  `propose_loan_application`) must be added to `LLM_TOOL_NAMES` — that
  frozenset is the actual gate `nano_manager.agent_tools()` filters
  against; `execute_action`/`cancel_action` stay confirm-path only by
  their absence from it (`CONFIRM_ONLY_TOOL_NAMES`), which is the
  mechanism the "propose but can't execute via A2A" claim below relies on.

**Skill**
- `agent/skills/loan.md` — new `kind: product`, `product: loan` skill
  (auto-tagged `[held]`/`[available — not held]` by the existing
  `build_skill_menu`, since `accounts()` already returns `account_type =
  'loan'` rows once one exists). Content: call `get_loans()` first: if the
  client already holds one, report its terms/status instead of proposing a
  new one. For a car-purchase conversation with no existing loan: give a
  concrete illustrative rate band (**6.99%–9.99% APR**) and term band
  (**36–84 months**, default **60**) for an auto loan, reason about
  affordability from the client's accounts/transactions, state the
  estimated monthly payment for the client's stated car price before
  proposing anything, then call `propose_loan_application` — and always
  tell the client the application needs their own confirmation in the app
  (execute is confirm-path only; the agent can't do that step itself).

**Demo instruction**: update `04-external-agent/app.py`'s (and
`present/capture.py`'s) default instruction from the current
bill-payment-only text to
`"Pay my $50 Epcor utility bill and tell me what a loan would look like if I want to buy a $28,000 car."`
— one `act` (bill pay, exercises the mandate/gateway side) plus one
`message` (exercises the new loan skill on the manager side), giving the
cinematic one of each event type to show off.

## Testing

- `agent/tests/`: unit tests for `ActionStore` kind `"loan"` — propose
  rejects a non-owned-account-style path N/A (no accounts involved),
  rejects `amount > loan_max_principal`, rejects invalid
  `interest_rate`/`amortization_months`, and `execute()` calls
  `apply_for_loan` then `disburse_loan` against a fake `bank`. Unit tests
  for `db.loans()` and the `get_loans`/`propose_loan_application` MCP
  tools (fakes, matching the existing `get_cards`/`propose_transfer` test
  style).
- `present/state.py` (or the reused helpers): existing `state.py` tests
  already cover `read_jsonl`/`save_recording`/`load_recording` generically;
  add a demo-4-specific test only if the adaptation isn't a pure
  re-export.
- No Rust changes — `loans.rs` is untouched.
- Manual verification: run `present/capture.py` against the live stack,
  confirm `gateway.html` builds and plays; run `app.py` live once to
  confirm the loan proposal round-trips through the manager (propose
  succeeds, execute is correctly refused when attempted from the A2A
  path — only reachable via the confirm-path console).
