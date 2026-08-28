//! Integration tests for the interest / NIM engine (spec #2).
//!
//! Same graceful-skip harness as `tests/transactions.rs`: probes `GET /health`
//! and returns (still passing) when the API isn't running. The accrue/capitalise
//! flows post aggregate GL, so they additionally skip when a deposit or the batch
//! returns `503` (GL core down).
//!
//! Run against a live stack (a core must be up):
//! ```bash
//! cd api && CORE_BACKEND=modern cargo test --test finance -- --nocapture
//! ```
//! Override the base URL with `NANO_BANK_TEST_URL`.

use serde_json::{json, Value};
use uuid::Uuid;

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
    matches!(
        c.get(format!("{}/health", base_url())).send().await,
        Ok(r) if r.status().is_success()
    )
}

macro_rules! require_stack {
    ($c:expr) => {
        if !stack_up($c).await {
            eprintln!("SKIP: nano-bank not reachable at {}", base_url());
            return;
        }
    };
}

fn as_num(v: &Value) -> f64 {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or_else(|| panic!("not a number: {v:?}"))
}

async fn create_customer(c: &reqwest::Client) -> String {
    let n = Uuid::new_v4().as_u128();
    let email = format!("fintest_{}@example.com", n % 1_000_000_000);
    let body = json!({
        "email": email,
        "phone_number": format!("{:010}", (n % 10_000_000_000u128)),
        "first_name": "Fin",
        "last_name": "Test",
        "date_of_birth": "1990-01-01",
        "sin": format!("{:09}", n % 1_000_000_000),
        "password": TEST_PASSWORD
    });
    let resp = c
        .post(format!("{}/api/v1/customers", base_url()))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "create customer: {}",
        resp.status()
    );
    email
}

async fn login(c: &reqwest::Client, email: &str) -> String {
    let resp = c
        .post(format!("{}/api/v1/auth/login", base_url()))
        .json(&json!({ "email": email, "password": TEST_PASSWORD }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "login: {}", resp.status());
    let v: Value = resp.json().await.unwrap();
    v["access_token"].as_str().unwrap().to_string()
}

async fn create_account(c: &reqwest::Client, token: &str, account_type: &str) -> Uuid {
    let resp = c
        .post(format!("{}/api/v1/accounts", base_url()))
        .bearer_auth(token)
        .json(&json!({ "account_type": account_type }))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "create account: {}",
        resp.status()
    );
    let v: Value = resp.json().await.unwrap();
    Uuid::parse_str(v["account_id"].as_str().unwrap()).unwrap()
}

/// Mint a network-plane service token (for the finance batch endpoints).
async fn service_token(c: &reqwest::Client) -> String {
    let resp = c
        .post(format!("{}/api/v1/auth/service-token", base_url()))
        .json(&json!({ "client_secret": SERVICE_SECRET }))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "service-token: {}",
        resp.status()
    );
    let v: Value = resp.json().await.unwrap();
    v["access_token"].as_str().unwrap().to_string()
}

/// Deposit into an account. Returns false if the GL core is down (503) so the
/// caller can skip.
async fn deposit(c: &reqwest::Client, token: &str, account: Uuid, amount: f64) -> bool {
    let resp = c
        .post(format!("{}/api/v1/transactions/deposit", base_url()))
        .bearer_auth(token)
        .json(&json!({ "account_id": account, "amount": amount, "description": "seed funds" }))
        .send()
        .await
        .unwrap();
    if resp.status().as_u16() == 503 {
        return false;
    }
    assert!(resp.status().is_success(), "deposit: {}", resp.status());
    true
}

