# PR test-gate CI + single-command stack/demo bring-up — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a GitHub Actions CI workflow that actually builds/tests nano-bank
against a real modern-core, and a single local `./nb` command that replaces the
scattered scripts for bringing up the full stack (with world-model data) or any
one demo.

**Architecture:** Two independent deliverables, one repo. (1)
`.github/workflows/ci.yml` — a `rust` job (real Postgres service container +
real `nano-bank-modern-core` via its own `docker compose`, fmt/clippy/build/test)
and a `ui` job (Node-only: typecheck/lint/vitest). (2) `./nb` — one bash
dispatcher script at the repo root with four subcommands
(`up --world-model`, `up --demo <name>`, `down`, `list-demos`) that orchestrates
the *existing* scripts (`scripts/deploy-all.sh`, each demo's own runner) rather
than reimplementing them.

**Tech Stack:** GitHub Actions (`ubuntu-latest`), bash, the existing Rust/axum
API, Next.js UI, Python demos, `uv`-managed `nano-bank-world-model` CLI, kind/kubectl.

**Spec:** `docs/specs/2026-08-27-ci-and-single-command-bringup-design.md`

## Global Constraints

- Clippy is **report-only** — `cargo clippy` must run and its output must be
  visible in the CI log, but must never fail the job (root cause of the prior
  revert, PR #85).
- CI must exercise a **real** `nano-bank-modern-core` (its own `docker compose
  up -d --build`), never a mock/stub ledger (the other root cause of PR #85).
- No CRM bring-up, no separate "fraud" mode — fraud lives inside the bank and
  comes up automatically; CRM integration is unbuilt/parked.
- No self-hosted runner, no GitHub-triggered bring-up — `./nb` is a local CLI
  only.
- No Playwright e2e in the CI gate — `scripts/e2e-ui.sh` stays manual.
- No branch-protection changes.
- `./nb` orchestrates existing scripts; it does not reimplement
  `deploy-all.sh`, `k8s/deploy.sh`, or any demo's own runner.

## Live-verification caveat for this session

The user has confirmed Ollama-cloud credits are exhausted until tomorrow, so
**anything that calls an LLM cannot be live-verified today**: demos
`03-manager-chat`, `04-external-agent`, `05-coo`, `06-cfo`, `07-suite-console`,
`08-cto`, `09-cxo`, `10-ceo` all drive an agent that calls the model. Build
those paths correctly against the actual code (confirmed by reading each
demo's `app.py`/`run-demo.sh` below) and verify the *mechanical* parts today
(stack comes up, port-forward succeeds, the process starts and reaches the
point of an HTTP/model call) — defer confirming an actual agent response to a
follow-up session. Everything else in this plan (the CI workflow, `./nb down`,
`./nb list-demos`, `./nb up --world-model`, and `./nb up --demo` for
`01-onboarding`/`02-simulator`) has no LLM dependency and is fully
live-verifiable today.

---

## File Structure

- Create: `.github/workflows/ci.yml` — the two-job CI workflow.
- Create: `nb` (repo root, executable, `chmod +x`) — the single dispatcher
  script. One file, matching the spec ("single dispatcher script... an
  orchestration layer, not a rewrite of them") and the size of the tool (four
  subcommands, all thin wrappers around existing scripts) — splitting it into
  multiple files would be over-structuring a ~200-line bash CLI.

No other files are modified. `scripts/stop-nano-bank.sh` and
`scripts/start-nano-bank.sh` are left in place, untouched — `./nb down`
supersedes the former for anyone using `./nb`, but deleting old scripts is a
separate cleanup decision outside this plan's scope.

---

### Task 1: CI workflow — `rust` job

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Produces: a GitHub Actions workflow file with a `rust` job. Task 2 appends a
  sibling `ui` job to the same file.

- [ ] **Step 1: Write the workflow file with the `rust` job**

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  rust:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:16-alpine
        env:
          POSTGRES_DB: nano_bank_db
          POSTGRES_USER: nanobank_user
          POSTGRES_PASSWORD: secure_nano_password_2024!
        ports:
          - 5432:5432
        options: >-
          --health-cmd "pg_isready -U nanobank_user -d nano_bank_db"
          --health-interval 5s
          --health-timeout 3s
          --health-retries 10
    steps:
      - name: Checkout nano-bank
        uses: actions/checkout@v4

      - name: Checkout nano-bank-modern-core
        uses: actions/checkout@v4
        with:
          repository: bvcmartins/nano-bank-modern-core
          path: nano-bank-modern-core

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Cache cargo
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: api

      - name: Load nano-bank schema
        run: |
          for f in src/core/tables/*.sql; do
            echo "applying $f"
            PGPASSWORD=secure_nano_password_2024! psql -h 127.0.0.1 -U nanobank_user -d nano_bank_db -f "$f"
          done

      - name: Bring up real modern core
        working-directory: nano-bank-modern-core
        run: docker compose up -d --build

      - name: Wait for modern core
        run: |
          for i in $(seq 1 60); do
            curl -fsS http://localhost:8091/health && exit 0
            sleep 2
          done
          echo "modern-core never became healthy"
          exit 1

      - name: cargo fmt --check
        working-directory: api
        run: cargo fmt --all -- --check

      - name: cargo clippy (report only, does not fail the job)
        working-directory: api
        run: cargo clippy --all-targets || true

      - name: cargo build
        working-directory: api
        run: cargo build --all-targets

      - name: Start bank API
        working-directory: api
        env:
          CORE_BACKEND: modern
          MODERN_CORE_URL: http://localhost:8091
          NANO_BANK__DATABASE__HOST: 127.0.0.1
        run: |
          cargo run > /tmp/bank-api.log 2>&1 &
          echo $! > /tmp/bank-api.pid
          for i in $(seq 1 60); do
            curl -fsS http://localhost:8081/health && exit 0
            sleep 2
          done
          echo "bank-api never became healthy"
          cat /tmp/bank-api.log
          exit 1

      - name: cargo test
        working-directory: api
        env:
          CORE_BACKEND: modern
          MODERN_CORE_URL: http://localhost:8091
          NANO_BANK__DATABASE__HOST: 127.0.0.1
          NANO_BANK_TEST_DB_URL: postgres://nanobank_user:secure_nano_password_2024!@127.0.0.1:5432/nano_bank_db
        run: cargo test -- --nocapture

      - name: Stop bank API
        if: always()
        run: kill "$(cat /tmp/bank-api.pid)" 2>/dev/null || true
```

Notes for the implementer:
- `127.0.0.1`, not the usual `::1`, because the CI Postgres *service container*
  is only reachable over IPv4 — this mirrors the override the original
  (reverted) CI attempt used, and is unrelated to the local-dev `::1` gotcha in
  `CLAUDE.md` (that one is about a dead docker-proxy on the kind Postgres, which
  doesn't exist in this GH-hosted service-container setup).
- `CORE_BACKEND=modern` is technically the default already
  (`api/src/main.rs::build_ledger`), but it's set explicitly here for clarity
  and to guard against a future default change.
- `nano-bank`'s tests are a graceful-skip harness (`require_stack!` macros probe
  `GET /health` and skip, still passing, if unreachable) — the "Start bank API"
  step is what makes `cargo test` actually exercise them instead of skipping
  everything silently.

- [ ] **Step 2: Verify YAML is well-formed**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" ` (or any local YAML linter available)
Expected: no error.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add rust job (real Postgres + real modern-core, clippy report-only)"
```

---

### Task 2: CI workflow — `ui` job

**Files:**
- Modify: `.github/workflows/ci.yml` (append the `ui` job under `jobs:`, as a
  sibling of `rust`)

**Interfaces:**
- Consumes: the `jobs:` key created in Task 1.

- [ ] **Step 1: Append the `ui` job**

```yaml
  ui:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: npm
          cache-dependency-path: ui/package-lock.json

      - name: npm ci
        working-directory: ui
        run: npm ci

      - name: typecheck
        working-directory: ui
        run: npm run typecheck

      - name: lint
        working-directory: ui
        run: npm run lint

      - name: test
        working-directory: ui
        run: npm test
```

No Postgres, no kind, no Playwright in this job — it only needs Node
(`ui/package.json` scripts: `typecheck` → `tsc --noEmit`, `lint` → `eslint .`,
`test` → `vitest run`).

- [ ] **Step 2: Verify YAML is still well-formed**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"`
Expected: no error, and the file now has two top-level jobs (`rust`, `ui`).

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add ui job (typecheck, lint, vitest)"
```

---

### Task 3: Verify the CI workflow live

**Files:** none (verification only, against GitHub Actions)

- [ ] **Step 1: Push a throwaway branch with the new workflow**

```bash
git push -u origin ci-and-bringup
```

- [ ] **Step 2: Watch the run**

```bash
gh run watch --exit-status
```

Expected: both `rust` and `ui` jobs finish green. If `rust` fails at "Load
nano-bank schema" or "Start bank API", read the step log — the two most likely
causes are a schema file order issue (tables must apply in the sorted order
already reflected by the `for f in src/core/tables/*.sql` glob) or the
modern-core image failing to build (check the "Bring up real modern core" step
log via `gh run view --log`).

- [ ] **Step 3: Confirm clippy is report-only even with findings**

Run: `cd api && cargo clippy --all-targets 2>&1 | tail -20`
Expected: some pre-existing warnings print (this repo has known lint debt per
the design doc); note whether there are any and confirm the CI step for
clippy still shows green in the Actions log despite them (the `|| true` at the
end of that step is what guarantees this).

- [ ] **Step 4: Confirm a real failure actually fails the job**

On the same branch, temporarily break a test:

```bash
cd api
sed -i.bak '1i #[test]\nfn __ci_verification_failure() { assert!(false, "intentional CI verification failure"); }' tests/accounts.rs
git add tests/accounts.rs
git commit -m "test: intentionally break CI to verify the gate (revert next)"
git push
gh run watch --exit-status || echo "job failed as expected"
```

Expected: the `rust` job fails on the `cargo test` step.

- [ ] **Step 5: Revert the intentional break**

```bash
git revert --no-edit HEAD
git push
```

- [ ] **Step 6: Do the same one-line check for the `ui` job**

```bash
cd ui
echo "throw new Error('intentional CI verification failure')" >> src/lib/redirects.ts.tmp
mv src/lib/redirects.ts.tmp src/lib/redirects.ts
git add src/lib/redirects.ts
git commit -m "test: intentionally break ui CI to verify the gate (revert next)"
git push
gh run watch --exit-status || echo "job failed as expected"
git revert --no-edit HEAD
git push
```

Expected: the `ui` job fails (likely at `typecheck` or `test`), confirming the
gate is real; then the revert restores green.

No commit step here beyond the revert commits above — this task is pure
verification of Tasks 1–2.

---

### Task 4: `./nb` skeleton — `list-demos` and `down`

**Files:**
- Create: `nb` (repo root, `chmod +x`)

**Interfaces:**
- Produces: `DEMO_NAMES` (bash array of the 10 demo directory names),
  `demo_desc(name)` (echoes a one-line description), `is_valid_demo(name)`
  (return code 0/1), `pf(svc, port)` (backgrounds a port-forward, writes
  `/tmp/nb-pf-<svc>.pid`), `wait_http(url, label)`, `cmd_down()`. Tasks 5–6
  consume `pf`, `wait_http`, `is_valid_demo`, and the `/tmp/nb-*.pid`
  convention `cmd_down` cleans up.

- [ ] **Step 1: Write the script skeleton**

```bash
#!/usr/bin/env bash
# ./nb — single entry point for standing up nano-bank: the full stack + world-
# model data, or the full stack + one demo. Wraps the existing
# scripts/deploy-all.sh and each demo's own runner rather than reimplementing
# them.
#
#   ./nb up --world-model [--scenario <name>]
#   ./nb up --demo <NN-name>
#   ./nb down
#   ./nb list-demos
set -euo pipefail
cd "$(dirname "$0")"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1000}"
export XDG_DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"

WORLD_MODEL_DIR="../nano-bank-world-model"

DEMO_NAMES=(01-onboarding 02-simulator 03-manager-chat 04-external-agent \
            05-coo 06-cfo 07-suite-console 08-cto 09-cxo 10-ceo)

demo_desc() {
  case "$1" in
    01-onboarding)     echo "Create a customer, open accounts, post transactions" ;;
    02-simulator)      echo "Auto-generate activity across every transaction type incl. failures" ;;
    03-manager-chat)   echo "Personal manager as a left-right conversation" ;;
    04-external-agent) echo "Autonomous LLM agent operating a customer's bank under a mandate" ;;
    05-coo)            echo "Agent COO -- narrated 7-beat operational review + levers" ;;
    06-cfo)            echo "Agent CFO -- narrated 7-beat financial review + levers" ;;
    07-suite-console)  echo "Single-pane console: drive COO/CFO + watch the live audit ledger" ;;
    08-cto)            echo "Agent CTO -- reliability review, refused restart, autonomous rollback" ;;
    09-cxo)            echo "Agent CXO -- customer-experience analyst + ranked feature backlog" ;;
    10-ceo)            echo "Agent CEO -- synthesizes COO/CFO/CTO/CXO into an executive brief" ;;
    *) echo "" ;;
  esac
}

is_valid_demo() {
  local name="$1" d
  for d in "${DEMO_NAMES[@]}"; do [ "$d" = "$name" ] && return 0; done
  return 1
}

usage() {
  cat <<'EOF'
Usage:
  ./nb up --world-model [--scenario <name>]
  ./nb up --demo <NN-name>
  ./nb down
  ./nb list-demos
EOF
}

cmd_list_demos() {
  local d
  for d in "${DEMO_NAMES[@]}"; do
    printf '%-18s %s\n' "$d" "$(demo_desc "$d")"
  done
}

wait_http() {  # url label
  echo "waiting for $2 ($1) ..."
  for _ in $(seq 1 60); do curl -fsS "$1" >/dev/null 2>&1 && return 0; sleep 1; done
  echo "$2 never came up at $1"
  return 1
}

pf() {  # svc localport -> backgrounds a port-forward, tracked by a pidfile
  local svc="$1" port="$2"
  kubectl --context kind-nano-bank -n nano-bank port-forward "svc/$svc" "$port:$port" \
    >"/tmp/nb-pf-$svc.log" 2>&1 &
  echo $! > "/tmp/nb-pf-$svc.pid"
}

cmd_down() {
  echo "stopping everything ./nb can start ..."
  local f pid
  for f in /tmp/nb-*.pid; do
    [ -f "$f" ] || continue
    pid=$(cat "$f")
    if kill "$pid" 2>/dev/null; then echo "  stopped pid $pid ($f)"; fi
    rm -f "$f"
  done
  if kind get clusters 2>/dev/null | grep -qx "nano-bank"; then
    kind delete cluster --name nano-bank
  fi
  if kind get clusters 2>/dev/null | grep -qx "modern-core"; then
    kind delete cluster --name modern-core
  fi
  echo "down"
}

main() {
  local cmd="${1:-}"
  case "$cmd" in
    up)
      shift
      local mode="${1:-}"
      case "$mode" in
        --world-model)
          shift
          local scenario=""
          if [ "${1:-}" = "--scenario" ]; then scenario="${2:-}"; fi
          cmd_up_world_model "$scenario"
          ;;
        --demo)
          shift
          [ -n "${1:-}" ] || { echo "./nb up --demo <NN-name>"; exit 2; }
          cmd_up_demo "$1"
          ;;
        *) usage; exit 2 ;;
      esac
      ;;
    down) cmd_down ;;
    list-demos) cmd_list_demos ;;
    *) usage; exit 2 ;;
  esac
}

main "$@"
```

`cmd_up_world_model` and `cmd_up_demo` are referenced but not yet defined —
Tasks 5 and 6 add them above `main()`. Only `list-demos` and `down` are
exercised in this task's verification, so that's fine.

- [ ] **Step 2: Make it executable**

```bash
chmod +x nb
```

- [ ] **Step 3: Verify `list-demos`**

Run: `./nb list-demos`
Expected: 10 lines, `01-onboarding` through `10-ceo`, each with a non-empty
description, in order.

- [ ] **Step 4: Verify `down` is safe to run with nothing up**

Run: `./nb down`
Expected: prints "stopping everything ./nb can start ..." then "down", exits 0,
even with no clusters and no pidfiles present (`kind get clusters` grep and the
`/tmp/nb-*.pid` glob loop are both no-op-safe).

- [ ] **Step 5: Verify usage/error paths**

Run: `./nb` (no args) and `./nb bogus`
Expected: both print the `Usage:` block and exit 2.

- [ ] **Step 6: Commit**

```bash
git add nb
git commit -m "feat: add ./nb dispatcher skeleton (list-demos, down)"
```

---

### Task 5: `./nb up --world-model`

**Files:**
- Modify: `nb` (insert `cmd_up_world_model` above `main()`)

**Interfaces:**
- Consumes: `pf`, `wait_http` from Task 4.
- Produces: `cmd_up_world_model(scenario)` — `scenario` is `""` (run every
  `.yaml` in `nano-bank-world-model/scenarios/`) or a filename like
  `hero.yaml`.

- [ ] **Step 1: Insert the function**

```bash
cmd_up_world_model() {
  local scenario="${1:-}"
  echo "bringing up the full stack ..."
  SKIP_UI=1 ./scripts/deploy-all.sh

  echo "port-forward: bank-api:8081 ..."
  pf bank-api 8081
  wait_http http://localhost:8081/health "bank-api"

  [ -d "$WORLD_MODEL_DIR" ] || {
    echo "$WORLD_MODEL_DIR not found -- checkout nano-bank-world-model beside nano-bank"
    exit 1
  }
  ( cd "$WORLD_MODEL_DIR" && uv sync >/dev/null )

  local -a scenarios
  if [ -n "$scenario" ]; then
    local path="$WORLD_MODEL_DIR/scenarios/$scenario"
    [ -f "$path" ] || { echo "scenario not found: $path"; exit 1; }
    scenarios=("$path")
  else
    scenarios=("$WORLD_MODEL_DIR"/scenarios/*.yaml)
  fi

  local failures=0 s base tag
  for s in "${scenarios[@]}"; do
    base="$(basename "$s" .yaml)"
    tag="nb-$(date +%s)-$base"
    echo "realizing $base (run-tag $tag) ..."
    if ( cd "$WORLD_MODEL_DIR" && uv run world-model realize \
           --scenario "scenarios/$(basename "$s")" \
           --bank-url http://localhost:8081 --run-tag "$tag" --out ./realized ); then
      echo "  OK: $base reconciled"
    else
      echo "  FAILED: $base did not reconcile"
      failures=$((failures + 1))
    fi
  done

  echo
  echo "== world-model summary: $((${#scenarios[@]} - failures))/${#scenarios[@]} reconciled =="
  [ "$failures" -eq 0 ] || exit 1
}
```

Insert this immediately before the `main() {` line.

- [ ] **Step 2: Verify against a torn-down state**

```bash
./nb down     # make sure nothing is up
./nb up --world-model
```

Expected: `scripts/deploy-all.sh` brings up both clusters, the bank-api
port-forward comes up, and both `hero.yaml` and `corpus-measure.yaml` print
`OK: <name> reconciled`; final summary line reads `2/2 reconciled`; exit code
`0`.

- [ ] **Step 3: Verify the `--scenario` filter**

Run: `./nb up --world-model --scenario hero.yaml`
Expected: only `hero` runs, summary reads `1/1 reconciled`.

- [ ] **Step 4: Verify a bad scenario name fails fast**

Run: `./nb up --world-model --scenario does-not-exist.yaml`
Expected: prints `scenario not found: ...` and exits non-zero, without
attempting `deploy-all.sh` first... actually it *does* run `deploy-all.sh`
first (the check happens after bring-up in the current ordering) — confirm
this is acceptable (the stack coming up is harmless/idempotent even when the
scenario name is wrong) or, if the implementer prefers failing before paying
the bring-up cost, move the scenario-file existence check above the
`deploy-all.sh` call. Either behavior is correct; pick one and note it in the
commit message.

- [ ] **Step 5: Leave the stack up, then tear down**

Run: `./nb down`
Expected: kills the tracked bank-api port-forward, deletes both kind clusters,
prints `down`.

- [ ] **Step 6: Commit**

```bash
git add nb
git commit -m "feat: ./nb up --world-model (deploy stack, realize scenarios, summarize)"
```

---

### Task 6: `./nb up --demo` — narrated-agent demos (05, 06, 07, 08, 09, 10)

**Files:**
- Modify: `nb` (insert `cmd_up_demo` above `main()`)

**Interfaces:**
- Consumes: `is_valid_demo`, `cmd_list_demos` from Task 4.
- Produces: `cmd_up_demo(name)` — dispatches by demo shape. This task wires
  the narrated group; Task 7 extends the same function's `case` with the
  plain-Streamlit group and adds `up_plain_streamlit_demo`.

Investigation behind this task's dispatch (confirmed by reading the actual
scripts, not assumed from the design doc):

- `demos/05-coo/run-demo.sh`, `06-cfo/run-demo.sh`, `08-cto/run-demo.sh`,
  `09-cxo/run-demo.sh`, `10-ceo/run-demo.sh` all accept `--no-up`, and when
  called **without** it, each one runs `scripts/deploy-all.sh` itself *and*
  its own agent's `k8s/deploy.sh` (e.g. `08-cto/run-demo.sh` runs `SKIP_UI=1
  ./scripts/deploy-all.sh` then `./cto/k8s/deploy.sh`, gated by the **same**
  `DO_UP` flag). This means `./nb` must **not** call `deploy-all.sh` itself
  and then pass `--no-up` — `--no-up` would also skip that demo's own agent
  overlay deploy (`coo/k8s/deploy.sh`, `cfo/k8s/deploy.sh`, etc.), leaving the
  agent itself never deployed. (This is a correction versus the original
  design doc, which assumed `--no-up` only skips the base stack.) The correct
  call is simply the script with no flags — it is self-sufficient and
  `deploy-all.sh` is already idempotent, so there's no real cost to letting it
  run its own bring-up.
- `demos/07-suite-console/run.sh` is a different shape: it takes `--no-seed`
  (not `--no-up`) and its own comment says "It does NOT bring the cluster up"
  — its README says the stack must already be deployed via
  `scripts/deploy-all.sh` + `coo/k8s/deploy.sh` + `cfo/k8s/deploy.sh`. So for
  `07-suite-console`, `./nb` must do that bring-up itself before calling
  `run.sh`.

- [ ] **Step 1: Insert the function with the narrated-group cases**

```bash
cmd_up_demo() {
  local name="$1"
  is_valid_demo "$name" || {
    echo "unknown demo: $name"
    echo
    cmd_list_demos
    exit 1
  }

  case "$name" in
    05-coo|06-cfo|08-cto|09-cxo|10-ceo)
      # These runners bring up the base stack + their own agent overlay
      # themselves (gated by the same flag as --no-up) -- call with no flags.
      echo "launching demos/$name/run-demo.sh (it brings the stack up itself) ..."
      "demos/$name/run-demo.sh"
      ;;
    07-suite-console)
      echo "bringing up the stack + coo/cfo overlays for 07-suite-console ..."
      SKIP_UI=1 ./scripts/deploy-all.sh
      ./coo/k8s/deploy.sh
      ./cfo/k8s/deploy.sh
      "demos/07-suite-console/run.sh"
      ;;
    01-onboarding|02-simulator|03-manager-chat|04-external-agent)
      up_plain_streamlit_demo "$name"
      ;;
  esac
}
```

Insert this immediately before `main() {`. `up_plain_streamlit_demo` is added
in Task 7; the `01-onboarding|02-simulator|...)` branch is unreachable until
then, which is fine — this task's own verification only exercises the
narrated group.

- [ ] **Step 2: Verify demo-name validation**

Run: `./nb up --demo 99-nonexistent`
Expected: prints `unknown demo: 99-nonexistent` followed by the `list-demos`
table, exits 1, without touching the cluster.

- [ ] **Step 3: Verify the mechanical bring-up for one narrated demo (08-cto)**

This is the one narrated demo whose "incident" beats don't require a
successful LLM call to observe *cluster-level* effects (the bad-rollout
staging and health polling are plain kubectl), so it's the best mechanical
check available today. Full narrated-arc verification (the actual `/ask`
responses) is deferred per the live-verification caveat.

```bash
./nb down
timeout 300 ./nb up --demo 08-cto || true
kubectl --context kind-nano-bank -n nano-bank get pods
```

Expected: `scripts/deploy-all.sh` and `./cto/k8s/deploy.sh` run, `bank-api`
and `cto` pods become ready, the port-forwards come up
(`/tmp/cto-demo-pf-*.log`), and the script reaches the narrated-arc driver
step (`drive.py`) — it may then hang or error on the actual model call given
today's exhausted credits, which is expected and fine; `timeout 300` bounds
the wait so this check doesn't hang the session. Confirm via
`kubectl get pods` that `bank-api`, `cto`, and `postgres` are all `Running`.

- [ ] **Step 4: Tear down**

```bash
./nb down
demos/08-cto/run-demo.sh --down   # restores the staged 'cfo' victim + its own port-forwards
```

- [ ] **Step 5: Commit**

```bash
git add nb
git commit -m "feat: ./nb up --demo for the narrated-agent group (05,06,07,08,09,10)"
```

---

### Task 7: `./nb up --demo` — plain-Streamlit demos (01, 02, 03, 04)

**Files:**
- Modify: `nb` (add `up_plain_streamlit_demo` above `cmd_up_demo`, which
  already dispatches to it from Task 6)

**Interfaces:**
- Consumes: `pf`, `wait_http` from Task 4.
- Produces: `up_plain_streamlit_demo(name)`.

Investigation behind each demo's requirements (read directly from
`app.py`/`requirements.txt`, since `demos/04-external-agent`'s README section
is empty and needed confirming, per the design doc's flagged risk):

| demo | port-forward | env vars | deps |
|---|---|---|---|
| `01-onboarding` | `bank-api:8081` | `DEMO_API_BASE=http://localhost:8081` | `demos/01-onboarding/requirements.txt` |
| `02-simulator` | `bank-api:8081` | `DEMO_API_BASE=http://localhost:8081` (its `SERVICE_CLIENT_SECRET` already defaults to the right dev value, `api/config/default.toml`'s `security.service_client_secret`) | `demos/02-simulator/requirements.txt` |
| `03-manager-chat` | `agent-api:8086` | `DEMO_BRANCH_BASE=http://localhost:8086`, `DEMO_BRANCH_TOKEN=<BRANCH_SERVICE_TOKEN from agent/.env>` | `demos/03-manager-chat/requirements.txt` |
| `04-external-agent` | `agent-api:8086` | `DEMO_BRANCH_BASE=http://localhost:8086`, `AGENT_GATEWAY_TOKEN=<BRANCH_SERVICE_TOKEN from agent/.env>`, `OLLAMA_API_KEY=<from agent/.env>`, `PYTHONPATH=<repo root>` | **`agent/requirements.txt`**, not `demos/04-external-agent/requirements.txt` (`app.py` does `from agent.external_agent.agent import ExternalAgent, GatewayHTTP` and `from agent import model_factory as mf`, i.e. it imports the `agent` package directly and needs its full dependency set, not the demo's own thin `streamlit`+`requests` file) |

Note on `04-external-agent`'s token: its `app.py` reads `AGENT_GATEWAY_TOKEN`,
but `agent/.env` only defines `BRANCH_SERVICE_TOKEN` — there is no separate
`AGENT_GATEWAY_TOKEN` secret anywhere in the repo (`agent/config.py`'s
`Settings.agent_gateway_token` defaults to `""` since nothing sets it in
`agent/k8s/deploy.sh`'s secret). This plan reuses `BRANCH_SERVICE_TOKEN` for
`AGENT_GATEWAY_TOKEN` (mirroring how `03-manager-chat` uses the same token as
`DEMO_BRANCH_TOKEN`), since that is the only credential the repo actually
provisions. If the live gateway rejects it, that is a pre-existing gap in the
demo's own auth wiring — out of scope for this bring-up-tooling plan to fix.

- [ ] **Step 1: Insert the function**

```bash
up_plain_streamlit_demo() {
  local name="$1"
  echo "bringing up the full stack ..."
  SKIP_UI=1 ./scripts/deploy-all.sh

  local venv="demos/$name/.venv"
  local reqs="demos/$name/requirements.txt"
  local port env_str=""

  case "$name" in
    01-onboarding)
      pf bank-api 8081
      wait_http http://localhost:8081/health "bank-api"
      port=8510
      env_str="DEMO_API_BASE=http://localhost:8081"
      ;;
    02-simulator)
      pf bank-api 8081
      wait_http http://localhost:8081/health "bank-api"
      port=8511
      env_str="DEMO_API_BASE=http://localhost:8081"
      ;;
    03-manager-chat)
      pf agent-api 8086
      wait_http http://localhost:8086/health "agent-api"
      local token
      token=$(grep -E '^BRANCH_SERVICE_TOKEN=' agent/.env | cut -d= -f2-)
      [ -n "$token" ] || { echo "BRANCH_SERVICE_TOKEN empty in agent/.env"; exit 1; }
      port=8512
      env_str="DEMO_BRANCH_BASE=http://localhost:8086 DEMO_BRANCH_TOKEN=$token"
      ;;
    04-external-agent)
      pf agent-api 8086
      wait_http http://localhost:8086/health "agent-api"
      local token ollama_key
      token=$(grep -E '^BRANCH_SERVICE_TOKEN=' agent/.env | cut -d= -f2-)
      ollama_key=$(grep -E '^OLLAMA_API_KEY=' agent/.env | cut -d= -f2-)
      [ -n "$token" ] || { echo "BRANCH_SERVICE_TOKEN empty in agent/.env"; exit 1; }
      reqs="agent/requirements.txt"
      port=8513
      env_str="DEMO_BRANCH_BASE=http://localhost:8086 AGENT_GATEWAY_TOKEN=$token OLLAMA_API_KEY=$ollama_key PYTHONPATH=$PWD"
      ;;
  esac

  if [ ! -x "$venv/bin/streamlit" ]; then
    echo "creating demo venv ($venv) via uv ..."
    uv venv "$venv" >/dev/null
    uv pip install --python "$venv/bin/python" -r "$reqs" >/dev/null
  fi

  echo "launching demos/$name on http://localhost:$port ..."
  env $env_str "$venv/bin/streamlit" run "demos/$name/app.py" \
    --server.port "$port" --server.headless true --browser.gatherUsageStats false \
    >"/tmp/nb-streamlit-$name.log" 2>&1 &
  echo $! > "/tmp/nb-streamlit-$name.pid"
  wait_http "http://localhost:$port" "$name"
  echo "open http://localhost:$port"
}
```

Insert this immediately above `cmd_up_demo` (which Task 6 already wired to
call it for `01-onboarding|02-simulator|03-manager-chat|04-external-agent`).

- [ ] **Step 2: Verify `01-onboarding` end to end (no LLM dependency)**

```bash
./nb down
./nb up --demo 01-onboarding
curl -fsS http://localhost:8510 >/dev/null && echo "reachable"
```

Expected: stack comes up, `bank-api` port-forward succeeds, the venv is
created once (`demos/01-onboarding/.venv`), Streamlit starts headless on
`:8510`, `curl` succeeds. Manually confirm in a browser (or via
`curl -fsS http://localhost:8510 | head -20`, checking for the Streamlit HTML
shell) that the page is nano-bank's onboarding demo, then exercise the golden
path (create a customer, open an account, post a transaction) to confirm it
actually talks to the live bank-api — this demo has no LLM dependency so this
is fully verifiable today.

- [ ] **Step 3: Verify `02-simulator` (no LLM dependency)**

```bash
./nb down
./nb up --demo 02-simulator
curl -fsS http://localhost:8511 >/dev/null && echo "reachable"
```

Expected: same shape as Step 2; spot-check the simulator's "generate activity"
button actually posts real transactions against the live bank-api and the
event-log tab shows green entries.

- [ ] **Step 4: Verify `03-manager-chat` and `04-external-agent` mechanically only**

```bash
./nb down
./nb up --demo 03-manager-chat
curl -fsS http://localhost:8512 >/dev/null && echo "reachable"
./nb down
./nb up --demo 04-external-agent
curl -fsS http://localhost:8513 >/dev/null && echo "reachable"
```

Expected: both reach a running Streamlit page (confirming the port-forward,
token sourcing, and — for 04 — the heavier `agent/requirements.txt` venv
install all work). Do **not** expect the actual chat/agent turns to succeed
today (Ollama-cloud credits exhausted); if the page loads but a chat message
errors out with a model/auth failure, that confirms the bring-up tooling
worked and the failure is the known, expected credits gap — not a bug in
`./nb`. Re-verify the live conversation once credits are back.

- [ ] **Step 5: Tear down**

```bash
./nb down
```

- [ ] **Step 6: Commit**

```bash
git add nb
git commit -m "feat: ./nb up --demo for the plain-Streamlit group (01,02,03,04)"
```

---

### Task 8: Final pass — full script review + `docs/specs` cross-check

**Files:**
- Modify: `nb` (only if Task 8 review finds an issue; otherwise no changes)

- [ ] **Step 1: Re-read the whole `nb` file top to bottom**

Run: `cat nb` and check: every function referenced in `main()`/`cmd_up_demo()`
is defined exactly once, no leftover `TODO`/placeholder text, `chmod +x nb`
still set (`ls -l nb`).

- [ ] **Step 2: Cross-check against the spec's four subcommands**

Confirm each of the four bullets in
`docs/specs/2026-08-27-ci-and-single-command-bringup-design.md`'s Goals
section has a working command: `./nb up --world-model [--scenario <name>]`
(Task 5), `./nb up --demo <NN-name>` (Tasks 6–7), `./nb down` (Task 4),
`./nb list-demos` (Task 4).

- [ ] **Step 3: Run `./nb down` one last time to leave a clean state**

```bash
./nb down
```

- [ ] **Step 4: Update the PR / branch summary (no file changes)**

Note in the branch description (not a new doc file) which parts were
live-verified today versus deferred per the credits caveat, so the next
session picks up exactly at "re-verify demos 03/04/05/06/07/08/09/10 live."

No commit needed for this task unless Step 1 surfaces a real fix.
