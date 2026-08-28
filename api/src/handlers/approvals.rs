//! Step-up approvals (Phase 3) — the customer's side of a parked transfer.
//!
//! When an agent transfer breaches the mandate's amount caps, it parks in
//! `pending_approvals` (see `handlers/agent_api.rs`) instead of hard-failing.
//! These endpoints are **customer-plane only** (an agent token is rejected by
//! the extractor): the agent can never resolve its own ask. Approve executes
//! the transfer with the caps overridden for that one transfer — every other
//! check (mandate active, scope, payee allowlist, funds, account limits) still
//! runs. Decline kills it. Unresolved asks expire lazily on read/resolve.
//!
//! Status contract: `pending → executing → approved | pending(revert)`, or
//! `pending → declined | expired`. `approved` always carries `transaction_id`
//! (written atomically); `executing` is the short in-flight claim — never swept
//! by expiry, but **reclaimed** back to `pending` once `claimed_at` ages past
//! the lease window (a crash mid-execution can't strand the ask). Re-approve
//! after a reclaim is safe: the approve path first finalizes by idempotency
//! key, so money that already moved is adopted, never re-sent.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::errors::AppError;
use crate::handlers::agent_api::transfer_failure_reason;
use crate::handlers::transactions::{
    execute_transfer, find_by_idempotency_key, load_transaction_response, AgentTransferCtx,
    TransferSpec,
};
use crate::handlers::AppState;
use crate::middleware::auth::AuthenticatedCustomer;
use crate::models::agent::PendingApprovalResponse;
use crate::models::transaction::TransactionResponse;
use crate::policy;

pub fn approval_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_approvals))
        .route("/:id/approve", post(approve_approval))
        .route("/:id/decline", post(decline_approval))
}

/// The customer-facing projection: joined with the agent's display name and
/// the funding account's last-4 so the owner can decide at a glance.
const APPROVAL_COLUMNS: &str = "p.approval_id, p.mandate_id, \
     a.display_name AS agent_display_name, p.account_id, \
     right(ac.account_number, 4) AS account_last4, p.to_account_id, p.amount, \
     p.description, p.reason, p.status, p.transaction_id, p.created_at, \
     p.expires_at, p.resolved_at";

const APPROVAL_FROM: &str = "FROM pending_approvals p \
     JOIN agents a ON a.agent_id = p.agent_id \
     JOIN accounts ac ON ac.account_id = p.account_id";

/// How long an `executing` claim may live before it is presumed dead and
/// reclaimed. 3× the 30s request timeout: an execution still in flight can't
/// outlive its request by that much.
const RECLAIM_AFTER_SECONDS: i32 = 90;

/// Which open asks a lazy sweep may touch.
///
/// Both planes run the *same* two statements and the same audit through
/// [`reclaim_and_expire`] — only the scope differs. Keeping one implementation
/// is the point: the agent plane once had its own inline expiry UPDATE with no
/// audit at all, so which plane happened to touch a row first decided whether
/// its ending was recorded.
#[derive(Clone, Copy)]
pub(crate) enum ExpiryScope {
    /// Customer plane: every open ask this owner has.
    Owner { customer_id: Uuid },
    /// Agent plane: exactly one ask, and only if the polling mandate owns it.
    /// Without the mandate bind this is a cross-mandate write — an agent could
    /// expire an ask it is not even allowed to read.
    Ask {
        customer_id: Uuid,
        mandate_id: Uuid,
        approval_id: Uuid,
    },
}

impl ExpiryScope {
    /// `(customer_id, approval_id, mandate_id)` — the last two are NULL on the
    /// customer plane, where the sweep covers the whole owner's queue.
    fn binds(self) -> (Uuid, Option<Uuid>, Option<Uuid>) {
        match self {
            ExpiryScope::Owner { customer_id } => (customer_id, None, None),
            ExpiryScope::Ask {
                customer_id,
                mandate_id,
                approval_id,
            } => (customer_id, Some(approval_id), Some(mandate_id)),
        }
    }
}

