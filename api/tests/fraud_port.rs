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

async fn register_agent(c: &reqwest::Client) -> (Uuid, String) {
    let v: Value = c
        .post(format!("{}/api/v1/agents", base_url()))
        .json(&json!({"display_name": "Fraud Port Agent", "description": "fraud_port e2e"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    (
        Uuid::parse_str(v["agent_id"].as_str().unwrap()).unwrap(),
        v["agent_secret"].as_str().unwrap().to_string(),
    )
}

async fn mandated_agent_token(c: &reqwest::Client, token: &str, account: Uuid) -> (Uuid, String) {
    let (agent_id, secret) = register_agent(c).await;
    let granted: Value = c
        .post(format!("{}/api/v1/mandates", base_url()))
        .bearer_auth(token)
        .json(&json!({
            "agent_id": agent_id,
            "account_id": account,
            "scopes": ["transfer:initiate"],
            // Caps are mandatory with transfer:initiate; both sit above the test
            // amount so the refusal comes from the engine, not the step-up path.
            "max_per_tx": 100,
            "daily_cap": 500,
            "expires_at": (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mandate = Uuid::parse_str(granted["mandate_id"].as_str().unwrap()).unwrap();
    let issued: Value = c
        .post(format!("{}/api/v1/auth/agent-token", base_url()))
        .json(&json!({"agent_id": agent_id, "agent_secret": secret, "mandate_id": mandate}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    (
        mandate,
        issued["access_token"].as_str().unwrap().to_string(),
    )
}

async fn agent_transfer(
    c: &reqwest::Client,
    atoken: &str,
    to: Uuid,
    amount: f64,
) -> reqwest::Response {
    c.post(format!("{}/api/v1/agent/transfers", base_url()))
        .bearer_auth(atoken)
        .json(&json!({
            "to_account_id": to,
            "amount": amount,
            "description": "agent payment",
            "idempotency_key": Uuid::new_v4().to_string(),
        }))
        .send()
        .await
        .unwrap()
}

async fn engine_list_add(c: &reqwest::Client, list: &str, key: &str) -> Value {
    let created = c
        .post(format!("{}/admin/v1/lists", engine_url()))
        .bearer_auth(admin_token())
        .header("X-Actor", "fraud-port-e2e")
        .json(&json!({"list_name": list, "entry_key": key, "reason": "fraud_port e2e test"}))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status().as_u16(), 201, "engine list add");
    created.json().await.unwrap()
}

async fn engine_list_revoke(c: &reqwest::Client, entry: &Value) {
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
    assert_eq!(revoked.status().as_u16(), 204, "engine list revoke");
}

/// The engine's own database, for asserting what it did — or did not — record.
/// `None` with a SKIP note when unreachable, same convention as `test_db`.
async fn engine_db() -> Option<sqlx::PgPool> {
    let url = std::env::var("FRAUD_ENGINE_TEST_DB_URL")
        .unwrap_or_else(|_| "postgres://fraud:fraud@localhost:5436/fraud_engine".to_string());
    match sqlx::PgPool::connect(&url).await {
        Ok(pool) => Some(pool),
        Err(e) => {
            eprintln!("SKIP engine DB assertions: {e}");
            None
        }
    }
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

/// Tier 2 — engine mode: an engine refusal on the AGENT plane leaves exactly one
/// audit row, carrying the risk reason.
///
/// Regression guard. The gate used to audit the decline itself while the agent
/// handler's catch-all audited every failure too, so the owner's activity view
/// showed the real `RISK_REVIEW` beside a contradictory `denied / INTERNAL` — the
/// catch-all had no arm for the fraud errors. One writer, one row, right reason.
#[tokio::test]
async fn engine_mode_agent_refusal_audits_once() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let (_, email) = create_customer(&c).await;
    let token = login_with_device(&c, &email, &format!("agent-dev-{}", Uuid::new_v4())).await;
    let from = create_account(&c, &token).await;
    let to = create_account(&c, &token).await;

    // The engine watches the destination, so any payment to it is held. No funding
    // needed: screening happens before the money transaction opens.
    let entry = engine_list_add(&c, "account_watch", &to.to_string()).await;
    let (mandate, atoken) = mandated_agent_token(&c, &token, from).await;

    let resp = agent_transfer(&c, &atoken, to, 40.0).await;
    assert_eq!(resp.status().as_u16(), 403, "watched destination must 403");
    let v: Value = resp.json().await.unwrap();
    // The agent plane returns the OPAQUE refusal, not the specific review code:
    // `refusal_for_agent` (handlers/agent_api.rs) collapses every refusal —
    // including hold_review — to `TRANSFER_REFUSED`, because *why* a transfer was
    // refused is deliberately not the agent's business. The specific reason
    // survives for the granting customer in `agent_actions` (asserted below), not
    // in the HTTP body. Do not "fix" this back to TRANSACTION_UNDER_REVIEW.
    assert_eq!(v["error"]["code"], "TRANSFER_REFUSED");

    if let Some(db) = test_db().await {
        let rows: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT decision, reason FROM agent_actions \
             WHERE mandate_id = $1 AND operation = 'transfer' ORDER BY created_at",
        )
        .bind(mandate)
        .fetch_all(&db)
        .await
        .unwrap();
        assert_eq!(
            rows,
            vec![("denied".to_string(), Some("RISK_REVIEW".to_string()))],
            "exactly one audit row, with the risk reason"
        );
    }
    engine_list_revoke(&c, &entry).await;
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

/// Tier 2 — engine mode: a retried rail movement must NOT reach the engine again.
///
/// The four rail handlers used to screen *before* their idempotency replay, so a
/// bank retry of an already-posted AFT/Interac/Lynx movement re-invoked the
/// engine: velocity counted twice, a second decision row per retry, and above
/// `fail_closed_above` a 503 for a request that had already succeeded
/// (`design/INTEGRATION_DESIGN.md` §5 requires the replay to short-circuit
/// first). The ordering is invisible on inspection — two adjacent blocks — so it
/// gets a test rather than a comment.
///
/// Uses the AFT credit rail deliberately: originating accrues into the open batch
/// and moves no money until settlement, so the assertion needs no funded account
/// and runs where the funded-flow tests skip.
#[tokio::test]
async fn engine_mode_retried_rail_send_is_not_rescreened() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let (_, email) = create_customer(&c).await;
    let token = login_with_device(&c, &email, &format!("rail-dev-{}", Uuid::new_v4())).await;
    let from = create_account(&c, &token).await;

    // A FRESH counterparty per run, not a fixed one: the engine tracks payee-side
    // velocity, so a hardcoded account accumulates inbound attempts across runs
    // until `payee_inbound_24h_high` joins the novelty codes and the engine starts
    // holding the originate — a self-poisoning test.
    let counterparty_account = format!("{:07}", Uuid::new_v4().as_u128() % 10_000_000);
    let key = format!("rail-idem-{}", Uuid::new_v4());
    let body = json!({
        "originator_account_id": from,
        "counterparty_institution": "003",
        "counterparty_transit": "12345",
        "counterparty_account": counterparty_account,
        "payee_name": "Utility Co",
        "amount": 40.0,
        "idempotency_key": key,
    });
    let originate = |b: serde_json::Value| {
        let c = c.clone();
        let token = token.clone();
        async move {
            c.post(format!("{}/api/v1/aft/credits", base_url()))
                .bearer_auth(&token)
                .json(&b)
                .send()
                .await
                .unwrap()
        }
    };
    let first = originate(body.clone()).await;
    assert_eq!(first.status().as_u16(), 201, "first originate");
    let first_id = first.json::<Value>().await.unwrap()["entry_id"].clone();

    let replay = originate(body).await;
    assert_eq!(replay.status().as_u16(), 201, "replay returns the original");
    assert_eq!(
        replay.json::<Value>().await.unwrap()["entry_id"],
        first_id,
        "replay must be the same entry, not a second one"
    );

    // Counting decision ROWS cannot detect this: the engine is idempotent on the
    // same key, so a re-screened retry replays the stored decision instead of
    // inserting another. What it cannot undo is the velocity it already counted —
    // the engine records every assessed attempt before it notices the replay.
    //
    // So drive one further originate under a fresh key and read the velocity the
    // engine saw for this customer. Exactly one prior attempt means the retry
    // never reached it; two means it did.
    let probe_key = format!("rail-probe-{}", Uuid::new_v4());
    let probe = originate(json!({
        "originator_account_id": from,
        "counterparty_institution": "003",
        "counterparty_transit": "12345",
        "counterparty_account": counterparty_account,
        "payee_name": "Utility Co",
        "amount": 41.0,
        "idempotency_key": probe_key,
    }))
    .await;
    assert_eq!(probe.status().as_u16(), 201, "probe originate");

    if let Some(db) = engine_db().await {
        let vector: Value = sqlx::query_scalar(
            "SELECT feature_vector FROM decisions WHERE idempotency_key LIKE $1",
        )
        .bind(format!("%{probe_key}"))
        .fetch_one(&db)
        .await
        .unwrap();
        let counts: Vec<(String, i64)> = vector
            .as_object()
            .expect("feature vector object")
            .iter()
            .filter(|(k, _)| k.starts_with("velocity:customer_id:"))
            .map(|(k, v)| (k.clone(), v["count"].as_i64().unwrap_or(-1)))
            .collect();
        assert!(!counts.is_empty(), "no customer velocity in {vector}");
        for (key, count) in &counts {
            assert_eq!(
                *count, 1,
                "the engine should have seen ONE prior originate, not {count} ({key}) — \
                 a retry was re-screened and double-counted velocity"
            );
        }
    }
}

/// Mint a service-plane token — the fraud operator's identity.
async fn service_token(c: &reqwest::Client) -> String {
    let r = c
        .post(format!("{}/api/v1/auth/service-token", base_url()))
        .json(&json!({ "client_secret": "nano-bank-visa-network-secret-change-me" }))
        .send()
        .await
        .unwrap();
    assert!(r.status().is_success(), "service-token: {}", r.status());
    let v: Value = r.json().await.unwrap();
    v["access_token"].as_str().unwrap().to_string()
}

async fn fraud_link(c: &reqwest::Client, token: &str, txn: Uuid) -> reqwest::Response {
    c.get(format!(
        "{}/api/v1/fraud/admin/transactions/{txn}/fraud-link",
        base_url()
    ))
    .bearer_auth(token)
    .send()
    .await
    .unwrap()
}

/// Tier 2 — engine mode: the linkage endpoint hands the engine's `operation_id`
/// to a service caller (#46).
///
/// This is the key the whole label path turns on. The engine joins ground truth
/// on `outcome_events.operation_id = decisions.operation_id` and has no
/// `transaction_id` column to fall back on, so until this endpoint existed the
/// id was written to `transactions.metadata` and read by nobody — no decision
/// could be labelled, and the training-set export returned zero rows.
///
/// Asserted against the database rather than merely "is a UUID": the endpoint
/// returning *some* well-formed id that isn't the one the engine recorded would
/// be worse than returning nothing, because every downstream label would attach
/// to the wrong decision.
#[tokio::test]
async fn fraud_link_exposes_the_engine_operation_id_to_a_service_caller() {
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

    // The customer plane must not carry it — that is the disclosure decision
    // this endpoint exists to honour (#46: service plane only).
    assert!(
        v.get("metadata").is_none_or(Value::is_null),
        "engine ids must not reach the customer plane: {v}"
    );

    let svc = service_token(&c).await;
    let link = fraud_link(&c, &svc, txn_id).await;
    assert_eq!(link.status().as_u16(), 200);
    let link: Value = link.json().await.unwrap();

    let Some(pool) = test_db().await else { return };
    let (op_id, decision_id): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT metadata->'fraud'->>'operation_id', metadata->'fraud'->>'decision_id' \
         FROM transactions WHERE transaction_id = $1",
    )
    .bind(txn_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        link["operation_id"].as_str(),
        op_id.as_deref(),
        "must return the id the engine actually recorded: {link}"
    );
    assert_eq!(link["decision_id"].as_str(), decision_id.as_deref());
    assert_eq!(link["transaction_id"].as_str().unwrap(), txn_id.to_string());
}

/// The linkage is service-plane only: a customer token is refused even for the
/// customer's own transaction. Without this the endpoint would be a way to read
/// engine internals from the customer plane — the thing #46 chose against.
#[tokio::test]
async fn fraud_link_refuses_a_customer_token() {
    let c = client();
    require_stack!(&c);
    let (_, email) = create_customer(&c).await;
    let token = login_with_device(&c, &email, format!("dev-{}", Uuid::new_v4()).as_str()).await;
    let from = create_account(&c, &token).await;
    let to = create_account(&c, &token).await;
    if seed_deposit(&c, &token, from, 500.0).await.is_none() {
        return;
    }
    let resp = transfer(&c, &token, from, to, 25.0).await;
    assert!(resp.status().is_success());
    let v: Value = resp.json().await.unwrap();
    let txn_id = Uuid::parse_str(v["transaction_id"].as_str().unwrap()).unwrap();

    let status = fraud_link(&c, &token, txn_id).await.status().as_u16();
    assert_eq!(status, 403, "a customer token is the wrong plane");
}

/// An unscreened transaction answers 200 with nulls, not 404.
///
/// "This transaction has no fraud link" is a true answer — screening is off by
/// default, and several rails do not screen at all. A 404 would tell a caller
/// the transaction does not exist and invite it to retry something that is never
/// going to appear.
///
/// Runs WITHOUT `require_fraud_e2e`: it needs the backend off, which is the
/// shipped default, so this is the one linkage test that exercises the
/// unscreened branch.
#[tokio::test]
async fn fraud_link_is_null_for_an_unscreened_transaction() {
    let c = client();
    require_stack!(&c);
    if std::env::var("FRAUD_E2E").as_deref() == Ok("1") {
        eprintln!("SKIP: needs the fraud backend off (the default)");
        return;
    }
    let (_, email) = create_customer(&c).await;
    let token = login_with_device(&c, &email, format!("dev-{}", Uuid::new_v4()).as_str()).await;
    let from = create_account(&c, &token).await;
    let to = create_account(&c, &token).await;
    if seed_deposit(&c, &token, from, 500.0).await.is_none() {
        return;
    }
    let resp = transfer(&c, &token, from, to, 30.0).await;
    assert!(resp.status().is_success());
    let v: Value = resp.json().await.unwrap();
    let txn_id = Uuid::parse_str(v["transaction_id"].as_str().unwrap()).unwrap();

    let svc = service_token(&c).await;
    let link = fraud_link(&c, &svc, txn_id).await;
    assert_eq!(link.status().as_u16(), 200, "unscreened is not 'not found'");
    let link: Value = link.json().await.unwrap();
    assert!(link["operation_id"].is_null(), "{link}");
    assert!(link["decision_id"].is_null(), "{link}");
    assert_eq!(link["failed_open"], false);
}

/// An unknown transaction is a 404 — the one case that IS "not found".
#[tokio::test]
async fn fraud_link_404s_for_an_unknown_transaction() {
    let c = client();
    require_stack!(&c);
    let svc = service_token(&c).await;
    let status = fraud_link(&c, &svc, Uuid::new_v4()).await.status().as_u16();
    assert_eq!(status, 404);
}

/// A screened **rail** movement resolves to the engine's decision (#52).
///
/// This test used to pin the opposite. Interac, AFT and Lynx create real
/// `transactions` rows and call `screen()`, but never stamped
/// `metadata.fraud` — so their decisions, possibly **blocks**, were unreachable
/// and `fraud-link` answered nulls indistinguishable from "never screened".
/// That is what made a null uninterpretable, and it is now fixed for the rails
/// whose screening and money movement share a request.
///
/// Asserted against the engine's own `decisions` row, not merely "is a UUID":
/// a well-formed id that points at the wrong decision is worse than a null,
/// because every label downstream attaches silently to the wrong thing.
#[tokio::test]
async fn fraud_link_resolves_a_screened_rail_movement() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let (_, email) = create_customer(&c).await;
    let token = login_with_device(&c, &email, format!("dev-{}", Uuid::new_v4()).as_str()).await;
    let account = create_account(&c, &token).await;
    if seed_deposit(&c, &token, account, 5000.0).await.is_none() {
        return;
    }

    let sent = c
        .post(format!("{}/api/v1/interac/etransfers", base_url()))
        .bearer_auth(&token)
        .json(&json!({
            "from_account_id": account,
            "amount": 60.00,
            "recipient_handle_type": "email",
            "recipient_handle_value": format!("rail-{}@example.com", Uuid::new_v4()),
            "security_question": "q",
            "security_answer": "a",
            "idempotency_key": format!("rail-{}", Uuid::new_v4()),
        }))
        .send()
        .await
        .unwrap();
    assert!(
        sent.status().is_success(),
        "interac send: {}",
        sent.status()
    );

    // The rail hands back an `etransfer_id`; the row that was screened is the
    // `interac_hold` it wrote through `new_txn`.
    //
    // Scoped to the e-Transfer THIS test sent (#73, mechanism (a)). It used to
    // reach for the newest `interac_%` row in the whole table:
    //
    //     ORDER BY created_at DESC LIMIT 1
    //
    // Any concurrently-running Interac test could be that row, leaving this one
    // asserting about a movement it did not create — so it passed or failed on
    // scheduling, and could pass while describing someone else's money. Since
    // #66 the send response carries the `etransfer_id` and the row points at
    // its own hold, so the global query buys nothing.
    let sent_body: Value = sent.json().await.unwrap();
    let etransfer_id = Uuid::parse_str(sent_body["etransfer_id"].as_str().unwrap()).unwrap();
    let Some(pool) = test_db().await else { return };
    let (txn_id,): (Uuid,) = sqlx::query_as(
        "SELECT hold_transaction_id FROM interac_etransfers WHERE etransfer_id = $1",
    )
    .bind(etransfer_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let svc = service_token(&c).await;
    let link = fraud_link(&c, &svc, txn_id).await;
    assert_eq!(link.status().as_u16(), 200);
    let link: Value = link.json().await.unwrap();
    let op_id = link["operation_id"]
        .as_str()
        .unwrap_or_else(|| panic!("a screened rail movement must resolve: {link}"));

    // It has to be the decision the ENGINE recorded, not just a well-formed id.
    let Some(engine) = engine_db().await else {
        return;
    };
    let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM decisions WHERE operation_id = $1")
        .bind(Uuid::parse_str(op_id).unwrap())
        .fetch_one(&engine)
        .await
        .unwrap();
    assert_eq!(
        seen, 1,
        "operation_id {op_id} must name a real engine decision"
    );
}

/// A settled **AFT** movement resolves to the decision made at origination (#54).
///
/// AFT is the last of the split-request rails: `create_credit` screens and
/// writes only an `aft_entries` row — no `transactions` row exists until the
/// batch settles, which is a separate service-plane request where the
/// `FraudLink` is long gone. So the engine's ruling on a direct deposit,
/// possibly a **block**, was dropped, and `fraud-link` answered nulls
/// indistinguishable from "never screened".
///
/// Two assertions, and the second is the one a naive fix gets wrong: the id must
/// name a real engine decision, **and** be the one minted at origination.
/// Settlement does not re-screen, so a different id here would mean something
/// screened silently.
#[tokio::test]
async fn fraud_link_resolves_a_settled_aft_movement() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let (_, email) = create_customer(&c).await;
    let token = login_with_device(&c, &email, format!("dev-{}", Uuid::new_v4()).as_str()).await;
    let account = create_account(&c, &token).await;
    if seed_deposit(&c, &token, account, 5000.0).await.is_none() {
        return;
    }

    let credit = c
        .post(format!("{}/api/v1/aft/credits", base_url()))
        .bearer_auth(&token)
        .json(&json!({
            "originator_account_id": account,
            "amount": 250.00,
            "counterparty_institution": "003",
            "counterparty_transit": "12345",
            "counterparty_account": "9876543",
            "payee_name": "Linkage Payroll",
            "idempotency_key": format!("aft-{}", Uuid::new_v4()),
        }))
        .send()
        .await
        .unwrap();
    assert!(
        credit.status().is_success(),
        "aft credit: {}",
        credit.status()
    );
    let cv: Value = credit.json().await.unwrap();
    let entry_id = Uuid::parse_str(cv["entry_id"].as_str().expect("entry_id")).unwrap();

    // The decision exists now, parked on the entry — settlement only executes it.
    let Some(pool) = test_db().await else { return };
    let (stored,): (Option<Value>,) =
        sqlx::query_as("SELECT metadata FROM aft_entries WHERE entry_id = $1")
            .bind(entry_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let originated_op = stored
        .as_ref()
        .and_then(|m| m.get("fraud"))
        .and_then(|f| f.get("operation_id"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("origination must record its linkage: {stored:?}"))
        .to_string();

    // Submit and settle the batch the entry landed in.
    let (batch_id,): (Uuid,) =
        sqlx::query_as("SELECT batch_id FROM aft_entries WHERE entry_id = $1")
            .bind(entry_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let svc = service_token(&c).await;
    let submit = c
        .post(format!(
            "{}/api/v1/aft/batches/{batch_id}/submit",
            base_url()
        ))
        .bearer_auth(&svc)
        .send()
        .await
        .unwrap();
    assert!(submit.status().is_success(), "submit: {}", submit.status());
    let settle = c
        .post(format!(
            "{}/api/v1/aft/network/settle/{batch_id}",
            base_url()
        ))
        .bearer_auth(&svc)
        .send()
        .await
        .unwrap();
    assert!(settle.status().is_success(), "settle: {}", settle.status());

    let (settle_txn,): (Option<Uuid>,) =
        sqlx::query_as("SELECT settle_transaction_id FROM aft_entries WHERE entry_id = $1")
            .bind(entry_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let settle_txn = settle_txn.expect("settlement must record its transaction");

    let link = fraud_link(&c, &svc, settle_txn).await;
    assert_eq!(link.status().as_u16(), 200);
    let link: Value = link.json().await.unwrap();
    let op_id = link["operation_id"]
        .as_str()
        .unwrap_or_else(|| panic!("a settled AFT movement must resolve: {link}"));

    assert_eq!(
        op_id, originated_op,
        "settlement must carry the origination decision, not a new screening"
    );

    let Some(engine) = engine_db().await else {
        return;
    };
    let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM decisions WHERE operation_id = $1")
        .bind(Uuid::parse_str(op_id).unwrap())
        .fetch_one(&engine)
        .await
        .unwrap();
    assert_eq!(
        seen, 1,
        "operation_id {op_id} must name a real engine decision"
    );
}

/// A captured **card purchase** resolves to the decision made at authorize (#54).
///
/// Cards are the awkward case: `screen()` runs in `authorize`, but the
/// `transactions` row is not written until `capture` — a separate request where
/// the `FraudLink` no longer exists. Before this the engine's ruling on a card
/// purchase, possibly a **block**, was simply dropped, and `fraud-link` answered
/// nulls indistinguishable from "never screened".
///
/// Two things are asserted, and the second is the one a naive fix gets wrong:
/// the id must name a real engine decision, **and** it must be the one minted at
/// authorize. Capture does not re-screen — it settles a decision already made —
/// so a *different* id appearing here would mean something screened silently.
#[tokio::test]
async fn fraud_link_resolves_a_captured_card_purchase() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let (_, email) = create_customer(&c).await;
    let token = login_with_device(&c, &email, format!("dev-{}", Uuid::new_v4()).as_str()).await;
    let card = c
        .post(format!("{}/api/v1/accounts", base_url()))
        .bearer_auth(&token)
        .json(&json!({ "account_type": "credit_card" }))
        .send()
        .await
        .unwrap();
    assert!(card.status().is_success(), "create card: {}", card.status());
    let cv: Value = card.json().await.unwrap();
    let card_id = Uuid::parse_str(cv["account_id"].as_str().unwrap()).unwrap();

    let svc = service_token(&c).await;
    let auth = c
        .post(format!("{}/api/v1/cards/authorize", base_url()))
        .bearer_auth(&svc)
        .json(&json!({ "account_id": card_id, "amount": 42.50, "merchant": "Linkage Co" }))
        .send()
        .await
        .unwrap();
    assert!(auth.status().is_success(), "authorize: {}", auth.status());
    let av: Value = auth.json().await.unwrap();
    assert_eq!(av["status"], "approved", "authorize should approve: {av}");
    let auth_id = av["auth_id"].as_str().expect("auth_id").to_string();

    // The decision was made above; capture only settles it.
    let Some(pool) = test_db().await else { return };
    let (held,): (Option<Value>,) =
        sqlx::query_as("SELECT metadata FROM account_holds WHERE hold_id = $1")
            .bind(Uuid::parse_str(&auth_id).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    let authorized_op = held
        .as_ref()
        .and_then(|m| m.get("fraud"))
        .and_then(|f| f.get("operation_id"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("authorize must record its linkage on the hold: {held:?}"))
        .to_string();

    let cap = c
        .post(format!("{}/api/v1/cards/capture", base_url()))
        .bearer_auth(&svc)
        .json(&json!({ "auth_id": auth_id }))
        .send()
        .await
        .unwrap();
    assert!(cap.status().is_success(), "capture: {}", cap.status());
    let cvj: Value = cap.json().await.unwrap();
    let txn_id = Uuid::parse_str(
        cvj["transaction_id"]
            .as_str()
            .unwrap_or_else(|| panic!("capture must return a transaction_id: {cvj}")),
    )
    .unwrap();

    let link = fraud_link(&c, &svc, txn_id).await;
    assert_eq!(link.status().as_u16(), 200);
    let link: Value = link.json().await.unwrap();
    let op_id = link["operation_id"]
        .as_str()
        .unwrap_or_else(|| panic!("a captured card purchase must resolve: {link}"));

    assert_eq!(
        op_id, authorized_op,
        "the purchase must carry the decision from authorize, not a new screening"
    );

    let Some(engine) = engine_db().await else {
        return;
    };
    let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM decisions WHERE operation_id = $1")
        .bind(Uuid::parse_str(op_id).unwrap())
        .fetch_one(&engine)
        .await
        .unwrap();
    assert_eq!(
        seen, 1,
        "operation_id {op_id} must name a real engine decision"
    );
}

// ---------------------------------------------------------------------------
// Rail fraud-link resolver: /admin/rails/{rail}/{rail_id}/fraud-link
// ---------------------------------------------------------------------------

async fn fraud_link_rail(
    c: &reqwest::Client,
    token: &str,
    rail: &str,
    rail_id: Uuid,
) -> reqwest::Response {
    c.get(format!(
        "{}/api/v1/fraud/admin/rails/{rail}/{rail_id}/fraud-link",
        base_url()
    ))
    .bearer_auth(token)
    .send()
    .await
    .unwrap()
}

/// The resolver maps a rail's own id to the screened decision — same answer the
/// transaction route gives, without the caller having to know the money
/// `transaction_id`. Asserted against the engine's `decisions` table, not merely
/// "is a UUID": a well-formed id that isn't the one the engine recorded is worse
/// than a null.
#[tokio::test]
async fn rail_fraud_link_resolves_an_interac_etransfer() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let (_, email) = create_customer(&c).await;
    let token = login_with_device(&c, &email, format!("dev-{}", Uuid::new_v4()).as_str()).await;
    let account = create_account(&c, &token).await;
    if seed_deposit(&c, &token, account, 5000.0).await.is_none() {
        return;
    }
    let sent = c
        .post(format!("{}/api/v1/interac/etransfers", base_url()))
        .bearer_auth(&token)
        .json(&json!({
            "from_account_id": account,
            "amount": 60.00,
            "recipient_handle_type": "email",
            "recipient_handle_value": format!("rail-{}@example.com", Uuid::new_v4()),
            "security_question": "q",
            "security_answer": "a",
            "idempotency_key": format!("rail-{}", Uuid::new_v4()),
        }))
        .send()
        .await
        .unwrap();
    assert!(
        sent.status().is_success(),
        "interac send: {}",
        sent.status()
    );
    let etransfer_id = Uuid::parse_str(
        sent.json::<Value>().await.unwrap()["etransfer_id"]
            .as_str()
            .unwrap(),
    )
    .unwrap();

    let svc = service_token(&c).await;
    let link = fraud_link_rail(&c, &svc, "interac", etransfer_id).await;
    assert_eq!(link.status().as_u16(), 200);
    let op_id = link.json::<Value>().await.unwrap()["operation_id"]
        .as_str()
        .unwrap_or_else(|| panic!("interac etransfer must resolve"))
        .to_string();
    assert_engine_decision_exists(&c, &op_id).await;
}

/// Lynx: the linkage sits on the send-time hold (`settlement_transaction_id`),
/// stamped at `/wires`; reachable straight away, no network settle needed.
#[tokio::test]
async fn rail_fraud_link_resolves_a_lynx_wire() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let (_, email) = create_customer(&c).await;
    let token = login_with_device(&c, &email, format!("dev-{}", Uuid::new_v4()).as_str()).await;
    let account = create_account(&c, &token).await;
    if seed_deposit(&c, &token, account, 50000.0).await.is_none() {
        return;
    }
    let sent = c
        .post(format!("{}/api/v1/lynx/wires", base_url()))
        .bearer_auth(&token)
        .json(&json!({
            "from_account_id": account,
            "amount": 15000.00,
            "counterparty_name": "Acme Corp",
            "counterparty_institution": "003",
            "counterparty_account": "9876543",
            "idempotency_key": format!("rail-{}", Uuid::new_v4()),
        }))
        .send()
        .await
        .unwrap();
    assert!(sent.status().is_success(), "lynx wire: {}", sent.status());
    let sv: Value = sent.json().await.unwrap();
    let wire_id = Uuid::parse_str(sv["wire_id"].as_str().unwrap()).unwrap();

    let svc = service_token(&c).await;
    let link = fraud_link_rail(&c, &svc, "lynx", wire_id).await;
    assert_eq!(link.status().as_u16(), 200);
    let op_id = link.json::<Value>().await.unwrap()["operation_id"]
        .as_str()
        .unwrap_or_else(|| panic!("lynx wire must resolve"))
        .to_string();
    assert_engine_decision_exists(&c, &op_id).await;
}

/// AFT: keyed on the **entry** id (a batch has many); the linkage lands on
/// `settle_transaction_id`, so the batch is submitted + settled first.
#[tokio::test]
async fn rail_fraud_link_resolves_an_aft_entry() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let (_, email) = create_customer(&c).await;
    let token = login_with_device(&c, &email, format!("dev-{}", Uuid::new_v4()).as_str()).await;
    let account = create_account(&c, &token).await;
    if seed_deposit(&c, &token, account, 5000.0).await.is_none() {
        return;
    }
    let credit = c
        .post(format!("{}/api/v1/aft/credits", base_url()))
        .bearer_auth(&token)
        .json(&json!({
            "originator_account_id": account,
            "amount": 250.00,
            "counterparty_institution": "003",
            "counterparty_transit": "12345",
            "counterparty_account": "9876543",
            "payee_name": "Rail Payroll",
            "idempotency_key": format!("rail-{}", Uuid::new_v4()),
        }))
        .send()
        .await
        .unwrap();
    let credit_status = credit.status();
    assert!(credit_status.is_success(), "aft credit: {credit_status}");
    let cv: Value = credit.json().await.unwrap();
    let entry_id = Uuid::parse_str(cv["entry_id"].as_str().unwrap()).unwrap();
    let batch_id = Uuid::parse_str(cv["batch_id"].as_str().unwrap()).unwrap();

    let svc = service_token(&c).await;
    for path in [
        format!("aft/batches/{batch_id}/submit"),
        format!("aft/network/settle/{batch_id}"),
    ] {
        let r = c
            .post(format!("{}/api/v1/{path}", base_url()))
            .bearer_auth(&svc)
            .send()
            .await
            .unwrap();
        assert!(r.status().is_success(), "{path}: {}", r.status());
    }

    let link = fraud_link_rail(&c, &svc, "aft", entry_id).await;
    assert_eq!(link.status().as_u16(), 200);
    let op_id = link.json::<Value>().await.unwrap()["operation_id"]
        .as_str()
        .unwrap_or_else(|| panic!("settled aft entry must resolve"))
        .to_string();
    assert_engine_decision_exists(&c, &op_id).await;
}

/// An unknown rail name is a client error, distinct from an unknown id.
#[tokio::test]
async fn rail_fraud_link_400_for_an_unknown_rail() {
    let c = client();
    require_stack!(&c);
    let svc = service_token(&c).await;
    // Was "cards", which #70 makes a real arm. A rail name has to be one the
    // bank genuinely does not serve for this to assert anything.
    let r = fraud_link_rail(&c, &svc, "cheque", Uuid::new_v4()).await;
    assert_eq!(r.status().as_u16(), 400, "unknown rail is a 400");
}

/// An unknown rail id (or one whose money row isn't written yet) is a 404.
#[tokio::test]
async fn rail_fraud_link_404_for_an_unknown_id() {
    let c = client();
    require_stack!(&c);
    let svc = service_token(&c).await;
    let r = fraud_link_rail(&c, &svc, "interac", Uuid::new_v4()).await;
    assert_eq!(r.status().as_u16(), 404, "unknown rail id is a 404");
}

/// Shared: the resolved operation_id must name a real row in the engine's
/// `decisions` table — a valid-but-wrong id is worse than a null.
async fn assert_engine_decision_exists(_c: &reqwest::Client, op_id: &str) {
    let Some(engine) = engine_db().await else {
        return;
    };
    let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM decisions WHERE operation_id = $1")
        .bind(Uuid::parse_str(op_id).unwrap())
        .fetch_one(&engine)
        .await
        .unwrap();
    assert_eq!(
        seen, 1,
        "operation_id {op_id} must name a real engine decision"
    );
}

/// Tier 2 — engine mode: a card `auth_id` resolves through the rail route (#70).
///
/// Cards were deferred from #66's uniform match because they are the one rail
/// whose linkage is not a column on a row keyed by the rail id: screening
/// happens at authorize, the money row appears at capture, and the join is the
/// `auth_id` stamped into `transactions.metadata`. Until this arm existed, a
/// realize run came back with ~45% of its movements unlinkable — the decisions
/// were made, they just could not be addressed.
///
/// Asserted as *equal to* the transaction-keyed answer rather than merely
/// non-null: two routes disagreeing about one money row is worse than one of
/// them not existing, because a label attaches to whichever the caller asked.
#[tokio::test]
async fn rail_fraud_link_resolves_a_captured_card_by_auth_id() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let (_, email) = create_customer(&c).await;
    let token = login_with_device(&c, &email, format!("dev-{}", Uuid::new_v4()).as_str()).await;
    let card = c
        .post(format!("{}/api/v1/accounts", base_url()))
        .bearer_auth(&token)
        .json(&json!({ "account_type": "credit_card" }))
        .send()
        .await
        .unwrap();
    assert!(card.status().is_success(), "create card: {}", card.status());
    let cv: Value = card.json().await.unwrap();
    let card_id = Uuid::parse_str(cv["account_id"].as_str().unwrap()).unwrap();

    let svc = service_token(&c).await;
    let auth = c
        .post(format!("{}/api/v1/cards/authorize", base_url()))
        .bearer_auth(&svc)
        .json(&json!({ "account_id": card_id, "amount": 61.25, "merchant": "Rail Cards Co" }))
        .send()
        .await
        .unwrap();
    assert!(auth.status().is_success(), "authorize: {}", auth.status());
    let av: Value = auth.json().await.unwrap();
    assert_eq!(av["status"], "approved", "authorize should approve: {av}");
    let auth_id = Uuid::parse_str(av["auth_id"].as_str().expect("auth_id")).unwrap();

    // Before capture there is no money row, so there is nothing to resolve —
    // the same state an unsettled AFT entry is in, and it must 404 rather than
    // 500 or answer nulls.
    let before = fraud_link_rail(&c, &svc, "cards", auth_id).await;
    assert_eq!(
        before.status().as_u16(),
        404,
        "an authorized-but-uncaptured card has no money row yet"
    );

    let cap = c
        .post(format!("{}/api/v1/cards/capture", base_url()))
        .bearer_auth(&svc)
        .json(&json!({ "auth_id": auth_id }))
        .send()
        .await
        .unwrap();
    assert!(cap.status().is_success(), "capture: {}", cap.status());
    let cvj: Value = cap.json().await.unwrap();
    let txn_id = Uuid::parse_str(cvj["transaction_id"].as_str().expect("transaction_id")).unwrap();

    let by_rail: Value = fraud_link_rail(&c, &svc, "cards", auth_id)
        .await
        .json()
        .await
        .unwrap();
    let op_id = by_rail["operation_id"]
        .as_str()
        .unwrap_or_else(|| panic!("a captured card must resolve: {by_rail}"));

    let by_txn: Value = fraud_link(&c, &svc, txn_id).await.json().await.unwrap();
    assert_eq!(by_rail, by_txn, "the two entry points must not disagree");

    // And it names a decision the engine actually recorded.
    let Some(engine) = engine_db().await else {
        return;
    };
    let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM decisions WHERE operation_id = $1")
        .bind(Uuid::parse_str(op_id).unwrap())
        .fetch_one(&engine)
        .await
        .unwrap();
    assert_eq!(
        seen, 1,
        "operation_id {op_id} must name a real engine decision"
    );
}

/// The cards arm sits on the service plane like every other (#46).
#[tokio::test]
async fn rail_fraud_link_cards_refuses_a_customer_token() {
    let c = client();
    require_stack!(&c);
    let (_, email) = create_customer(&c).await;
    let token = login_with_device(&c, &email, format!("dev-{}", Uuid::new_v4()).as_str()).await;

    let r = fraud_link_rail(&c, &token, "cards", Uuid::new_v4()).await;
    assert!(
        matches!(r.status().as_u16(), 401 | 403),
        "a customer token must not reach a card linkage: {}",
        r.status()
    );
}

// ---------------------------------------------------------------------------
// Held movements: parking, and what a verdict does to a parked movement (#74)
// ---------------------------------------------------------------------------
//
// These are the tests that say the engine can *act*. Before them a hold was a
// dead end: the bank declined, the money went home, and a reviewer clearing the
// case changed nothing for the customer.
//
// Every one of them asserts on **money**, not on the poll response, wherever
// money is the point. A poll that says "executed" while the balance is
// unchanged is exactly the failure worth catching, and a response-only
// assertion cannot see it.

/// Ask the bank what became of a held movement.
async fn poll_review(c: &reqwest::Client, token: &str, review: Uuid) -> Value {
    let r = c
        .get(format!("{}/api/v1/reviews/{}", base_url(), review))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 200, "review poll");
    r.json().await.unwrap()
}

async fn balance_of(c: &reqwest::Client, token: &str, account: Uuid) -> f64 {
    let v: Value = c
        .get(format!("{}/api/v1/accounts/{}", base_url(), account))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    v["balance"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| v["balance"].as_f64())
        .unwrap_or_else(|| panic!("no balance in {v}"))
}

/// Park a held transfer, deterministically.
///
/// The hold comes from the `account_watch` list — rules_v1's
/// `watched_destination_account`, a hard-rule gate whose description is exactly
/// this scenario: *"Destination account under active investigation (suspected
/// mule). Hold rather than block: analysts decide, and the sender may be a
/// victim we need to talk to."* A parked movement is what "analysts decide"
/// requires.
///
/// It is driven by an analyst action the test controls, which is the point.
/// An earlier version of this fixture sent $1,500 and hoped policy_v3's
/// `first_payment_new_payee_above_baseline` would catch it. That rule needs
/// `amount_above_p90`, which is relative to *this customer's own* history — a
/// fresh account has none, so nothing held, the fixture skipped, and six tests
/// reported `ok` while asserting nothing whatsoever. Never again: this returns
/// a park or it fails.
///
/// Returns `(review_id, from, to, token, list_entry)`; the caller revokes the
/// entry so repeated runs stay independent.
async fn park_a_held_transfer(c: &reqwest::Client) -> Option<(Uuid, Uuid, Uuid, String, Value)> {
    // Bounded retry, because a miss here has an innocent cause worth
    // distinguishing from a bug. At $120 the movement is below
    // `fail_closed_above`, so when this suite's ~24 concurrent tests burst at a
    // single engine and one screening call exceeds its 150ms budget, the gate
    // fails OPEN and the transfer posts. That is the configured behaviour, not
    // a parking defect — the pre-existing `engine_mode_blocked_device_declines`
    // flakes the same way for the same reason.
    //
    // Three attempts, each with a fresh customer and destination. If every one
    // posts, that is a real failure and the assertion says what it saw.
    let mut last = String::new();
    for _ in 0..3 {
        let (_, email) = create_customer(c).await;
        let token = login_with_device(c, &email, &format!("dev-{}", Uuid::new_v4())).await;
        let from = create_account(c, &token).await;
        let to = create_account(c, &token).await;
        seed_deposit(c, &token, from, 5000.0).await?;

        // The analyst flags the destination. `payee` is the destination account
        // UUID for an internal transfer (the counterparty handle for the
        // external rails), so this is the subject the gate keys on.
        let entry = engine_list_add(c, "account_watch", &to.to_string()).await;

        let resp = transfer(c, &token, from, to, 120.0).await;
        let status = resp.status().as_u16();
        let body: Value = resp.json().await.unwrap();
        if status != 202 {
            last = format!("{status}: {body}");
            engine_list_revoke(c, &entry).await;
            continue;
        }
        let review = Uuid::parse_str(body["review_id"].as_str().unwrap()).unwrap();
        assert_eq!(body["status"], "held", "a fresh park is held: {body}");
        assert!(
            body["transaction_id"].is_null(),
            "nothing has posted yet: {body}"
        );
        return Some((review, from, to, token, entry));
    }
    panic!("a watched destination never parked in three attempts; last was {last}");
}

/// Resolve the engine case behind a parked movement, and return whether we
/// could (the engine DB is optional in this harness).
async fn resolve_case_for(c: &reqwest::Client, review: Uuid, verdict: &str) -> bool {
    let Some(bank) = test_db().await else {
        return false;
    };
    let (op_id,): (Uuid,) =
        sqlx::query_as("SELECT operation_id FROM pending_reviews WHERE review_id = $1")
            .bind(review)
            .fetch_one(&bank)
            .await
            .unwrap();

    let Some(engine) = engine_db().await else {
        return false;
    };
    let case: Option<(Uuid,)> = sqlx::query_as("SELECT case_id FROM cases WHERE operation_id = $1")
        .bind(op_id)
        .fetch_optional(&engine)
        .await
        .unwrap();
    let Some((case_id,)) = case else {
        panic!("a held movement must have opened a case (operation {op_id})");
    };

    let r = c
        .post(format!(
            "{}/admin/v1/cases/{}/resolution",
            engine_url(),
            case_id
        ))
        .bearer_auth(admin_token())
        .header("X-Actor", "fraud-port-e2e")
        .json(&json!({ "verdict": verdict, "note": "fraud_port e2e" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 201, "case resolution");
    true
}

/// Tier 2 — the whole point: a held transfer PARKS rather than declining.
///
/// A 403 here is the old dead end. The money must still be in the sending
/// account (nothing has moved) and the review must be addressable.
#[tokio::test]
async fn engine_mode_a_held_transfer_parks_instead_of_declining() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let Some((review, from, _to, token, watch)) = park_a_held_transfer(&c).await else {
        return;
    };

    // Parked, not posted: the sender still has every cent.
    assert_eq!(
        balance_of(&c, &token, from).await,
        5000.0,
        "a parked movement must not have moved money"
    );

    let v = poll_review(&c, &token, review).await;
    assert_eq!(v["status"], "held", "still waiting on a reviewer: {v}");
    assert!(v["transaction_id"].is_null());
    engine_list_revoke(&c, &watch).await;
}

/// Tier 2 — a cleared case RELEASES the money. This is the assertion the whole
/// mechanism exists for, and it is made against the ledger, not the response.
#[tokio::test]
async fn engine_mode_a_cleared_case_executes_the_held_movement() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let Some((review, from, to, token, watch)) = park_a_held_transfer(&c).await else {
        return;
    };
    if !resolve_case_for(&c, review, "cleared").await {
        eprintln!("SKIP: no engine/bank DB access to resolve the case");
        return;
    }

    let v = poll_review(&c, &token, review).await;
    assert_eq!(v["status"], "executed", "a cleared case must release: {v}");
    let txn = v["transaction_id"]
        .as_str()
        .unwrap_or_else(|| panic!("executed must carry its transaction: {v}"));

    // The money actually moved. A poll that says "executed" over an unchanged
    // balance is the failure this asserts against.
    assert_eq!(
        balance_of(&c, &token, to).await,
        120.0,
        "the recipient must have been credited by the released transfer"
    );
    assert!(
        balance_of(&c, &token, from).await < 5000.0,
        "the sender must have been debited"
    );
    engine_list_revoke(&c, &watch).await;

    // And it carries the decision that held it, so the audit trail survives the
    // detour through review.
    let svc = service_token(&c).await;
    let link = fraud_link(&c, &svc, Uuid::parse_str(txn).unwrap()).await;
    assert_eq!(link.status().as_u16(), 200);
    let link: Value = link.json().await.unwrap();
    let op_id = link["operation_id"]
        .as_str()
        .unwrap_or_else(|| panic!("a released movement keeps its linkage: {link}"));
    assert_engine_decision_exists(&c, op_id).await;
}

/// Tier 2 — `confirmed_fraud` must NEVER execute. The mirror of the test above,
/// and the one that matters more: releasing on the wrong verdict pays a
/// fraudster.
#[tokio::test]
async fn engine_mode_confirmed_fraud_never_executes_the_held_movement() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let Some((review, from, to, token, watch)) = park_a_held_transfer(&c).await else {
        return;
    };
    if !resolve_case_for(&c, review, "confirmed_fraud").await {
        eprintln!("SKIP: no engine/bank DB access to resolve the case");
        return;
    }

    let v = poll_review(&c, &token, review).await;
    assert_eq!(v["status"], "refused", "confirmed fraud must refuse: {v}");
    assert!(
        v["transaction_id"].is_null(),
        "a refused review must post nothing: {v}"
    );
    assert_eq!(
        balance_of(&c, &token, from).await,
        5000.0,
        "the sender must be untouched"
    );
    assert_eq!(
        balance_of(&c, &token, to).await,
        0.0,
        "the recipient must never have been credited"
    );
    engine_list_revoke(&c, &watch).await;
}

/// Tier 2 — an OPEN case is not a release.
///
/// The engine reports "no case" and "a case nobody has ruled on" as different
/// things (fraud engine #31) precisely so this cannot be collapsed. A bank that
/// treated `open` as clearance would release every held movement the instant it
/// was polled — which is worse than never parking at all, because it would look
/// like review was happening.
#[tokio::test]
async fn engine_mode_an_unreviewed_case_is_not_a_release() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let Some((review, from, to, token, watch)) = park_a_held_transfer(&c).await else {
        return;
    };

    // Poll repeatedly WITHOUT resolving the case. Every one must leave it held.
    for attempt in 0..3 {
        let v = poll_review(&c, &token, review).await;
        assert_eq!(
            v["status"], "held",
            "poll {attempt} released an unreviewed movement: {v}"
        );
    }
    assert_eq!(balance_of(&c, &token, from).await, 5000.0);
    assert_eq!(balance_of(&c, &token, to).await, 0.0);
    engine_list_revoke(&c, &watch).await;
}

/// Tier 2 — another customer's review is a 404, not a 403.
///
/// Same rule the rest of the bank follows: a stranger must not be able to
/// confirm that a review exists, which a 403 would do.
#[tokio::test]
async fn engine_mode_a_review_is_invisible_to_another_customer() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let Some((review, _from, _to, _token, watch)) = park_a_held_transfer(&c).await else {
        return;
    };

    let (_, other_email) = create_customer(&c).await;
    let other = login_with_device(&c, &other_email, &format!("dev-{}", Uuid::new_v4())).await;
    let r = c
        .get(format!("{}/api/v1/reviews/{}", base_url(), review))
        .bearer_auth(&other)
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status().as_u16(),
        404,
        "another customer's review is a 404"
    );
    engine_list_revoke(&c, &watch).await;
}

/// Tier 2 — the second rail parks too.
///
/// e-Transfer is where the corpus's typologies live (ATO drain, mule fan-in,
/// the daily drip), so a hold that only parked internal transfers would miss
/// most of what the engine actually stops. Held on the recipient handle, which
/// is what `payee` resolves to on the external rails.
#[tokio::test]
async fn engine_mode_a_held_etransfer_parks_instead_of_declining() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let (_, email) = create_customer(&c).await;
    let token = login_with_device(&c, &email, &format!("dev-{}", Uuid::new_v4())).await;
    let account = create_account(&c, &token).await;
    if seed_deposit(&c, &token, account, 5000.0).await.is_none() {
        return;
    }

    // Retried for the same reason `park_a_held_transfer` retries: at $120 a
    // screening timeout under concurrent load fails OPEN and the send goes
    // through. Three attempts, then a real failure.
    let mut watch = Value::Null;
    let mut body = Value::Null;
    let mut ok = false;
    let mut last = String::new();
    for _ in 0..3 {
        let handle = format!("held-{}@example.com", Uuid::new_v4());
        watch = engine_list_add(&c, "account_watch", &handle).await;
        let resp = c
            .post(format!("{}/api/v1/interac/etransfers", base_url()))
            .bearer_auth(&token)
            .json(&json!({
                "from_account_id": account,
                "amount": 120.00,
                "recipient_handle_type": "email",
                "recipient_handle_value": handle,
                "security_question": "q",
                "security_answer": "a",
                "idempotency_key": format!("held-{}", Uuid::new_v4()),
            }))
            .send()
            .await
            .unwrap();
        let status = resp.status().as_u16();
        body = resp.json().await.unwrap();
        if status == 202 {
            ok = true;
            break;
        }
        last = format!("{status}: {body}");
        engine_list_revoke(&c, &watch).await;
    }
    assert!(
        ok,
        "a watched recipient never parked the e-Transfer in three attempts; last was {last}"
    );
    assert_eq!(body["rail"], "interac_etransfer", "{body}");
    assert_eq!(body["status"], "held", "{body}");

    // No e-Transfer was created and no money moved — a park is not a send.
    assert_eq!(
        balance_of(&c, &token, account).await,
        5000.0,
        "a parked e-Transfer must not have moved money"
    );
    engine_list_revoke(&c, &watch).await;
}

