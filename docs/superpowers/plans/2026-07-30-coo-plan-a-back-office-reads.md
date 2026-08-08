# COO Plan A — Bank Back-Office Operational Reads (Rust) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a service-plane, read-only `/api/v1/back-office/ops/*` surface to the bank so the COO's operations MCP can read bank-wide operational data that today's customer-scoped endpoints cannot expose.

**Architecture:** A new `back_office` handler module in the axum API, mounted under `/api/v1/back-office`, every route gated by the existing `AuthenticatedService` extractor. Handlers run raw `sqlx` aggregate queries against the shared `PgPool` and return JSON. This plan delivers the two cheapest, highest-signal endpoints — `float` and `transactions` — and establishes the module + auth + test harness the follow-on plan (rails/cards/exceptions) extends.

**Tech Stack:** Rust, axum 0.7, sqlx 0.7 (raw queries, no ORM), chrono 0.4, rust_decimal, serde. HTTP integration tests with reqwest.

## Global Constraints

- **Service plane only.** Every new route takes `_: AuthenticatedService` as its first extractor. Customer-plane handlers (`accounts.rs`, `transactions.rs`, etc.) are **not** modified.
- **Endpoint namespace** is `/api/v1/back-office/` — a neutral placeholder; do not rename in this plan.
- **Fraud tables are unreadable.** No query may touch `suspicious_activities`, `monitoring_rules`, or `rule_violations`.
- **SQL style:** raw `sqlx::query_as::<_, Row>(sql).bind(..).fetch_all(&state.pool).await.map_err(AppError::Database)?` — matches `api/src/handlers/accounts.rs`. No ORM, no repository layer.
- **DB host is `::1`** locally (via `kubectl port-forward`); `127.0.0.1` does not work. No GL core is needed — these are reads, they do not dual-post.
- **Test harness:** integration tests in `api/tests/`, graceful-skip pattern from `api/tests/finance.rs` — probe `GET /health`, return early (still passing) when the API is unreachable. Service token minted via `POST /api/v1/auth/service-token` with the dev secret `nano-bank-visa-network-secret-change-me`.
- All Rust commands run from `api/`.

---

## File Structure

- **Create** `api/src/handlers/back_office.rs` — the module: `back_office_routes()`, handlers, and their request/response/row structs. One responsibility: service-plane operational reads.
- **Modify** `api/src/handlers/mod.rs` — add `pub mod back_office;`.
- **Modify** `api/src/main.rs` — `.nest("/api/v1/back-office", handlers::back_office::back_office_routes())`.
- **Create** `api/tests/back_office.rs` — integration tests (float shape + auth plane; transactions shape + window validation).

---

### Task 1: `back_office` module + `GET /ops/float` + service-plane auth

**Files:**
- Create: `api/src/handlers/back_office.rs`
- Modify: `api/src/handlers/mod.rs` (add `pub mod back_office;`)
- Modify: `api/src/main.rs` (nest the router, next to the other `.nest("/api/v1/...")` lines)
- Test: `api/tests/back_office.rs`

**Interfaces:**
- Consumes: `crate::handlers::AppState` (`{ pool: DatabasePool, settings, ledger }`); `crate::middleware::auth::AuthenticatedService` (unit-struct extractor that 403s a non-service token, 401s a missing one); `crate::errors::AppError` (`Database(sqlx::Error)`, `BadRequest(String)`).
- Produces: `pub fn back_office_routes() -> axum::Router<AppState>`; `GET /api/v1/back-office/ops/float` → `{ "accounts": [{system, role, account_type, balance}], "total_float": "<decimal>" }`.

- [ ] **Step 1: Write the failing test**

Create `api/tests/back_office.rs`:

