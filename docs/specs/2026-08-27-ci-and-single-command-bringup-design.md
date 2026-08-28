# PR test-gate CI + single-command stack/demo bring-up

Date: 2026-08-27
Status: Approved (design)

## Problem

Two unrelated gaps, bundled because they were raised together:

1. There is no CI. PRs merge with nothing verifying they build or pass tests.
   A CI workflow was tried once (PRs #82/#83, 2026-08-21) and fully reverted
   (PR #85) after `clippy --all-targets -D warnings` turned pre-existing lint
   debt into hard failures, and a `testing/mock_ledger.py` stand-in couldn't
   satisfy what `api/tests/finance.rs` actually needs (a real ledger core).
2. Standing up the stack or a demo means chaining several scripts by hand
   (`scripts/deploy-all.sh`, then a port-forward, then a demo-specific
   command from its README) or reading `demos/README.md` per demo. There is
   one command for "deploy everything" but none for "deploy everything *and*
   populate it with world-model data" or "deploy everything *and* launch demo
   N", and no single teardown that matches the current two-cluster topology
   (`scripts/stop-nano-bank.sh` still assumes the old single-cluster / `cargo
   run` layout from before `deploy-all.sh` existed).

## Goals

- A GitHub Actions workflow that runs on every push/PR to `main`, builds and
  tests the Rust API against a *real* modern-core (not a stub), and reports
  (not blocks on) lint findings — fixing both root causes of the August
  revert rather than re-trying the same workflow.
- One local command, `./nb`, that replaces the scattered scripts as the way
  to bring the stack up:
  - `./nb up --world-model [--scenario <name>]` — full stack, then realize
    the world-model dataset against it.
  - `./nb up --demo <NN-name>` — full stack, then launch that demo.
  - `./nb down` — tear down everything the two modes above can start.
  - `./nb list-demos` — print the demo catalogue.

## Non-goals

- No self-hosted runner, no GitHub-triggered bring-up. `./nb` is a local CLI
  only (confirmed in brainstorming — this is the "single command" ask, not a
  CD target).
- No CRM bring-up (its nano-bank integration is unbuilt/parked — see
  `nano-bank-crm-cleanroom` memory) and no separate "fraud" mode (fraud lives
  inside the bank itself and comes up whenever the bank does).
- No Playwright e2e in the automatic PR gate — it needs the full k8s stack,
  which doesn't belong on a GitHub-hosted runner. `scripts/e2e-ui.sh` stays a
  manual/local step.
- No branch-protection change bundled into this work. Once the workflow
  exists and has a green run, marking it "required" in `main`'s branch
  protection is a separate, explicit step the user takes (or asks for).

## Component 1 — `.github/workflows/ci.yml`

Runs on GitHub-hosted `ubuntu-latest`. Two independent jobs, both on
`push: [main]` and `pull_request`.

### Job `rust`

1. Checkout `nano-bank`.
2. Checkout `nano-bank-modern-core` as a sibling checkout (`actions/checkout`
   with `repository: bvcmartins/nano-bank-modern-core`, `path:
   nano-bank-modern-core`) — `origin` for both repos is `bvcmartins`, even
   though `nano-bank` itself also has `arunscape`/`blashkar`/`tgoyal` remotes
   from collaborators.
3. Postgres service container for nano-bank's own DB: image `postgres:16-alpine`,
   `POSTGRES_DB=nano_bank_db`, `POSTGRES_USER=nanobank_user`,
   `POSTGRES_PASSWORD=secure_nano_password_2024!` (matches
   `api/config/default.toml`), port `5432:5432`, with a `pg_isready`
   healthcheck.
4. Load schema: `psql -h 127.0.0.1 -U nanobank_user -d nano_bank_db -f
   <file>` for every `src/core/tables/*.sql` in sorted order.
5. Bring up the **real** modern core (not a mock): `cd
   nano-bank-modern-core && docker compose up -d --build` — this starts its
   own Postgres (`modern-core-db`, port 5435) and the `app` service on
   `:8091`, `depends_on: db: condition: service_healthy` already wired in its
   `docker-compose.yml`. Wait for `curl -fsS localhost:8091/health`.
6. `cargo fmt --all -- --check` (working directory `api`) — blocking.
7. `cargo clippy --all-targets` (no `-D warnings`) — runs and its output is
   visible in the log, but the step does not fail the job. This is the fix
   for the August failure: clippy is a report, not a gate, until the
   existing lint debt is deliberately paid down.
8. `cargo build --all-targets` — blocking.
9. Start the bank API: `CORE_BACKEND=modern MODERN_CORE_URL=http://localhost:8091
   NANO_BANK__DATABASE__HOST=127.0.0.1 cargo run &`, wait for
   `curl -fsS localhost:8081/health`. (`127.0.0.1` here, not the usual `::1` —
   the CI Postgres service is only reachable over IPv4, same override the
   original attempt used.)
10. `cargo test -- --nocapture` (working directory `api`) with
    `NANO_BANK_TEST_DB_URL=postgres://nanobank_user:secure_nano_password_2024!@127.0.0.1:5432/nano_bank_db`
    (and the same `CORE_BACKEND`/`MODERN_CORE_URL`/`NANO_BANK__DATABASE__HOST`
    as step 9) — blocking. This is the step that exercises `finance.rs`
    against the live services from steps 5 and 9.

### Job `ui`

1. Checkout `nano-bank`.
2. `actions/setup-node@v4` with `node-version: 20`.
3. `cd ui && npm ci`.
4. `npm run typecheck` — blocking.
5. `npm run lint` — blocking (ESLint, unlike clippy, has no known pre-existing
   debt problem here; revisit if it turns out to).
6. `npm test` (`vitest run`) — blocking.

No Postgres, no kind, no Playwright in this job — it only needs Node.

## Component 2 — `./nb` (repo root, bash)

Single dispatcher script, executable, replacing
`scripts/{start,stop,setup-k8s,deploy-all}*.sh` and demo-specific README
instructions as the thing a user actually types. Internally it still calls
those existing scripts — it's an orchestration layer, not a rewrite of them.

```
./nb up --world-model [--scenario <name>]
./nb up --demo <NN-name>
./nb down
./nb list-demos
```

### `./nb up --world-model`

1. `./scripts/deploy-all.sh` (idempotent — already checks for an existing
   cluster before creating one).
2. Port-forward the bank API if not already forwarded:
   `kubectl --context kind-nano-bank -n nano-bank port-forward svc/bank-api
   8081:8081` in the background, tracked by a pidfile under
   `/tmp/nb-*.pid` so `./nb down` can find and kill it.
3. For every `.yaml` in `../nano-bank-world-model/scenarios/` (or just the
   one named by `--scenario`): `uv run world-model realize --scenario
   <file> --bank-url http://localhost:8081 --run-tag nb-$(date
   +%s)-<basename>` from the `nano-bank-world-model` checkout, capturing
   each run's exit code (`0` only when the GL reconciles).
4. Print a per-scenario pass/fail summary; exit non-zero if any scenario
   failed to reconcile.
5. Leaves the stack (and the port-forward) running — this is a bring-up, not
   a test run; `./nb down` tears it back down when the user is done.

### `./nb up --demo <NN-name>`

1. `./scripts/deploy-all.sh`.
2. Validate `<NN-name>` against the `demos/*/` directory list (same names
   `./nb list-demos` prints); fail fast with the valid list on a typo.
3. Dispatch by demo shape:
   - **Narrated agent demos** (currently `05-coo`, `06-cfo`, `08-cto`,
     `09-cxo`, `10-ceo` — any demo with its own `run-demo.sh`): invoke
     `demos/<name>/run-demo.sh --no-up` (the stack is already up from step
     1; these scripts already manage their own port-forwards internally, as
     `08-cto/run-demo.sh` does today).
   - **Plain Streamlit demos** (`01-onboarding`, `02-simulator`,
     `03-manager-chat`, `04-external-agent`, `07-suite-console`): a small
     per-demo table drives what each one needs — which service(s) to
     port-forward (bank API `:8081` for 01/02; branch/agent API `:8086` for
     03/04, including the `BRANCH_SERVICE_TOKEN` read from `agent/.env`;
     07 additionally needs `coo/k8s/deploy.sh` + `cfo/k8s/deploy.sh` applied
     first per its README), which env var carries the base URL
     (`DEMO_API_BASE` / `DEMO_BRANCH_BASE` etc.), and its
     `requirements.txt`. The exact table is enumerated in the implementation
     plan by reading each demo's own README (04's is currently empty — its
     exact requirements get confirmed against `demos/04-external-agent/app.py`
     during implementation) rather than guessed here.
   - Backgrounds `streamlit run demos/<name>/app.py --server.headless
     true`, health-checks the port, prints the URL, and returns — leaves it
     running, same as world-model mode.

### `./nb down`

- Kills every backgrounded process tracked by an `/tmp/nb-*.pid` (port-forwards,
  Streamlit servers).
- `kind delete cluster --name nano-bank` and `kind delete cluster --name
  modern-core`.
- Replaces `scripts/stop-nano-bank.sh` / `stop-nano-bank.sh`, which predate
  `deploy-all.sh`'s two-cluster topology and don't tear either cluster down
  correctly today.

### `./nb list-demos`

Prints the 10 `demos/NN-*` names with the one-line description already in
`demos/README.md`'s table, so there's no need to open the README to remember
a demo's name.

## Testing / verification

- **CI workflow**: verified by pushing a branch and watching the Actions run
  go green (both jobs), then confirming a deliberately broken `cargo test`
  (or `npm test`) on a throwaway branch actually fails the respective job.
  Clippy findings are confirmed to *not* fail the job even when present.
- **`./nb up --world-model`**: run against a torn-down state, confirm both
  clusters come up, both scenarios (`hero.yaml`, `corpus-measure.yaml`)
  realize with exit code 0, and the printed summary reflects that.
- **`./nb up --demo <name>`**: spot-check one narrated demo (e.g. `08-cto`)
  and one plain Streamlit demo (e.g. `02-simulator`); confirm the printed URL
  is reachable and the right service is actually behind it.
- **`./nb down`**: run after each of the above, confirm `kind get clusters`
  shows neither cluster and no `nb-*` pidfile processes remain.

## Risks / trade-offs

- **Two Postgres instances in one CI job** (nano-bank's own + modern-core's
  `docker compose` one on 5435) adds a bit of job startup time but keeps
  each service's data isolated exactly as it is in local dev — no shared-DB
  shortcuts that could mask a real cross-service bug.
- **Clippy as report-only** means lint debt can keep accumulating unblocked.
  Accepted deliberately (this is exactly what caused the prior revert);
  revisit turning it into a gate once the existing debt is paid down as its
  own piece of work.
- **The plain-Streamlit demo table** is the one part of `./nb` not fully
  specified here (04's requirements in particular need confirming against
  its code, since its README is empty). Flagged explicitly rather than
  guessed, to be resolved in the implementation plan.
