//! Integration tests for the FraudCheck port.
//!
//! Same harness as `tests/transactions.rs`: every test probes `GET /health`
//! and **skips (still passes)** when the API isn't running.
//!
//! Two tiers:
//! - The baseline test runs in ANY fraud mode (off or engine): money movement
//!   must keep working — the port's first promise is zero behavior change by
//!   default.
//! - The engine-mode tests additionally require the fraud engine live and the
//!   bank started with `NANO_BANK__FRAUD__BACKEND=engine`; they skip unless
//!   `FRAUD_E2E=1` is set (the harness can't introspect the bank's backend).
//!
//! Run the full tier against a live stack:
//! ```bash
//! # engine repo: ./start-engine.sh   bank: NANO_BANK__FRAUD__BACKEND=engine cargo run
//! cd api && FRAUD_E2E=1 cargo test --test fraud_port -- --nocapture
//! ```
//! Overrides: `NANO_BANK_TEST_URL`, `NANO_BANK_TEST_DB_URL`,
//! `FRAUD_ENGINE_TEST_URL` (default http://localhost:8092),
//! `FRAUD_ADMIN_TOKEN` (default dev-admin-token).

use serde_json::{json, Value};
use uuid::Uuid;

const TEST_PASSWORD: &str = "securepass123";

fn base_url() -> String {
    std::env::var("NANO_BANK_TEST_URL").unwrap_or_else(|_| "http://localhost:8081".to_string())
}

fn engine_url() -> String {
    std::env::var("FRAUD_ENGINE_TEST_URL").unwrap_or_else(|_| "http://localhost:8092".to_string())
}