```rust
//! Integration tests for the service-plane back-office operational reads.
//!
//! Graceful-skip harness (mirrors tests/finance.rs): every test probes
//! `GET /health` and returns early (still passing) when the API is unreachable,
//! so `cargo test` passes with nothing running. No GL core needed — these are
//! reads. Run against a live stack:
//!   cd api && cargo test --test back_office -- --nocapture
//! Override the base URL with NANO_BANK_TEST_URL.

use serde_json::{json, Value};

const TEST_PASSWORD: &str = "securepass123";
// Dev service-plane secret (api/config/default.toml). Overridable in CI.
const SERVICE_SECRET: &str = "nano-bank-visa-network-secret-change-me";

fn base_url() -> String {
    std::env::var("NANO_BANK_TEST_URL").unwrap_or_else(|_| "http://localhost:8081".to_string())
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

async fn stack_up(c: &reqwest::Client) -> bool {
    c.get(format!("{}/health", base_url())).send().await.is_ok()
}

async fn service_token(c: &reqwest::Client) -> String {
    let resp = c
        .post(format!("{}/api/v1/auth/service-token", base_url()))
        .json(&json!({ "client_secret": SERVICE_SECRET }))
        .send()
        .await
        .expect("service-token request");
    assert!(resp.status().is_success(), "service-token: {}", resp.status());
    resp.json::<Value>().await.unwrap()["access_token"]
        .as_str()
        .unwrap()
        .to_string()
}

// A logged-in customer token, to prove the service plane rejects it.
async fn customer_token(c: &reqwest::Client) -> String {
    let email = format!("bo-{}@example.com", uuid::Uuid::new_v4());
    let reg = c
        .post(format!("{}/api/v1/customers", base_url()))
        .json(&json!({
            "email": email, "password": TEST_PASSWORD,
            "first_name": "Bo", "last_name": "Tester",
            "date_of_birth": "1990-01-01", "phone": "+15145550123"
        }))
        .send().await.expect("register");
    assert!(reg.status().is_success(), "register: {}", reg.status());
    let resp = c
        .post(format!("{}/api/v1/auth/login", base_url()))
        .json(&json!({ "email": email, "password": TEST_PASSWORD }))
        .send().await.expect("login");
    assert!(resp.status().is_success(), "login: {}", resp.status());
    resp.json::<Value>().await.unwrap()["access_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn float_returns_system_accounts_for_a_service_token() {
    let c = client();
    if !stack_up(&c).await { eprintln!("stack down; skipping"); return; }
    let svc = service_token(&c).await;

    let resp = c
        .get(format!("{}/api/v1/back-office/ops/float", base_url()))
        .bearer_auth(&svc)
        .send().await.expect("float request");
    assert!(resp.status().is_success(), "float: {}", resp.status());

    let body = resp.json::<Value>().await.unwrap();
    let accounts = body["accounts"].as_array().expect("accounts array");
    assert!(!accounts.is_empty(), "expected the bootstrapped system accounts");
    assert!(
        accounts.iter().any(|a| a["system"] == "system"),
        "expected a system@ (VISA_CLEARING/BANK_SETTLEMENT) entry, got {accounts:?}"
    );
    assert!(body["total_float"].is_string(), "total_float should be a decimal string");
}

#[tokio::test]
async fn float_rejects_a_customer_token() {
    let c = client();
    if !stack_up(&c).await { eprintln!("stack down; skipping"); return; }
    let cust = customer_token(&c).await;

    let resp = c
        .get(format!("{}/api/v1/back-office/ops/float", base_url()))
        .bearer_auth(&cust)
        .send().await.expect("float request");
    assert_eq!(resp.status().as_u16(), 403, "customer token must be refused on the service plane");
}
```

- [ ] **Step 2: Run test to verify it fails**

Bring up a stack (`./k8s/deploy.sh`; port-forward Postgres; `cd api && cargo run`), then:

Run: `cd api && cargo test --test back_office float_returns_system_accounts_for_a_service_token -- --nocapture`
Expected: FAIL — the route does not exist yet, so the request returns 404 and the `is_success()` assert fails. (If the stack is down the test prints "stack down; skipping" and passes — bring the stack up to see the real red.)

- [ ] **Step 3: Write minimal implementation**

Create `api/src/handlers/back_office.rs`:

