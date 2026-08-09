//! Drains the agent-denial outbox to the fraud engine.
//!
//! Every `agent_actions` row that is not `allowed` is mirrored into
//! `agent_denial_outbox` by the same statement that writes the audit (the CTE in
//! `policy.rs`), so the telemetry and the record it describes commit together.
//! This module is the other end: it claims undelivered rows and POSTs them to
//! the engine's `/v1/outcomes`.
//!
//! Why the bank pushes rather than the engine pulling: the engine has no access
//! to this database, and never will — the integration is HTTP-only by design.
//!
//! The API runs zero background workers by design, so the drain is an admin
//! endpoint poked on a schedule (see `k8s/fraud-denial-drainer-cronjob.yaml`),
//! the same shape as the Interac notification drainer — and now literally the
//! same claim, which both take from [`crate::outbox::OutboxClaim`].

use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post},
    Router,
};
use uuid::Uuid;

use crate::config::database::DatabasePool;
use crate::errors::AppError;
use crate::handlers::AppState;
use crate::middleware::auth::AuthenticatedService;
use crate::outbox::OutboxClaim;

/// Attempts before a denial is dead-lettered: left undelivered with its
/// `last_delivery_error`, and no longer claimed.
const MAX_DELIVERY_ATTEMPTS: i32 = 5;
/// Rows claimed per flush — bounds one admin call's work.
const FLUSH_BATCH: i64 = 100;
/// Delivered rows are kept this long for debugging, then purged.
const DELIVERED_RETENTION_DAYS: i32 = 7;
/// Undelivered rows are kept longer, counted from creation and **regardless of
/// attempt count**. Dead-lettered rows are evidence that delivery is broken and
/// deleting them quickly would hide the outage; rows that were never attempted
/// at all (the `backend = "off"` default) are the same problem seen from the
/// other side. One window covers both, and covers the rows in between — a
/// partly-attempted row this old means nothing is draining either.
///
/// It is longer than the delivered window on purpose: enabling the backend
/// after a break should find a recent backlog to flush, not a hole.
const UNDELIVERED_RETENTION_DAYS: i32 = 30;

pub fn fraud_admin_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/flush-denials", post(flush_denials))
        .route(
            "/admin/transactions/:transaction_id/fraud-link",
            get(fraud_link),
        )
        // Rails whose caller never learns a transaction_id. The money row
        // exists and carries the linkage; only its id is private to the bank.
        .route(
            "/admin/rails/interac/:etransfer_id/fraud-link",
            get(interac_fraud_link),
        )
        .route(
            "/admin/rails/lynx/:wire_id/fraud-link",
            get(lynx_fraud_link),
        )
}

/// The engine's identifiers for one money row, where the bank persisted them.
///
/// **A null is not evidence that no decision exists** — see [`fraud_link`] for
/// the two different states it collapses.
#[derive(serde::Serialize)]
struct FraudLinkResponse {
    transaction_id: Uuid,
    operation_id: Option<Uuid>,
    decision_id: Option<Uuid>,
    /// Screening failed open: the engine was unreachable and the movement
    /// proceeded anyway. The decision may not exist engine-side, so a caller
    /// joining on `operation_id` should expect a miss rather than treat one as
    /// an error.
    failed_open: bool,
}

/// Look up the fraud engine's `operation_id` for a bank transaction.
///
/// **Why this exists.** The engine joins ground truth to decisions on
/// `outcome_events.operation_id = decisions.operation_id`; its `decisions` table
/// has no `transaction_id` column, because the bank mints that inside its own
/// transaction *after* the fraud check. So `operation_id` is the only key that
/// can attach an outcome to a decision — and until now it never left the bank.
/// It was written to `transactions.metadata.fraud` and read by nobody, so no
/// decision could be labelled and the engine's training-set export returned zero
/// rows however much traffic ran (#46).
///
/// **Why an endpoint rather than a response field.** `operation_id` and
/// `decision_id` are fraud-engine internals. Putting them on
/// `TransactionResponse` would publish them to the customer plane — the
/// disclosure concern #34's review raised about echoing bank internals. A
/// service-token route keeps the whole concern on the service plane behind one
/// auth check, instead of a field whose presence depends on who is asking.
///
/// **Not customer-scoped**, deliberately: the caller is the fraud operator, not
/// an account holder.
///
/// # What a null means — three states, and this response distinguishes two
///
/// | State | Response | Reachable? |
/// |---|---|---|
/// | Screened, link persisted | ids present | yes |
/// | Never screened (`backend = "off"`) | nulls | nothing to reach |
/// | **Screened, link not persisted** | **nulls** | **no — but a decision exists** |
///
/// **The third row is closed as of #53/#56/#57.** Interac, Lynx, AFT and cards
/// now stamp `metadata.fraud` on the row their hold creates, so a screened
/// movement on any rail reports its linkage here. What used to be the trap —
/// screened traffic reading as never-screened, and the label pipeline silently
/// under-counting — no longer applies to any rail the bank has.
///
/// **A null now means one thing: nothing screened this row.** Either the
/// backend was off, or the row was written by a path that does not screen at
/// all (a settlement leg, a GL adjustment). It is still not a promise that no
/// decision exists anywhere — a movement the engine *refused* has no
/// `transactions` row at all, so it cannot be reached from here by
/// construction. That case is addressed engine-side instead, by idempotency
/// key (see #58).
///
/// The remaining reachability gap is which id the caller holds, not whether the
/// linkage was recorded — see `rail_transaction_id` and #62.
async fn fraud_link(
    State(state): State<AppState>,
    Path(transaction_id): Path<Uuid>,
    _svc: AuthenticatedService,
) -> Result<Json<FraudLinkResponse>, AppError> {
    Ok(Json(link_for(&state, transaction_id).await?))
}