// The rails that do NOT park (cards, AFT, Lynx, deposit, withdrawal) have no
// test of their own here, deliberately.
//
// A first attempt asserted that a card authorization returns something other
// than 202 and creates no `pending_reviews` row. Both hold trivially: cards
// require a `credit_card` account, so an authorization against the chequing
// account these fixtures create is refused before screening is even reached.
// The test could not fail, whatever the code did.
//
// What actually guards those call sites is their EXISTING tests —
// `fraud_link_resolves_a_captured_card_purchase`, `rail_fraud_link_resolves_a_lynx_wire`,
// `rail_fraud_link_resolves_an_aft_entry` and the interac/AFT linkage tests all
// drive `Screened::into_refusal`'s success path, and all had to keep passing
// unchanged for this change to land.
//
// Known gap, stated rather than faked: `into_refusal`'s *refusal* path — a
// held card collapsing back to `TRANSACTION_UNDER_REVIEW` — is not covered.
// Forcing a hold on the card rail needs a rule that fires without a payee
// subject (cards carry no `to_account_id`), which is a fixture worth building
// when something depends on it.

// ---------------------------------------------------------------------------
// The reclaim/idempotency path (review on #75)
// ---------------------------------------------------------------------------
//
// The lease that makes a crashed release recoverable is also the thing that can
// pay twice, tell a customer "refused" about money that moved, or let one
// customer write another's row. Each of these drives the crash window directly
// rather than hoping to observe it.