async fn balances(c: &reqwest::Client) -> Value {
    c.get(format!("{}/api/v1/ledger/balances", base_url()))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

/// Sum the GL balance of the account(s) named by any of `names` (accepts the
/// modern code or the legacy saknr), as an absolute magnitude (sign-agnostic).
async fn gl_balance_abs(c: &reqwest::Client, names: &[&str]) -> f64 {
    let bal = balances(c).await;
    bal.as_array()
        .map(|arr| {
            arr.iter()
                .filter(|a| names.contains(&a["account"].as_str().unwrap_or("")))
                .map(|a| as_num(&a["balance"]).abs())
                .sum()
        })
        .unwrap_or(0.0)
}

async fn account_balance(c: &reqwest::Client, token: &str, account_id: Uuid) -> f64 {
    let v: Value = c
        .get(format!(
            "{}/api/v1/accounts/{}/balance",
            base_url(),
            account_id
        ))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    as_num(&v["balance"])
}

/// A $10,000 deposit at 3% accrues $0.82 for one day; the run is idempotent and
/// the GL shows the accrued-interest-payable side.
#[tokio::test]
async fn daily_accrual_posts_and_is_idempotent() {
    let c = client();
    require_stack!(&c);

    let email = create_customer(&c).await;
    let token = login(&c, &email).await;
    let acct = create_account(&c, &token, "chequing").await; // chequing auto-earns 3%
    if !deposit(&c, &token, acct, 10_000.0).await {
        eprintln!("SKIP: GL core unavailable (deposit 503)");
        return;
    }

    let svc = service_token(&c).await;
    // Unique far-future date per run: accrual_runs (and thus GL posting) is keyed
    // by date in nano-bank's shared DB, so a fixed date consumed by an earlier run
    // (e.g. against the other core) would return cached totals and skip the post.
    let n = Uuid::new_v4().as_u128();
    let year = 2400 + (n % 300) as i64;
    let date = format!("{year}-03-10");

    let post_accrue = || async {
        c.post(format!("{}/api/v1/finance/accrue", base_url()))
            .bearer_auth(&svc)
            .json(&json!({ "as_of": date }))
            .send()
            .await
            .unwrap()
    };

    let r1 = post_accrue().await;
    if r1.status().as_u16() == 503 {
        eprintln!("SKIP: GL core unavailable (accrue 503)");
        return;
    }
    assert!(r1.status().is_success(), "accrue: {}", r1.status());
    let v1: Value = r1.json().await.unwrap();
    let expense_1 = as_num(&v1["expense_total"]);
    // Our $10k @ 3% contributes exactly 0.82; other test accounts may add more.
    assert!(
        expense_1 >= 0.82,
        "expense_total should include our 0.82, got {expense_1}"
    );

    // Idempotent: a second run for the same date returns identical totals.
    let r2 = post_accrue().await;
    assert!(r2.status().is_success(), "re-accrue: {}", r2.status());
    let v2: Value = r2.json().await.unwrap();
    assert_eq!(
        as_num(&v2["expense_total"]),
        expense_1,
        "re-running the same date must be a no-op"
    );
    assert_eq!(
        v2["economic_event_id"], v1["economic_event_id"],
        "idempotent run keeps the same event id"
    );

    // The GL carries the accrued-interest-payable position. The balances endpoint
    // names accounts by the active backend's own code (modern `ACCR_INT_PAY`,
    // legacy saknr `0000220000`) — accept either.
    let bal = balances(&c).await;
    let arr = bal.as_array().expect("balances is an array");
    assert!(
        arr.iter().any(|a| {
            let name = a["account"].as_str().unwrap_or("");
            name == "ACCR_INT_PAY" || name == "0000220000"
        }),
        "balances should list the accrued-interest-payable account after accrual"
    );
}

/// A $1,000 deposit (below the maintenance waiver) accrues $0.08 for a day, then
/// capitalisation credits the interest and charges the $4 maintenance fee, and is
/// idempotent per period. A unique far-future period keeps the run isolated across
/// re-runs of the suite (the batch is idempotent per period).
#[tokio::test]
async fn capitalisation_credits_interest_and_charges_maintenance() {
    let c = client();
    require_stack!(&c);

    let email = create_customer(&c).await;
    let token = login(&c, &email).await;
    let acct = create_account(&c, &token, "chequing").await; // auto 3%
    if !deposit(&c, &token, acct, 1_000.0).await {
        eprintln!("SKIP: GL core unavailable (deposit 503)");
        return;
    }

    let svc = service_token(&c).await;
    // Unique period/date per run so period-level idempotency doesn't collide.
    let n = Uuid::new_v4().as_u128();
    let year = 2200 + (n % 400) as i64;
    let period = format!("{year}-06");
    let as_of = format!("{year}-06-15");

    // Accrue one day: 1000 * 0.03 / 365 = 0.08.
    let ar = c
        .post(format!("{}/api/v1/finance/accrue", base_url()))
        .bearer_auth(&svc)
        .json(&json!({ "as_of": as_of }))
        .send()
        .await
        .unwrap();
    if ar.status().as_u16() == 503 {
        eprintln!("SKIP: GL core unavailable (accrue 503)");
        return;
    }
    assert!(ar.status().is_success(), "accrue: {}", ar.status());

    let before = account_balance(&c, &token, acct).await;
    assert!(
        (before - 1000.0).abs() < 1e-6,
        "pre-capitalisation balance should be 1000, got {before}"
    );

    let post_cap = || async {
        c.post(format!("{}/api/v1/finance/capitalise", base_url()))
            .bearer_auth(&svc)
            .json(&json!({ "period": period }))
            .send()
            .await
            .unwrap()
    };

    let r1 = post_cap().await;
    assert!(r1.status().is_success(), "capitalise: {}", r1.status());
    let v1: Value = r1.json().await.unwrap();

    // 1000 + 0.08 interest - 4.00 maintenance = 996.08.
    let after = account_balance(&c, &token, acct).await;
    assert!(
        (after - 996.08).abs() < 1e-6,
        "post-capitalisation balance should be 996.08, got {after}"
    );

    // Idempotent: re-running the period moves nothing and returns the same totals.
    let r2 = post_cap().await;
    assert!(r2.status().is_success(), "re-capitalise: {}", r2.status());
    let v2: Value = r2.json().await.unwrap();
    assert_eq!(
        v2["economic_event_id"], v1["economic_event_id"],
        "same event id on re-run"
    );
    let after2 = account_balance(&c, &token, acct).await;
    assert!(
        (after2 - 996.08).abs() < 1e-6,
        "re-run must not move the balance, got {after2}"
    );
}

/// A $100 card capture recognizes $1.50 interchange income (150 bps) in the GL.
#[tokio::test]
async fn card_capture_recognizes_interchange() {
    let c = client();
    require_stack!(&c);

    let email = create_customer(&c).await;
    let token = login(&c, &email).await;
    let card = create_account(&c, &token, "credit_card").await; // active, $5000 limit
    let svc = service_token(&c).await;

    let before = gl_balance_abs(&c, &["INTERCHANGE", "0000800200"]).await;

    // Authorize $100.
    let auth = c
        .post(format!("{}/api/v1/cards/authorize", base_url()))
        .bearer_auth(&svc)
        .json(&json!({ "account_id": card, "amount": 100.00, "merchant": "Test Merchant" }))
        .send()
        .await
        .unwrap();
    assert!(auth.status().is_success(), "authorize: {}", auth.status());
    let av: Value = auth.json().await.unwrap();
    assert_eq!(av["status"], "approved", "authorize should approve");
    let auth_id = av["auth_id"].as_str().expect("auth_id").to_string();

    // Capture it (posts the purchase GL + interchange).
    let cap = c
        .post(format!("{}/api/v1/cards/capture", base_url()))
        .bearer_auth(&svc)
        .json(&json!({ "auth_id": auth_id }))
        .send()
        .await
        .unwrap();
    if cap.status().as_u16() == 503 {
        eprintln!("SKIP: GL core unavailable (capture 503)");
        return;
    }
    assert!(cap.status().is_success(), "capture: {}", cap.status());

    let after = gl_balance_abs(&c, &["INTERCHANGE", "0000800200"]).await;
    assert!(
        (after - before - 1.50).abs() < 1e-6,
        "interchange income should rise by 1.50, before={before} after={after}"
    );
}

/// Sending an outgoing e-transfer charges the $1.50 fee: the sender is debited
/// the amount plus the fee, and fee income moves in the GL.
#[tokio::test]
async fn etransfer_charges_fee_income() {
    let c = client();
    require_stack!(&c);

    let email = create_customer(&c).await;
    let token = login(&c, &email).await;
    let acct = create_account(&c, &token, "chequing").await;
    if !deposit(&c, &token, acct, 1_000.0).await {
        eprintln!("SKIP: GL core unavailable (deposit 503)");
        return;
    }

    let fee_before = gl_balance_abs(&c, &["FEE_INCOME", "0000800300"]).await;

    // Send $50 to an external handle (security Q&A path; funds are held).
    let handle = format!("ext_{}@example.com", Uuid::new_v4().as_u128() % 1_000_000);
    let resp = c
        .post(format!("{}/api/v1/interac/etransfers", base_url()))
        .bearer_auth(&token)
        .json(&json!({
            "from_account_id": acct,
            "amount": 50.0,
            "recipient_handle_type": "email",
            "recipient_handle_value": handle,
            "security_question": "pet?",
            "security_answer": "rex"
        }))
        .send()
        .await
        .unwrap();
    if resp.status().as_u16() == 503 {
        eprintln!("SKIP: GL core unavailable (e-transfer 503)");
        return;
    }
    assert!(
        resp.status().is_success(),
        "send e-transfer: {}",
        resp.status()
    );

    // 1000 - 50 held - 1.50 fee = 948.50.
    let bal = account_balance(&c, &token, acct).await;
    assert!(
        (bal - 948.50).abs() < 1e-6,
        "balance should be 948.50 after send+fee, got {bal}"
    );

    let fee_after = gl_balance_abs(&c, &["FEE_INCOME", "0000800300"]).await;
    assert!(
        (fee_after - fee_before - 1.50).abs() < 1e-6,
        "fee income should rise by 1.50, before={fee_before} after={fee_after}"
    );
}
