# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this project is

An experimental Canadian challenger-bank core API — Rust (`axum 0.7`) over a PostgreSQL 16 double-entry ledger running in a local Kind (Kubernetes-in-Docker) cluster. The schema and infrastructure are fully built; most handler business logic is still TODO stubs. The card payment rails (`/api/v1/cards/*`) are the most complete part.

## Commands

All Rust commands run from `api/`:

```bash
cargo check          # fast type-check without producing a binary
cargo build          # compile
cargo run            # build + start API on 0.0.0.0:8081
cargo fmt            # format
cargo clippy         # lint
cargo test           # run tests (dev-dependencies are wired but no tests exist yet)
```

Infrastructure (run from repo root):

```bash
kind create cluster --config k8s/kind-cluster-config.yaml   # one-time cluster setup
./k8s/deploy.sh                                              # deploy Postgres + run DDL init Job
kubectl port-forward -n nano-bank svc/postgres-service 5432:5432  # required before cargo run
kubectl exec -it -n nano-bank deployment/postgres -- psql -U nanobank_user -d nano_bank_db
testing/cleanup.sh --dry-run   # preview row counts
testing/cleanup.sh             # TRUNCATE customers CASCADE (wipes all data, GL accounts self-heal)
```

Config override via env vars uses prefix `NANO_BANK__` with `__` as separator (e.g. `NANO_BANK__DATABASE__HOST`). Layer order: `config/default.toml` → `config/{RUN_MODE}.toml` → `config/local.toml` → env vars.

## Architecture

### Request flow

```
HTTP request
  → axum middleware stack (CORS, gzip, 30s timeout, 10MB body limit, tracing)
  → handler fn (State<AppState>, Json<RequestType>) -> Result<(StatusCode, Json<ResponseType>), AppError>
  → sqlx raw query against PgPool
  → PostgreSQL triggers mutate balances / enforce invariants
  → AppError::into_response() serialises errors as { "error": { "code", "message", "details" } }
```

`AppState` (in `handlers/mod.rs`) holds a cloned `PgPool` and `Settings`. It is injected into every handler via axum's `State` extractor.

### Database interaction pattern

There is no ORM, no repository layer, and no service layer — those modules exist as empty placeholders. All SQL is inline in the handler using `sqlx::query_as::<_, ModelType>(raw_sql).bind(...).fetch_one(&pool)`.

Postgres constraint codes are matched directly in handlers to return correct HTTP statuses:
- `23505` → `AppError::Conflict` (unique violation)
- `23503` → `AppError::BadRequest` (FK violation)
- `23514` → `AppError::BadRequest` (CHECK violation)

### Double-entry ledger (critical invariant)

Both legs of every transaction **must be inserted in a single multi-row `INSERT` statement** — see `post_two_legged()` in `handlers/cards.rs`. The reason: `trigger_validate_transaction_balance` (AFTER INSERT on `transaction_entries`) checks that `SUM(debits) = SUM(credits)` for the transaction and raises an exception if they don't balance. Inserting one leg at a time trips this trigger between inserts.

Other key triggers (all in `src/core/tables/06_triggers.sql`):
- `trigger_update_account_balance` — BEFORE INSERT on `transaction_entries`: updates `accounts.balance` and fills `balance_before`/`balance_after`. Never update balances directly.
- `trigger_generate_account_number` — BEFORE INSERT on `accounts`: overwrites `account_number` with a random 12-digit value. GL accounts cannot be found by account number; they are keyed by `(customer_id, account_type)`.

### Card payment rails GL accounts

Two internal accounts exist under `system@nano.bank`:
- `VISA_CLEARING` (`account_type = chequing`) — carries a negative balance representing the issuer's obligation to the network
- `BANK_SETTLEMENT` (`account_type = savings`) — the funding account

Both carry a $1 trillion overdraft limit so their balances can go negative. They are bootstrapped idempotently at startup by `handlers::cards::ensure_system_accounts()` and their UUIDs are re-resolved per request (not cached in AppState) because a `testing/cleanup.sh` run wipes and rebuilds them.

### Enum serialisation quirk

`KycStatus` and similar enums use `#[sqlx(rename_all = "snake_case")]` for DB mapping but have no serde rename attribute, so JSON output is PascalCase (e.g. `"Pending"`, not `"pending"`).

### Configuration

`api/config/default.toml` is the source of truth for local dev credentials. The `database.host` is `::1` (IPv6 loopback) because `kubectl port-forward` creates a dead `0.0.0.0:5432` mapping — IPv4 connections are reset by the Kind/Docker proxy.

### What is and isn't implemented

Implemented handlers (real SQL + logic):
- `POST /api/v1/customers` — `handlers/customers.rs`
- `POST /api/v1/accounts` — `handlers/accounts.rs`
- `POST /api/v1/cards/authorize|capture|settle` — `handlers/cards.rs`
- `GET /health`, `GET /docs`

Everything else (`auth`, `transactions`, `security`, GET endpoints for customers/accounts) returns a static `"... endpoint - TODO: implement"` string.

### Bruno collection

Bruno `.bru` files require `body: json` and `auth: inherit` declared **inside the `post {}` block** alongside the URL — without those, Bruno ignores the `body:json {}` content block entirely. See the working `_2` files in `bruno/` for the correct format Bruno generates from its own UI.

### Testing

No Rust tests exist. The test harness is three Python containers in `testing/`:
- `generator/generate_customers.py` — seeds fake Canadian customers and accounts via the API
- `visa/visa_simulator.py` — drives the full authorize → capture → settle loop
- `viewer/app.py` — Streamlit live dashboard on port 8504