/// Force the window a crashed release leaves behind: claimed, nothing recorded,
/// and old enough to reclaim.
async fn strand_release(review: Uuid) {
    let Some(pool) = test_db().await else { return };
    sqlx::query(
        "UPDATE pending_reviews SET status = 'executing', transaction_id = NULL, \
         claimed_at = now() - interval '10 minutes' WHERE review_id = $1",
    )
    .bind(review)
    .execute(&pool)
    .await
    .unwrap();
}

async fn review_row(review: Uuid) -> Option<(String, Option<Uuid>)> {
    let pool = test_db().await?;
    sqlx::query_as("SELECT status, transaction_id FROM pending_reviews WHERE review_id = $1")
        .bind(review)
        .fetch_optional(&pool)
        .await
        .unwrap()
}

/// A handler must not write a row it may not read.
///
/// `reclaim_stranded` runs *before* the customer-scoped load, so without the
/// caller's scope any authenticated customer holding a stranger's review id
/// flips that stranger's stranded claim back to `held`. The read then 404s —
/// which looks safe — but the write already landed.
#[tokio::test]
async fn engine_mode_a_stranger_cannot_reclaim_another_customers_review() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let Some((review, _from, _to, _token, watch)) = park_a_held_transfer(&c).await else {
        return;
    };
    strand_release(review).await;

    let (_, other_email) = create_customer(&c).await;
    let other = login_with_device(&c, &other_email, &format!("dev-{}", Uuid::new_v4())).await;
    let r = c
        .get(format!("{}/api/v1/reviews/{}", base_url(), review))
        .bearer_auth(&other)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 404, "a stranger's review is a 404");

    // The 404 is not the assertion that matters — this is.
    let (status, _) = review_row(review).await.expect("the review still exists");
    assert_eq!(
        status, "executing",
        "a stranger's poll must not have reclaimed the row"
    );
    engine_list_revoke(&c, &watch).await;
}