/// The linkage on one money row. Shared by every entry point so a rail answer
/// can never drift from the transaction answer for the same row.
async fn link_for(state: &AppState, transaction_id: Uuid) -> Result<FraudLinkResponse, AppError> {
    // `metadata` is nullable, so the outer Option is "no such transaction" and
    // the inner one is "transaction exists, metadata NULL". Only the former is a
    // 404.
    let metadata: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT metadata FROM transactions WHERE transaction_id = $1")
            .bind(transaction_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("transaction not found".to_string()))?;

    // No `fraud` metadata is a real answer, so it is a 200 with nulls rather
    // than a 404 — a 404 would invite the caller to retry something that is
    // never going to appear. But see the doc comment: nulls collapse two
    // different states, and only one of them means "no decision".
    let fraud = metadata.as_ref().and_then(|m| m.get("fraud"));
    let uuid_at = |key: &str| -> Option<Uuid> {
        fraud
            .and_then(|f| f.get(key))
            .and_then(serde_json::Value::as_str)
            .and_then(|s| s.parse().ok())
    };

    Ok(FraudLinkResponse {
        transaction_id,
        operation_id: uuid_at("operation_id"),
        decision_id: uuid_at("decision_id"),
        failed_open: fraud
            .and_then(|f| f.get("failed_open"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    })
}

/// Resolve a rail's own id to the `transactions` row its hold created.
///
/// Every rail screens before it moves money and, since #53/#56/#57, stamps the
/// linkage onto that row. But the id a caller is handed back is the rail's
/// (`etransfer_id`, `wire_id`) — the `transactions` id stays inside the bank. So
/// the linkage was recorded and unreachable at the same time, which is #62.
///
/// A rail row with a NULL transaction column is a 404 rather than a nulled 200:
/// unlike a transaction with no `fraud` metadata, this means the movement never
/// got as far as a money row, so there is nothing for a caller to wait for.
async fn rail_transaction_id(
    state: &AppState,
    sql: &'static str,
    rail_id: Uuid,
    what: &str,
) -> Result<Uuid, AppError> {
    sqlx::query_scalar::<_, Option<Uuid>>(sql)
        .bind(rail_id)
        .fetch_optional(&state.pool)
        .await?
        .flatten()
        .ok_or_else(|| AppError::NotFound(format!("{what} has no money row")))
}

async fn interac_fraud_link(
    State(state): State<AppState>,
    Path(etransfer_id): Path<Uuid>,
    _svc: AuthenticatedService,
) -> Result<Json<FraudLinkResponse>, AppError> {
    let txn = rail_transaction_id(
        &state,
        "SELECT hold_transaction_id FROM interac_etransfers WHERE etransfer_id = $1",
        etransfer_id,
        "e-transfer",
    )
    .await?;
    Ok(Json(link_for(&state, txn).await?))
}

async fn lynx_fraud_link(
    State(state): State<AppState>,
    Path(wire_id): Path<Uuid>,
    _svc: AuthenticatedService,
) -> Result<Json<FraudLinkResponse>, AppError> {
    // `settlement_transaction_id` despite the name: lynx.rs binds the HOLD's
    // transaction_id into it at send time, which is the row `screen()` gated
    // and `tag_fraud` stamped.
    let txn = rail_transaction_id(
        &state,
        "SELECT settlement_transaction_id FROM lynx_wires WHERE wire_id = $1",
        wire_id,
        "wire",
    )
    .await?;
    Ok(Json(link_for(&state, txn).await?))
}

#[derive(sqlx::FromRow)]
struct ClaimedDenial {
    outbox_id: Uuid,
    payload: serde_json::Value,
}

/// Drain the agent-denial outbox (admin plane, service token).
///
/// The claim is an atomic `delivery_attempts += 1` under `FOR UPDATE SKIP
/// LOCKED`, so concurrent drainers or multiple API replicas never grab the same
/// row, and a claim that dies mid-send costs one attempt rather than stranding
/// an in-flight state.
///
/// **Delivery is at-least-once**, and that is safe here precisely because the
/// payload carries `event_key` derived from `action_id`: the engine's outcome
/// ingestion is idempotent on it, so a redelivery collapses into the original
/// event instead of double-counting a denial.
async fn flush_denials(
    State(state): State<AppState>,
    _svc: AuthenticatedService,
) -> Result<Json<serde_json::Value>, AppError> {
    // Retention runs first, and unconditionally. The table grows fastest in
    // exactly the configuration that never reaches the delivery loop below —
    // `backend = "off"` is the default, and every denial still lands in the
    // outbox — so a purge that only runs when draining is enabled is a purge
    // that never runs on the deployments that need it.
    let purged = purge_expired(&state.pool).await?;

    // With screening off there is no engine to talk to. Skip without claiming:
    // claiming would burn the retry budget of every row against a backend
    // nobody asked us to call, dead-lettering the lot before it is ever enabled.
    if state.fraud.backend() == "off" {
        let pending: i64 =
            sqlx::query_scalar("SELECT count(*) FROM agent_denial_outbox WHERE delivered = FALSE")
                .fetch_one(&state.pool)
                .await?;
        return Ok(Json(serde_json::json!({
            "skipped": pending,
            "purged": purged,
            "reason": "fraud backend off",
        })));
    }

    let claimed = sqlx::query_as::<_, ClaimedDenial>(
        &OutboxClaim {
            table: "agent_denial_outbox",
            id_column: "outbox_id",
            returning: "outbox_id, payload",
        }
        .sql(),
    )
    .bind(MAX_DELIVERY_ATTEMPTS)
    .bind(FLUSH_BATCH)
    .fetch_all(&state.pool)
    .await?;

    let claimed_count = claimed.len() as i64;
    let mut delivered = 0i64;
    let mut failed = 0i64;

    for row in claimed {
        match state.fraud.report_denial(&row.payload).await {
            Ok(()) => {
                sqlx::query(
                    "UPDATE agent_denial_outbox \
                     SET delivered = TRUE, delivered_at = CURRENT_TIMESTAMP, \
                         last_delivery_error = NULL \
                     WHERE outbox_id = $1",
                )
                .bind(row.outbox_id)
                .execute(&state.pool)
                .await?;
                delivered += 1;
            }
            Err(err) => {
                // Leave delivered = FALSE (the attempt is already counted): it
                // retries next flush until the budget is spent, then dead-letters.
                sqlx::query(
                    "UPDATE agent_denial_outbox SET last_delivery_error = $2 \
                     WHERE outbox_id = $1",
                )
                .bind(row.outbox_id)
                .bind(err.to_string())
                .execute(&state.pool)
                .await?;
                failed += 1;
            }
        }
    }

    Ok(Json(serde_json::json!({
        "claimed": claimed_count,
        "delivered": delivered,
        "failed": failed,
        "purged": purged,
    })))
}

/// Drop outbox rows past their retention window. The Interac outbox has no
/// purge and grows forever; this one must have it.
///
/// Two predicates, split on the only thing that changes the window: whether the
/// row ever reached the engine. Delivered rows are debugging residue and go
/// early; undelivered ones are kept the full window from creation whatever
/// their attempt count, because "never attempted", "mid-retry" and
/// "dead-lettered" are all the same condition — nothing is draining — and
/// deserve the same grace period.
async fn purge_expired(pool: &DatabasePool) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "DELETE FROM agent_denial_outbox \
         WHERE (delivered = TRUE \
                AND delivered_at < CURRENT_TIMESTAMP - ($1 || ' days')::interval) \
            OR (delivered = FALSE \
                AND created_at < CURRENT_TIMESTAMP - ($2 || ' days')::interval)",
    )
    .bind(DELIVERED_RETENTION_DAYS.to_string())
    .bind(UNDELIVERED_RETENTION_DAYS.to_string())
    .execute(pool)
    .await?
    .rows_affected())
}