```rust
//! Service-plane, read-only operational reads (the "back office" — the COO's
//! perception surface). Bank-wide aggregates with no customer identity; every
//! route requires a service token. The customer-plane handlers are untouched,
//! and no fraud table is ever read here.
use axum::{extract::State, routing::get, Json, Router};
use rust_decimal::Decimal;
use serde::Serialize;

use crate::errors::AppError;
use crate::handlers::AppState;
use crate::middleware::auth::AuthenticatedService;

pub fn back_office_routes() -> Router<AppState> {
    Router::new().route("/ops/float", get(ops_float))
}

#[derive(Serialize)]
struct FloatAccount {
    system: String,       // interac | aft | lynx | system | cash
    role: String,         // clearing | settlement | external_cash | other
    account_type: String, // chequing | savings | ...
    balance: Decimal,
}

#[derive(Serialize)]
struct FloatResponse {
    accounts: Vec<FloatAccount>,
    total_float: Decimal,
}

#[derive(sqlx::FromRow)]
struct FloatRow {
    email: String,
    account_type: String,
    balance: Decimal,
}

/// The clearing/settlement float: balances of the synthetic system customers'
/// accounts (`*@nano.bank`). `chequing`→clearing, `savings`→settlement, except
/// `cash@nano.bank`'s chequing which is EXTERNAL_CASH.
async fn ops_float(
    _: AuthenticatedService,
    State(state): State<AppState>,
) -> Result<Json<FloatResponse>, AppError> {
    let rows = sqlx::query_as::<_, FloatRow>(
        "SELECT c.email AS email, a.account_type::text AS account_type, a.balance AS balance
         FROM accounts a
         JOIN customers c ON c.customer_id = a.customer_id
         WHERE c.email LIKE '%@nano.bank'
         ORDER BY c.email, a.account_type",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(AppError::Database)?;

    let mut accounts = Vec::with_capacity(rows.len());
    let mut total = Decimal::ZERO;
    for r in rows {
        let system = r.email.split('@').next().unwrap_or("").to_string();
        let role = match (system.as_str(), r.account_type.as_str()) {
            ("cash", _) => "external_cash",
            (_, "chequing") => "clearing",
            (_, "savings") => "settlement",
            _ => "other",
        }
        .to_string();
        total += r.balance;
        accounts.push(FloatAccount {
            system,
            role,
            account_type: r.account_type,
            balance: r.balance,
        });
    }
    Ok(Json(FloatResponse { accounts, total_float: total }))
}
```

Add to `api/src/handlers/mod.rs` (with the other `pub mod` lines):

```rust
pub mod back_office;
```

Add to `api/src/main.rs`, alongside the existing `.nest("/api/v1/...")` calls (e.g. right after the `accounts` nest):

```rust
        .nest("/api/v1/back-office", handlers::back_office::back_office_routes())
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd api && cargo test --test back_office -- --nocapture`
Expected: PASS — `float_returns_system_accounts_for_a_service_token` returns the bootstrapped `system@` accounts; `float_rejects_a_customer_token` gets a 403. (`cargo fmt && cargo clippy` clean.)

- [ ] **Step 5: Commit**

```bash
git add api/src/handlers/back_office.rs api/src/handlers/mod.rs api/src/main.rs api/tests/back_office.rs
git commit -m "feat(back-office): service-plane GET /ops/float (COO perception surface)"
```

---

### Task 2: `GET /ops/transactions?window=`

**Files:**
- Modify: `api/src/handlers/back_office.rs` (add the route + handler + structs)
- Test: `api/tests/back_office.rs` (add two tests)

**Interfaces:**
- Consumes: everything from Task 1.
- Produces: `GET /api/v1/back-office/ops/transactions?window=24h|7d|30d` (default `24h`) → `{ "window": "24h", "since": "<rfc3339>", "groups": [{transaction_type, status, count, total}] }`; an unsupported `window` → 400.

- [ ] **Step 1: Write the failing test**

Append to `api/tests/back_office.rs`:

```rust
#[tokio::test]
async fn transactions_summary_returns_grouped_shape() {
    let c = client();
    if !stack_up(&c).await { eprintln!("stack down; skipping"); return; }
    let svc = service_token(&c).await;

    let resp = c
        .get(format!("{}/api/v1/back-office/ops/transactions?window=7d", base_url()))
        .bearer_auth(&svc)
        .send().await.expect("transactions request");
    assert!(resp.status().is_success(), "transactions: {}", resp.status());

    let body = resp.json::<Value>().await.unwrap();
    assert_eq!(body["window"], "7d");
    assert!(body["since"].is_string(), "since should be an rfc3339 string");
    assert!(body["groups"].is_array(), "groups should be an array");
}

#[tokio::test]
async fn transactions_summary_rejects_bad_window() {
    let c = client();
    if !stack_up(&c).await { eprintln!("stack down; skipping"); return; }
    let svc = service_token(&c).await;

    let resp = c
        .get(format!("{}/api/v1/back-office/ops/transactions?window=1y", base_url()))
        .bearer_auth(&svc)
        .send().await.expect("transactions request");
    assert_eq!(resp.status().as_u16(), 400, "unsupported window must be a 400");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd api && cargo test --test back_office transactions_summary_returns_grouped_shape -- --nocapture`