/// A reclaimed release must adopt the money that already moved, not send it
/// again — including when the caller supplied no idempotency key of their own.
///
/// This is the window: post the money, crash before recording `executed`, let
/// the lease age out, poll again. Both uniqueness guards skip NULL, so before
/// the park derived a key this re-executed and paid a second time.
#[tokio::test]
async fn engine_mode_a_reclaimed_release_does_not_pay_twice() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    // `transfer()` sends no idempotency_key — the keyless case, deliberately.
    let Some((review, from, to, token, watch)) = park_a_held_transfer(&c).await else {
        return;
    };
    if !resolve_case_for(&c, review, "cleared").await {
        eprintln!("SKIP: no engine/bank DB access to resolve the case");
        return;
    }

    let first = poll_review(&c, &token, review).await;
    assert_eq!(
        first["status"], "executed",
        "the first release must post: {first}"
    );
    let credited = balance_of(&c, &token, to).await;
    assert_eq!(credited, 120.0, "recipient credited once");

    // Crash window, then a poll that reclaims and re-releases.
    strand_release(review).await;
    let second = poll_review(&c, &token, review).await;

    assert_eq!(
        balance_of(&c, &token, to).await,
        credited,
        "a reclaimed release paid the recipient a SECOND time: {second}"
    );
    assert_eq!(
        balance_of(&c, &token, from).await,
        5000.0 - 120.0 - 1.50,
        "the sender was debited twice"
    );
    engine_list_revoke(&c, &watch).await;
}