/// The scope predicate both sweep statements share. `customer_id` is always
/// bound — it keeps the customer index usable and makes a cross-owner sweep
/// unrepresentable; the other two only narrow. Same NULL-tolerant idiom as the
/// status filter in [`list_approvals`].
const SWEEP_SCOPE: &str = "customer_id = $1 \
     AND ($2::uuid IS NULL OR approval_id = $2) \
     AND ($3::uuid IS NULL OR mandate_id = $3)";

/// One expired ask's audit ingredients. A named struct rather than a tuple:
/// five consecutive `Uuid`s decoded positionally is a bug waiting to happen.
#[derive(sqlx::FromRow)]
struct ExpiredAsk {
    approval_id: Uuid,
    mandate_id: Uuid,
    agent_id: Uuid,
    customer_id: Uuid,
    account_id: Uuid,
    amount: Decimal,
}

/// Reclaim-then-expire, called before every read/resolve so nobody ever acts
/// on a stale row (no sweeper needed): first revert dead `executing` claims
/// (crashed executor — the lease timed out) back to `pending`, then flip
/// overdue open asks to `expired`. Order matters: a reclaimed row already past
/// its `expires_at` correctly cascades to expired in the second statement.
///
/// **All of it commits as one transaction.** Expiry is a terminal outcome for
/// the agent's ask, so it is audited like the other two — and the expiry
/// predicate is `status = 'pending'`, so a row that reached `expired` without
/// its audit row could never be re-found. Flipping first and auditing after
/// (which is what this used to do, on the pool under autocommit) makes a
/// mid-loop failure permanently unauditable. The reclaim rides along so the
/// cascade above is a structural invariant rather than an incidental one.
pub(crate) async fn reclaim_and_expire(
    pool: &crate::config::database::DatabasePool,
    scope: ExpiryScope,
) -> Result<(), AppError> {
    let (customer_id, approval_id, mandate_id) = scope.binds();
    let mut tx = pool.begin().await.map_err(AppError::Database)?;

    // Deliberately unaudited: a lease timeout restores the prior state, it is
    // not a decision about the agent's ask.
    sqlx::query(&format!(
        "UPDATE pending_approvals \
         SET status = 'pending', claimed_at = NULL \
         WHERE {SWEEP_SCOPE} AND status = 'executing' AND transaction_id IS NULL \
           AND claimed_at <= CURRENT_TIMESTAMP - $4 * INTERVAL '1 second'"
    ))
    .bind(customer_id)
    .bind(approval_id)
    .bind(mandate_id)
    .bind(RECLAIM_AFTER_SECONDS)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    let expired: Vec<ExpiredAsk> = sqlx::query_as(&format!(
        "UPDATE pending_approvals \
         SET status = 'expired', resolved_at = CURRENT_TIMESTAMP \
         WHERE {SWEEP_SCOPE} AND status = 'pending' \
           AND expires_at <= CURRENT_TIMESTAMP \
         RETURNING approval_id, mandate_id, agent_id, customer_id, account_id, amount"
    ))
    .bind(customer_id)
    .bind(approval_id)
    .bind(mandate_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    for ask in &expired {
        policy::record_action_tx(
            &mut tx,
            ask.mandate_id,
            ask.agent_id,
            ask.customer_id,
            ask.account_id,
            "transfer",
            Some(ask.amount),
            "denied",
            Some(policy::REASON_STEP_UP_EXPIRED),
            None,
        )
        .await
        .map_err(AppError::Database)?;
    }
    tx.commit().await.map_err(AppError::Database)?;

    // After the commit: logging an expiry that then rolled back would be a lie.
    for ask in &expired {
        tracing::info!(approval_id = %ask.approval_id, "step-up ask expired unanswered");
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ApprovalListQuery {
    /// Optional filter: `pending` / `approved` / `declined` / `expired`.
    status: Option<String>,
}

/// The caller's step-up approvals, newest first (all statuses unless filtered).
async fn list_approvals(
    State(state): State<AppState>,
    auth: AuthenticatedCustomer,
    Query(q): Query<ApprovalListQuery>,
) -> Result<Json<Vec<PendingApprovalResponse>>, AppError> {
    reclaim_and_expire(
        &state.pool,
        ExpiryScope::Owner {
            customer_id: auth.customer_id,
        },
    )
    .await?;

    let approvals = sqlx::query_as::<_, PendingApprovalResponse>(&format!(
        "SELECT {APPROVAL_COLUMNS} {APPROVAL_FROM} \
         WHERE p.customer_id = $1 AND ($2::text IS NULL OR p.status = $2) \
         ORDER BY p.created_at DESC LIMIT 100"
    ))
    .bind(auth.customer_id)
    .bind(&q.status)
    .fetch_all(&state.pool)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(approvals))
}

/// The claimed row's execution ingredients.
#[derive(sqlx::FromRow)]
struct ClaimedApproval {
    mandate_id: Uuid,
    agent_id: Uuid,
    account_id: Uuid,
    to_account_id: Uuid,
    amount: Decimal,
    description: String,
    idempotency_key: String,
    /// When the approval was parked — the park→approve latency is fraud
    /// context (see the screening call in `approve`).
    created_at: chrono::DateTime<chrono::Utc>,
}

/// The atomic finalization: `approved` is born WITH its transaction_id — one
/// write, guarded on the claim still being ours. If it matches 0 rows the
/// lease was reclaimed (and possibly re-resolved) while we executed — the
/// money response is still returned honestly, but the loser backs off: no
/// audit row (the winning resolution owns the trail) and a loud warning.
async fn finalize_approved(
    state: &AppState,
    approval_id: Uuid,
    claim: &ClaimedApproval,
    customer_id: Uuid,
    resp: &TransactionResponse,
) -> Result<(), AppError> {
    // One transaction: `approved` is terminal, so a flip whose audit then failed
    // would be unrecoverable. Rolling back instead leaves the row `executing`
    // with transaction_id IS NULL — the already-handled stranded state, which
    // the reclaim ages back to `pending` and re-approve adopts by idempotency
    // key. Atomicity turns an unrecoverable outcome into a recoverable one.
    let mut tx = state.pool.begin().await.map_err(AppError::Database)?;
    let updated = sqlx::query(
        "UPDATE pending_approvals \
         SET status = 'approved', transaction_id = $2, \
             resolved_at = CURRENT_TIMESTAMP, claimed_at = NULL \
         WHERE approval_id = $1 AND status = 'executing'",
    )
    .bind(approval_id)
    .bind(resp.transaction_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    if updated.rows_affected() != 1 {
        // Explicit rollback, not drop-rollback: this is a *success* return, and
        // a reader should not have to reason about Drop to see nothing was kept.
        tx.rollback().await.map_err(AppError::Database)?;
        tracing::warn!(approval_id = %approval_id, transaction_id = %resp.transaction_id,
            "step-up finalize lost its claim (reclaimed mid-execution) — money moved, \
             state owned by the other resolution");
        return Ok(());
    }
    policy::record_action_tx(
        &mut tx,
        claim.mandate_id,
        claim.agent_id,
        customer_id,
        claim.account_id,
        "transfer",
        Some(claim.amount),
        "allowed",
        Some(policy::REASON_STEP_UP_APPROVED),
        Some(resp.transaction_id),
    )
    .await
    .map_err(AppError::Database)?;
    tx.commit().await.map_err(AppError::Database)?;
    tracing::info!(approval_id = %approval_id, transaction_id = %resp.transaction_id,
        "✅ step-up approval executed");
    Ok(())
}

/// Approve a parked transfer: claim the row (guarded, race-safe), then execute
/// with the caps overridden — this consent IS the authorization for the
/// overage. The claim state is the transient `executing`, NOT `approved`:
/// `approved` is only ever written together with `transaction_id`, so a
/// polling agent can treat approved as final — there is no observable
/// approved-with-no-transaction window. On an execution failure the claim
/// reverts to `pending` (with the failure audited), so the owner can fund the
/// account and retry, or decline.
async fn approve_approval(
    State(state): State<AppState>,
    auth: AuthenticatedCustomer,
    Path(approval_id): Path<Uuid>,
) -> Result<(StatusCode, Json<TransactionResponse>), AppError> {
    reclaim_and_expire(
        &state.pool,
        ExpiryScope::Owner {
            customer_id: auth.customer_id,
        },
    )
    .await?;

    // Guarded claim: only one approver wins; a lost race / resolved / already
    // in-flight row is a clean 409, someone else's approval is a 404 (no
    // existence leak).
    let claimed = sqlx::query_as::<_, ClaimedApproval>(
        "UPDATE pending_approvals \
         SET status = 'executing', claimed_at = CURRENT_TIMESTAMP \
         WHERE approval_id = $1 AND customer_id = $2 AND status = 'pending' \
         RETURNING mandate_id, agent_id, account_id, to_account_id, amount, \
                   description, idempotency_key, created_at",
    )
    .bind(approval_id)
    .bind(auth.customer_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(AppError::Database)?;

    let Some(claim) = claimed else {
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM pending_approvals \
             WHERE approval_id = $1 AND customer_id = $2",
        )
        .bind(approval_id)
        .bind(auth.customer_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(AppError::Database)?;
        return match status.as_deref() {
            Some("expired") => Err(AppError::Conflict("approval has expired".to_string())),
            Some(s) => Err(AppError::Conflict(format!("approval is already {s}"))),
            None => Err(AppError::NotFound("approval not found".to_string())),
        };
    };

    // At-least-once safety (the reclaim makes re-approve possible): if a
    // previous stranded attempt already moved the money — executed, then
    // crashed before the approved-write — ADOPT that transaction instead of
    // paying again. The key is namespaced to this mandate, so it can only ever
    // surface this approval's own transfer. (Adoption re-screens nothing: the
    // money already moved under a screened execution.)
    if let Some(existing) = find_by_idempotency_key(
        &state.pool,
        &claim.idempotency_key,
        auth.customer_id,
        Some(claim.mandate_id),
    )
    .await?
    {
        let resp = load_transaction_response(&state.pool, existing).await?;
        finalize_approved(&state, approval_id, &claim, auth.customer_id, &resp).await?;
        tracing::info!(approval_id = %approval_id, transaction_id = %existing,
            "♻️ step-up approval finalized from a prior stranded execution");
        return Ok((StatusCode::OK, Json(resp)));
    }

    // Step-up context for fraud screening: how long the customer deliberated
    // before approving the over-cap ask (near-instant approvals are their own
    // risk signal, engine-side `rapid_approval` rule).
    let approval_latency_seconds = (chrono::Utc::now() - claim.created_at)
        .num_milliseconds()
        .max(0) as f64
        / 1000.0;

    let result = execute_transfer(
        &state,
        auth.customer_id,
        TransferSpec {
            from_account_id: claim.account_id,
            to_account_id: claim.to_account_id,
            amount: claim.amount,
            description: &claim.description,
            external_reference: None,
            idempotency_key: Some(&claim.idempotency_key),
            agent: Some(AgentTransferCtx {
                agent_id: claim.agent_id,
                mandate_id: claim.mandate_id,
                cap_override: true,
            }),
        },
        crate::fraud::gate::Screening {
            channel: "web", // overridden to agentic_branch by the agent ctx
            session_id: auth.session_id,
            approval_latency_seconds: Some(approval_latency_seconds),
            // The agent's original ask was already screened under this same
            // caller key; the approved execution is a DIFFERENT decision
            // (cap_override + latency context) and must not replay it.
            screen_scope: Some("stepup"),
        },
    )
    .await
    // The step-up plane cannot park a fraud hold: this ask is ALREADY parked,
    // in `pending_approvals`, awaiting this very customer. A second park would
    // give one movement two homes, two expiries and two release paths. A hold
    // here refuses exactly as it did before parking existed, and the revert
    // below keeps the ask actionable.
    .and_then(crate::handlers::transactions::Executed::posted_or_refuse);

    match result {
        Ok(resp) => {
            finalize_approved(&state, approval_id, &claim, auth.customer_id, &resp).await?;
            Ok((StatusCode::CREATED, Json(resp)))
        }
        Err(err) => {
            // Computed before begin() — keep the transaction to its two writes.
            let reason = transfer_failure_reason(&err);
            // Revert the claim so the ask stays actionable (expiry still
            // applies), with the denial audit in the same commit.
            let mut tx = state.pool.begin().await.map_err(AppError::Database)?;
            sqlx::query(
                "UPDATE pending_approvals \
                 SET status = 'pending', claimed_at = NULL \
                 WHERE approval_id = $1 AND status = 'executing' AND transaction_id IS NULL",
            )
            .bind(approval_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::Database)?;
            policy::record_action_tx(
                &mut tx,
                claim.mandate_id,
                claim.agent_id,
                auth.customer_id,
                claim.account_id,
                "transfer",
                Some(claim.amount),
                policy::decision_for(&reason),
                Some(&reason),
                None,
            )
            .await
            .map_err(AppError::Database)?;
            tx.commit().await.map_err(AppError::Database)?;
            // A dead mandate is a 401 on the AGENT plane; here the customer's
            // credential is fine — the conflict is with the approval's state.
            Err(match err {
                AppError::MandateInactive => {
                    AppError::Conflict("the mandate is no longer active".to_string())
                }
                other => other,
            })
        }
    }
}

/// Decline a parked transfer. Guarded flip, audited as a denial.
async fn decline_approval(
    State(state): State<AppState>,
    auth: AuthenticatedCustomer,
    Path(approval_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    reclaim_and_expire(
        &state.pool,
        ExpiryScope::Owner {
            customer_id: auth.customer_id,
        },
    )
    .await?;

    // The flip and its denial audit commit together — `declined` is terminal and
    // guarded on `status = 'pending'`, so an unaudited one could never be redone.
    let mut tx = state.pool.begin().await.map_err(AppError::Database)?;
    let declined = sqlx::query_as::<_, ClaimedApproval>(
        "UPDATE pending_approvals \
         SET status = 'declined', resolved_at = CURRENT_TIMESTAMP \
         WHERE approval_id = $1 AND customer_id = $2 AND status = 'pending' \
         RETURNING mandate_id, agent_id, account_id, to_account_id, amount, \
                   description, idempotency_key, created_at",
    )
    .bind(approval_id)
    .bind(auth.customer_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    let Some(claim) = declined else {
        // On the open transaction, not the pool: acquiring a second connection
        // while holding one is a pool-exhaustion deadlock under load. Nothing
        // was written, so the error return's drop-rollback is the right exit.
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM pending_approvals \
             WHERE approval_id = $1 AND customer_id = $2",
        )
        .bind(approval_id)
        .bind(auth.customer_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::Database)?;
        return match status.as_deref() {
            Some("expired") => Err(AppError::Conflict("approval has expired".to_string())),
            Some(s) => Err(AppError::Conflict(format!("approval is already {s}"))),
            None => Err(AppError::NotFound("approval not found".to_string())),
        };
    };

    policy::record_action_tx(
        &mut tx,
        claim.mandate_id,
        claim.agent_id,
        auth.customer_id,
        claim.account_id,
        "transfer",
        Some(claim.amount),
        "denied",
        Some(policy::REASON_STEP_UP_DECLINED),
        None,
    )
    .await
    .map_err(AppError::Database)?;
    tx.commit().await.map_err(AppError::Database)?;

    tracing::info!(approval_id = %approval_id, "🚫 step-up approval declined");
    Ok(StatusCode::NO_CONTENT)
}