fn admin_token() -> String {
    std::env::var("FRAUD_ADMIN_TOKEN").unwrap_or_else(|_| "dev-admin-token".to_string())
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

async fn engine_up(c: &reqwest::Client) -> bool {
    matches!(
        c.get(format!("{}/health", engine_url())).send().await,
        Ok(r) if r.status().is_success()
    )
}

macro_rules! require_stack {
    ($c:expr) => {
        if !stack_up($c).await {
            eprintln!("SKIP: bank API not reachable");
            return;
        }
    };
}

macro_rules! require_fraud_e2e {
    ($c:expr) => {
        if std::env::var("FRAUD_E2E").as_deref() != Ok("1") {
            eprintln!(
                "SKIP: set FRAUD_E2E=1 (bank must run with NANO_BANK__FRAUD__BACKEND=engine)"
            );
            return;
        }
        if !engine_up($c).await {
            eprintln!("SKIP: fraud engine not reachable");
            return;
        }
    };
}

async fn create_customer(c: &reqwest::Client) -> (Uuid, String) {
    let n = Uuid::new_v4().as_u128();
    let email = format!("fraudtest_{}@example.com", n % 1_000_000_000);
    let body = json!({
        "email": email,
        "phone_number": format!("{:010}", (n % 10_000_000_000u128)),
        "first_name": "Fraud",
        "last_name": "Port",
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
    let v: Value = resp.json().await.unwrap();
    (
        Uuid::parse_str(v["customer_id"].as_str().unwrap()).unwrap(),
        email,
    )
}

/// Login carrying a device fingerprint — the context the fraud engine keys
/// device rules and blocklists on (recovered per-transaction via the session).
async fn login_with_device(c: &reqwest::Client, email: &str, device: &str) -> String {
    let resp = c
        .post(format!("{}/api/v1/auth/login", base_url()))
        .json(&json!({
            "email": email,
            "password": TEST_PASSWORD,
            "device_fingerprint": device
        }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "login: {}", resp.status());
    let v: Value = resp.json().await.unwrap();
    v["access_token"].as_str().unwrap().to_string()
}

async fn create_account(c: &reqwest::Client, token: &str) -> Uuid {
    let resp = c
        .post(format!("{}/api/v1/accounts", base_url()))
        .bearer_auth(token)
        .json(&json!({ "account_type": "chequing" }))
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

/// Deposit; skips (None) when the GL core is down — same convention as
/// `tests/transactions.rs::seed_deposit`.
async fn seed_deposit(c: &reqwest::Client, token: &str, account: Uuid, amount: f64) -> Option<()> {
    let resp = c
        .post(format!("{}/api/v1/transactions/deposit", base_url()))
        .bearer_auth(token)
        .json(&json!({ "account_id": account, "amount": amount, "description": "seed" }))
        .send()
        .await
        .unwrap();
    if resp.status().as_u16() == 503 {
        eprintln!("SKIP: GL core unavailable (deposit returned 503)");
        return None;
    }
    assert!(resp.status().is_success(), "deposit: {}", resp.status());
    Some(())
}

async fn transfer(
    c: &reqwest::Client,
    token: &str,
    from: Uuid,
    to: Uuid,
    amount: f64,
) -> reqwest::Response {
    c.post(format!("{}/api/v1/transactions/transfer", base_url()))
        .bearer_auth(token)
        .json(&json!({
            "from_account_id": from,
            "to_account_id": to,
            "amount": amount,
            "description": "fraud port test"
        }))
        .send()
        .await
        .unwrap()
}

async fn test_db() -> Option<sqlx::PgPool> {
    let url = std::env::var("NANO_BANK_TEST_DB_URL").unwrap_or_else(|_| {
        "postgres://nanobank_user:secure_nano_password_2024!@[::1]:5432/nano_bank_db".to_string()
    });
    match sqlx::PgPool::connect(&url).await {
        Ok(pool) => Some(pool),
        Err(e) => {
            eprintln!("SKIP DB assertions: {e}");
            None
        }
    }
}

/// Tier 1 — any mode: the port's default must not change bank behavior.
#[tokio::test]
async fn transfers_still_work_with_port_in_place() {
    let c = client();
    require_stack!(&c);
    let (_, email) = create_customer(&c).await;
    let token = login_with_device(&c, &email, "fraud-port-baseline-device").await;
    let from = create_account(&c, &token).await;
    let to = create_account(&c, &token).await;
    if seed_deposit(&c, &token, from, 500.0).await.is_none() {
        return;
    }
    let resp = transfer(&c, &token, from, to, 50.0).await;
    assert!(resp.status().is_success(), "transfer: {}", resp.status());
}

/// Tier 2 — engine mode: an allowed transfer carries the engine linkage in
/// `transactions.metadata.fraud` (decision_id proves the round trip).
#[tokio::test]
async fn engine_mode_stamps_decision_linkage() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let (_, email) = create_customer(&c).await;
    let token = login_with_device(&c, &email, format!("dev-{}", Uuid::new_v4()).as_str()).await;
    let from = create_account(&c, &token).await;
    let to = create_account(&c, &token).await;
    if seed_deposit(&c, &token, from, 500.0).await.is_none() {
        return;
    }
    let resp = transfer(&c, &token, from, to, 40.0).await;
    assert!(resp.status().is_success(), "transfer: {}", resp.status());
    let v: Value = resp.json().await.unwrap();
    let txn_id = Uuid::parse_str(v["transaction_id"].as_str().unwrap()).unwrap();

    let Some(pool) = test_db().await else { return };
    let (op_id, decision_id): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT metadata->'fraud'->>'operation_id', metadata->'fraud'->>'decision_id' \
         FROM transactions WHERE transaction_id = $1",
    )
    .bind(txn_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(op_id.is_some(), "fraud.operation_id stamped");
    assert!(
        decision_id.is_some(),
        "fraud.decision_id stamped (engine round trip)"
    );
}

/// Tier 2 — engine mode: a device the fraud engine blocklists makes the bank
/// refuse the movement with the opaque decline, before any money moves.
#[tokio::test]
async fn engine_mode_blocked_device_declines() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let device = format!("blocked-dev-{}", Uuid::new_v4());
    let (_, email) = create_customer(&c).await;
    let token = login_with_device(&c, &email, &device).await;
    let from = create_account(&c, &token).await;
    let to = create_account(&c, &token).await;
    if seed_deposit(&c, &token, from, 500.0).await.is_none() {
        return;
    }

    // Analyst blocks the device on the engine side...
    let created = c
        .post(format!("{}/admin/v1/lists", engine_url()))
        .bearer_auth(admin_token())
        .header("X-Actor", "fraud-port-e2e")
        .json(&json!({
            "list_name": "device_block",
            "entry_key": device,
            "reason": "fraud_port e2e test"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status().as_u16(), 201, "engine blocklist add");
    let entry: Value = created.json().await.unwrap();

    // ...and the bank now refuses this session's transfers, opaquely.
    let resp = transfer(&c, &token, from, to, 40.0).await;
    assert_eq!(resp.status().as_u16(), 403, "blocked device must 403");
    let v: Value = resp.json().await.unwrap();
    assert_eq!(v["error"]["code"], "TRANSACTION_DECLINED");

    // Cleanup: revoke so repeated runs stay independent.
    let revoked = c
        .delete(format!(
            "{}/admin/v1/lists/{}",
            engine_url(),
            entry["entry_id"].as_str().unwrap()
        ))
        .bearer_auth(admin_token())
        .header("X-Actor", "fraud-port-e2e")
        .send()
        .await
        .unwrap();
    assert_eq!(revoked.status().as_u16(), 204, "engine blocklist revoke");
}