/// A retry of a held request must not reach the engine again.
///
/// The posted-movement replay check cannot see a parked movement — nothing has
/// posted — so without the park short-circuit the retry screens afresh. #28 set
/// the rule that a replay short-circuits *before* the engine is called.
///
/// Asserted on the engine's **replay counter**, not on the decision count. The
/// engine dedupes by idempotency key, so a second screening returns the
/// original decision and writes no second row — a decision-count assertion
/// passes even with the short-circuit removed, which is how the first version
/// of this test survived its probe. What actually leaks is the round trip
/// itself: the assessment runs before the dedupe, so the velocity windows count
/// one customer intent twice. That is precisely why the rails' own replay
/// checks sit ahead of screening.
#[tokio::test]
async fn engine_mode_a_retry_while_held_does_not_mint_a_second_decision() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let (customer_id, email) = create_customer(&c).await;
    let token = login_with_device(&c, &email, &format!("dev-{}", Uuid::new_v4())).await;
    let from = create_account(&c, &token).await;
    let to = create_account(&c, &token).await;
    if seed_deposit(&c, &token, from, 5000.0).await.is_none() {
        return;
    }
    let watch = engine_list_add(&c, "account_watch", &to.to_string()).await;
    let key = format!("retry-{}", Uuid::new_v4());

    let send = |k: String| {
        let c = c.clone();
        let token = token.clone();
        async move {
            c.post(format!("{}/api/v1/transactions/transfer", base_url()))
                .bearer_auth(&token)
                .json(&json!({
                    "from_account_id": from, "to_account_id": to, "amount": 120.0,
                    "description": "retry test", "idempotency_key": k,
                }))
                .send()
                .await
                .unwrap()
        }
    };

    let replays_before = engine_replays(&c).await;

    let first = send(key.clone()).await;
    if first.status().as_u16() != 202 {
        eprintln!("SKIP: screening failed open, so nothing parked");
        engine_list_revoke(&c, &watch).await;
        return;
    }
    let first: Value = first.json().await.unwrap();

    let retry = send(key.clone()).await;
    assert_eq!(retry.status().as_u16(), 202, "a retry must return the park");
    let retry: Value = retry.json().await.unwrap();
    assert_eq!(
        retry["review_id"], first["review_id"],
        "the retry must adopt the same review, not open a second"
    );

    // The number that matters: the engine was never asked a second time.
    assert_eq!(
        engine_replays(&c).await,
        replays_before,
        "the retry reached the engine — a replay was served, so velocity counted \
         this customer intent twice"
    );

    // And still exactly one decision, scoped to `transfer` (the seed deposit is
    // screened too, and counting it made an earlier version read 2 for correct
    // code).
    let Some(engine) = engine_db().await else {
        return;
    };
    let decisions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM decisions WHERE customer_id = $1 AND transaction_type = 'transfer'",
    )
    .bind(customer_id)
    .fetch_one(&engine)
    .await
    .unwrap();
    assert_eq!(decisions, 1, "one intent, one decision");
    engine_list_revoke(&c, &watch).await;
}