Expected: FAIL — route missing → 404, the `is_success()` assert fails (stack up).

- [ ] **Step 3: Write minimal implementation**

In `api/src/handlers/back_office.rs`, extend the imports and router and add the handler:

```rust
// widen the existing imports:
use axum::{extract::{Query, State}, routing::get, Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
```

```rust
// in back_office_routes(), chain the new route:
        .route("/ops/transactions", get(ops_transactions))
```

```rust
#[derive(Deserialize)]
struct WindowQuery {
    window: Option<String>,
}

/// Map a window shorthand to a cutoff instant. Unknown windows are a 400 so the
/// caller learns the vocabulary rather than getting silent 24h data.
fn window_cutoff(window: &str) -> Result<DateTime<Utc>, AppError> {
    let dur = match window {
        "24h" => Duration::hours(24),
        "7d" => Duration::days(7),
        "30d" => Duration::days(30),
        other => {
            return Err(AppError::BadRequest(format!(
                "unsupported window '{other}' (use 24h|7d|30d)"
            )))
        }
    };
    Ok(Utc::now() - dur)
}

#[derive(Serialize, sqlx::FromRow)]
struct TxnGroup {
    transaction_type: String,
    status: String,
    count: i64,
    total: Decimal,
}

#[derive(Serialize)]
struct TransactionsResponse {
    window: String,
    since: DateTime<Utc>,
    groups: Vec<TxnGroup>,
}

/// Bank-wide transaction counts + amounts grouped by type and status over a
/// window. Read-only aggregate; no customer scoping.
async fn ops_transactions(
    _: AuthenticatedService,
    State(state): State<AppState>,
    Query(q): Query<WindowQuery>,
) -> Result<Json<TransactionsResponse>, AppError> {
    let window = q.window.unwrap_or_else(|| "24h".to_string());
    let since = window_cutoff(&window)?;
    let groups = sqlx::query_as::<_, TxnGroup>(
        "SELECT transaction_type,
                status::text AS status,
                COUNT(*) AS count,
                COALESCE(SUM(amount), 0) AS total
         FROM transactions
         WHERE created_at >= $1
         GROUP BY transaction_type, status
         ORDER BY transaction_type, status",
    )
    .bind(since)
    .fetch_all(&state.pool)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(TransactionsResponse { window, since, groups }))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd api && cargo test --test back_office -- --nocapture`
Expected: PASS — all four tests green (`cargo fmt && cargo clippy` clean).

- [ ] **Step 5: Commit**

```bash
git add api/src/handlers/back_office.rs api/tests/back_office.rs
git commit -m "feat(back-office): GET /ops/transactions?window= summary"
```

---

## Self-Review

**1. Spec coverage (Component 1a).** This plan delivers `float` and `transactions` — the spec's stated "cheapest, highest-signal first" pair. The remaining three (`rails`, `cards`, `exceptions`) are explicitly out of this plan and go in the follow-on plan (Plan A2); the spec sanctions this incremental order. The service-plane gating, customer-plane-untouched rule, and fraud-table exclusion are all honoured. ✓

**2. Placeholder scan.** No TBD/TODO; every step has runnable code or an exact command. ✓

**3. Type consistency.** `AppError::Database`/`BadRequest`, `AppState.pool`, `AuthenticatedService`, `sqlx::query_as::<_, Row>`, `DateTime<Utc>` all match the verified codebase. `total_float` is serialized by rust_decimal as a JSON string, which the test asserts with `is_string()`. `count` is `i64` (Postgres `COUNT(*)` → `bigint`). `status::text` / `account_type::text` cast the PG enums to text so no enum decode is needed. ✓

**Note for the executor:** the red/green loop needs a live stack (bank API + Postgres on `::1`); with nothing running the tests skip-and-pass by design. Bring the stack up before Step 2 of each task.