/// Releasing a movement still produces its fraud label.
///
/// The bank used to POST a `released_after_review` event, which the engine
/// rejected on every call — `OutcomeEventType` is a closed set — silently,
/// because the send was fire-and-forget. Deleting it loses nothing, and this
/// asserts that: the label comes from the engine's own case resolution.
#[tokio::test]
async fn engine_mode_a_released_movement_still_carries_its_label() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let Some((review, _from, _to, token, watch)) = park_a_held_transfer(&c).await else {
        return;
    };
    if !resolve_case_for(&c, review, "cleared").await {
        eprintln!("SKIP: no engine/bank DB access to resolve the case");
        return;
    }
    let released = poll_review(&c, &token, review).await;
    assert_eq!(released["status"], "executed", "{released}");

    let Some(bank) = test_db().await else { return };
    let (op_id,): (Uuid,) =
        sqlx::query_as("SELECT operation_id FROM pending_reviews WHERE review_id = $1")
            .bind(review)
            .fetch_one(&bank)
            .await
            .unwrap();

    let Some(engine) = engine_db().await else {
        return;
    };
    let labelled: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM outcome_events \
         WHERE operation_id = $1 AND event_type = 'case_cleared'",
    )
    .bind(op_id)
    .fetch_one(&engine)
    .await
    .unwrap();
    assert_eq!(
        labelled, 1,
        "the released movement must carry a case_cleared label from the engine's own resolution"
    );
    engine_list_revoke(&c, &watch).await;
}

/// Idempotent replays the engine has served — the observable that says a caller
/// reached it when it should have short-circuited.
async fn engine_replays(c: &reqwest::Client) -> f64 {
    let text = c
        .get(format!("{}/metrics", engine_url()))
        .bearer_auth(admin_token())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    text.lines()
        .find(|l| l.starts_with("fraud_decision_replays_total "))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

/// Park a held e-Transfer and return `(review_id, account, token, watch_entry)`.
async fn park_a_held_etransfer(c: &reqwest::Client) -> Option<(Uuid, Uuid, String, Value)> {
    let (_, email) = create_customer(c).await;
    let token = login_with_device(c, &email, &format!("dev-{}", Uuid::new_v4())).await;
    let account = create_account(c, &token).await;
    seed_deposit(c, &token, account, 5000.0).await?;

    // Retried because at $120 a screening timeout under concurrent load fails
    // OPEN and the send goes through, which is indistinguishable here from a
    // policy miss. Raising the amount above `fail_closed_above` to force a
    // retryable 503 instead was measured and made things WORSE: each retry adds
    // a customer, a deposit and a send, so the extra traffic timed out other
    // tests' screening. Three attempts at $120 is the cheaper trade.
    for _ in 0..3 {
        let handle = format!("reclaim-{}@example.com", Uuid::new_v4());
        let watch = engine_list_add(c, "account_watch", &handle).await;
        let resp = c
            .post(format!("{}/api/v1/interac/etransfers", base_url()))
            .bearer_auth(&token)
            .json(&json!({
                "from_account_id": account,
                "amount": 120.00,
                "recipient_handle_type": "email",
                "recipient_handle_value": handle,
                "security_question": "q",
                "security_answer": "a",
            }))
            .send()
            .await
            .unwrap();
        if resp.status().as_u16() == 202 {
            let body: Value = resp.json().await.unwrap();
            let review = Uuid::parse_str(body["review_id"].as_str().unwrap()).unwrap();
            return Some((review, account, token, watch));
        }
        engine_list_revoke(c, &watch).await;
    }
    panic!("a watched recipient never parked an e-Transfer in three attempts");
}

/// A reclaimed e-Transfer release must report what actually happened.
///
/// The transfer rail has adopted the winner on a unique violation since #65;
/// interac mapped the same violation to a bare `Conflict`. So a release that
/// crashed after sending, then reclaimed, threw — and the review was recorded
/// `refused` ("cleared, but could not be completed") with no `transaction_id`,
/// while the money had already gone. Telling a customer their transfer was
/// refused after sending it is the worst thing this rail can do.
#[tokio::test]
async fn engine_mode_a_reclaimed_etransfer_release_reports_executed_not_refused() {
    let c = client();
    require_stack!(&c);
    require_fraud_e2e!(&c);
    let Some((review, account, token, watch)) = park_a_held_etransfer(&c).await else {
        return;
    };
    if !resolve_case_for(&c, review, "cleared").await {
        eprintln!("SKIP: no engine/bank DB access to resolve the case");
        return;
    }

    let first = poll_review(&c, &token, review).await;
    assert_eq!(
        first["status"], "executed",
        "the first release must send: {first}"
    );
    let after_send = balance_of(&c, &token, account).await;

    // Crash after sending, before recording it. The reclaim re-runs the send.
    strand_release(review).await;
    let second = poll_review(&c, &token, review).await;

    assert_eq!(
        second["status"], "executed",
        "a reclaimed send of an already-sent e-Transfer must adopt it, not report refused: {second}"
    );
    assert!(
        !second["transaction_id"].is_null(),
        "an adopted release must still name its money row: {second}"
    );
    assert_eq!(
        balance_of(&c, &token, account).await,
        after_send,
        "the reclaimed release sent the money a second time"
    );

    // And exactly one e-Transfer exists for it.
    if let Some(pool) = test_db().await {
        let sent: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM interac_etransfers e JOIN pending_reviews p \
             ON p.review_id = $1 WHERE e.sender_account_id = $2",
        )
        .bind(review)
        .bind(account)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(sent, 1, "exactly one e-Transfer must exist for this review");
    }
    engine_list_revoke(&c, &watch).await;
}
