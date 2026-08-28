//! The agent-facing surface (`/api/v1/agent/*`).
//!
//! Every handler takes [`AuthenticatedAgent`] — a live, re-validated mandate —
//! and **no account parameter**: the mandate pins the account, so an agent
//! token cannot name any other account (no confused-deputy surface). Each
//! operation passes through `policy::authorize_read`, which records the
//! decision (allow or deny) in `agent_actions` before anything is returned.
//!
//! Phase 2 adds `POST /transfers` here (mandatory `idempotency_key`, caps
//! checked under the mandate row lock). Phase 3: an over-cap transfer no
//! longer dead-ends — it parks as a pending approval (202) for the granting
//! customer to approve or decline; `GET /approvals/{id}` polls its fate.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use uuid::Uuid;
use validator::Validate;

use crate::errors::AppError;
use crate::handlers::cards::{normalize_amount, Tx};
use crate::handlers::transactions::{
    execute_transfer, fetch_history, find_by_idempotency_key, load_transaction_response,
    AgentTransferCtx, TransferSpec,
};
use crate::handlers::AppState;
use crate::middleware::auth::AuthenticatedAgent;
use crate::models::account::{Account, AccountBalanceResponse, ActiveHold};
use crate::models::agent::{
    AgentApprovalStatus, AgentTransferRequest, SCOPE_READ_BALANCE, SCOPE_READ_TRANSACTIONS,
};
use crate::models::transaction::{TransactionHistoryQuery, TransactionHistoryResponse};
use crate::policy;

pub fn agent_api_routes() -> Router<AppState> {
    Router::new()
        .route("/account", get(get_mandated_account))
        .route("/transactions", get(get_mandated_transactions))
        .route("/transfers", post(post_mandated_transfer))
        .route("/approvals/:id", get(get_approval_status))
}

/// The agent-visible projection of a pending approval.
const AGENT_APPROVAL_COLUMNS: &str =
    "approval_id, status, reason, amount, to_account_id, expires_at, transaction_id";

/// Balance snapshot of the mandate's account (scope `read:balance`).
/// Same response shape as the customer's `GET /accounts/{id}/balance`.
async fn get_mandated_account(
    State(state): State<AppState>,
    agent: AuthenticatedAgent,
) -> Result<Json<AccountBalanceResponse>, AppError> {
    policy::authorize_read(&state.pool, &agent, SCOPE_READ_BALANCE).await?;

    let account = sqlx::query_as::<_, Account>(
        "SELECT account_id, customer_id, account_number, account_type, currency,
                balance, available_balance, status, interest_rate, overdraft_limit,
                minimum_balance, created_at, updated_at, activated_at, closed_at
         FROM accounts WHERE account_id = $1",
    )
    .bind(agent.account_id)
    .fetch_one(&state.pool)
    .await
    .map_err(AppError::Database)?;

    let holds = sqlx::query_as::<_, ActiveHold>(
        "SELECT hold_id, amount, reason, expires_at
         FROM account_holds
         WHERE account_id = $1 AND released_at IS NULL
         ORDER BY created_at DESC",
    )
    .bind(agent.account_id)
    .fetch_all(&state.pool)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(AccountBalanceResponse {
        account_id: account.account_id,
        account_number: account.account_number,
        balance: account.balance,
        available_balance: account.available_balance,
        currency: account.currency,
        status: account.status,
        holds,
    }))
}

/// History of the mandate's account (scope `read:transactions`). Reuses the
/// customer history machinery with `account_id` pinned to the mandate.
async fn get_mandated_transactions(
    State(state): State<AppState>,
    agent: AuthenticatedAgent,
    Query(mut q): Query<TransactionHistoryQuery>,
) -> Result<Json<TransactionHistoryResponse>, AppError> {
    policy::authorize_read(&state.pool, &agent, SCOPE_READ_TRANSACTIONS).await?;

    // The mandate decides the account — any client-supplied value is ignored.
    q.account_id = Some(agent.account_id);
    let history = fetch_history(&state, agent.customer_id, q).await?;
    Ok(Json(history))
}

/// Agent-initiated transfer out of the mandate's account (Phase 2).
///
/// Scope `transfer:initiate`; `idempotency_key` is REQUIRED (agents retry).
/// The mandate's `max_per_tx` / `daily_cap` / `allowed_payees` are enforced —
/// and the spend *reserved* — under the mandate row lock inside the transfer's
/// own DB transaction (`policy::authorize_and_reserve_transfer`), so a racing
/// duplicate or revocation serializes there. The funding account is implicitly
/// the mandate's; the standard flat fee applies (a bank charge — the caps
/// meter the transfer amount only).
async fn post_mandated_transfer(
    State(state): State<AppState>,
    agent: AuthenticatedAgent,
    Json(req): Json<AgentTransferRequest>,
) -> Result<Response, AppError> {
    req.validate()?;
    let amount = normalize_amount(req.amount)?;

    // Reject a self-transfer BEFORE the replay check, so a malformed request
    // with a previously-used key is a 400, not a misleading 200 replay.
    if req.to_account_id == agent.account_id {
        return Err(AppError::BadRequest(
            "destination must differ from the mandated account".to_string(),
        ));
    }

    // No scope pre-check here: `authorize_and_reserve_transfer` checks scope
    // under the mandate lock, and the deny path below audits it — one audit
    // row per attempt, all under operation "transfer".

    // Idempotent replay: the key's namespace is THIS mandate (via the
    // metadata tag), so it can never surface a transfer the customer or
    // another mandate made — the agent plane stays pinned to its own history.
    // Best-effort like the customer path: sequential replays return the
    // original; a tightly-concurrent duplicate could still double-post.
    if let Some(existing) = find_by_idempotency_key(
        &state.pool,
        &req.idempotency_key,
        agent.customer_id,
        Some(agent.mandate_id),
    )
    .await?
    {
        policy::record_action(
            &state.pool,
            agent.mandate_id,
            agent.agent_id,
            agent.customer_id,
            agent.account_id,
            "transfer",
            Some(amount),
            "allowed",
            Some("IDEMPOTENT_REPLAY"),
            Some(existing),
        )
        .await
        .map_err(AppError::Database)?;
        let resp = load_transaction_response(&state.pool, existing).await?;
        return Ok((StatusCode::OK, Json(resp)).into_response());
    }

    // Step-up retry (Phase 3): the same request may already be parked awaiting
    // the owner's decision — or being executed right now — hand back the same
    // OPEN ask (pending or executing), don't stack another.
    if let Some(open) = sqlx::query_as::<_, AgentApprovalStatus>(&format!(
        "SELECT {AGENT_APPROVAL_COLUMNS} FROM pending_approvals \
         WHERE mandate_id = $1 AND idempotency_key = $2 \
           AND status IN ('pending', 'executing') \
           AND expires_at > CURRENT_TIMESTAMP",
    ))
    .bind(agent.mandate_id)
    .bind(&req.idempotency_key)
    .fetch_optional(&state.pool)
    .await
    .map_err(AppError::Database)?
    {
        return Ok((StatusCode::ACCEPTED, Json(open)).into_response());
    }

    let result = execute_transfer(
        &state,
        agent.customer_id,
        TransferSpec {
            from_account_id: agent.account_id,
            to_account_id: req.to_account_id,
            amount,
            description: &req.description,
            external_reference: None,
            idempotency_key: Some(&req.idempotency_key),
            agent: Some(AgentTransferCtx {
                agent_id: agent.agent_id,
                mandate_id: agent.mandate_id,
                cap_override: false,
            }),
        },
        crate::fraud::gate::Screening {
            channel: "web", // overridden to agentic_branch by the agent ctx
            session_id: None,
            approval_latency_seconds: None,
            screen_scope: None,
        },
    )
    .await
    // The agent plane does not park fraud holds. It already parks — for step-up
    // approval, below — and routing a hold into a SECOND park would hand the
    // agent a review id for an ask its owner never saw. A hold falls through to
    // the catch-all, which audits it and collapses it into the same opaque
    // refusal every other agent failure gets (see `refusal_for_agent`).
    .and_then(crate::handlers::transactions::Executed::posted_or_refuse);

    match result {
        Ok(resp) => Ok((StatusCode::CREATED, Json(resp)).into_response()),
        Err(err) => {
            // The failed attempt's transaction rolled back, so the audit row
            // is written here, outside it. EVERY failure is recorded — policy
            // denials with their reason code, and post-policy execution
            // failures (insufficient funds, inoperable account, a revocation
            // racing the reservation) with the error's code — so the owner's
            // activity view never has blind spots.
            //
            // One transaction, on every failure rather than only the step-up
            // ones. Two autocommit writes in sequence let a park failure leave
            // an `agent_actions` row describing a step-up that no
            // `pending_approvals` row backs (#36 item 1), and since #39 that
            // dangling audit is no longer merely internal: the same CTE mirrors
            // it into `agent_denial_outbox`, so it reaches the fraud engine as
            // an `agent_denial`. A retry then mints a fresh `action_id`, and
            // `event_key` derives from it — so the engine would count two
            // events for one logical attempt, inflating exactly the probing
            // signal that telemetry exists to measure. Committing the audit
            // with the ask removes the window rather than compensating for it.
            let reason = transfer_failure_reason(&err);
            let decision = policy::decision_for(&reason);
            let mut tx = state.pool.begin().await.map_err(AppError::Database)?;
            policy::record_action_tx(
                &mut tx,
                agent.mandate_id,
                agent.agent_id,
                agent.customer_id,
                agent.account_id,
                "transfer",
                Some(amount),
                decision,
                Some(&reason),
                None,
            )
            .await
            .map_err(AppError::Database)?;

            // Phase 3: the two cap overruns don't dead-end — they park as a
            // pending approval for the owner to approve/decline, and the agent
            // gets a 202 instead of a 403.
            if decision == "step_up_required" {
                let approval = park_pending_approval(
                    &mut tx,
                    &agent,
                    &req,
                    amount,
                    &reason,
                    state.settings.agent.approval_ttl_minutes,
                )
                .await?;
                tx.commit().await.map_err(AppError::Database)?;
                return Ok((StatusCode::ACCEPTED, Json(approval)).into_response());
            }
            tx.commit().await.map_err(AppError::Database)?;
            // The audit above kept the true reason; the agent gets one opaque
            // refusal, because a cause-specific one is an oracle (see
            // refusal_for_agent).
            Err(refusal_for_agent(err))
        }
    }
}

/// Collapse a refusal into what an automated client may learn.
///
/// Distinguishable refusals are an oracle: because the mandate's policy is
/// evaluated before account state — and a failed attempt rolls back, consuming no
/// cap — an agent holding only `transfer:initiate` could tell a nonexistent
/// destination from a frozen one from a closed one from a credit card from one
/// with insufficient funds, enumerate which accounts its own `allowed_payees`
/// covers, and bisect its funding account's balance without ever being granted
/// `read:balance`. Card networks collapse risk declines onto one generic code for
/// exactly this reason.
///
/// Three buckets, so the API stays usable:
/// - the agent's own malformed request stays specific (it must be fixable),
/// - transient failures stay distinguishable (or clients retry blindly),
/// - every refusal becomes one opaque code.
///
/// Nothing is lost: `agent_actions` still carries the true reason for the
/// granting customer, who is the party entitled to it.
pub(crate) fn refusal_for_agent(err: AppError) -> AppError {
    match err {
        // The agent's own bug — keep it debuggable.
        AppError::Validation(_) | AppError::BadRequest(_) => err,
        // Transient: the agent should back off and retry, so it must be able to
        // tell these apart from a refusal.
        AppError::ServiceUnavailable(_)
        | AppError::RateLimit(_)
        | AppError::Upstream { .. }
        | AppError::Database(_)
        | AppError::Internal(_) => err,
        // Its own credential died — it cannot act on anything else, and hiding
        // this would just make it retry forever.
        AppError::MandateInactive => err,
        // Everything else refused this transfer, and why is not the agent's
        // business: funds, account existence, account status, account limits,
        // payee allowlist, missing scope, risk.
        _ => AppError::TransferRefused,
    }
}

/// Machine-readable reason code for a failed transfer execution — shared by
/// the agent deny path above and the approve-execution path in
/// `handlers/approvals.rs`, so both audit with the same vocabulary.
pub(crate) fn transfer_failure_reason(err: &AppError) -> String {
    match err {
        AppError::PolicyDenied(reason) => reason.clone(),
        AppError::MandateInactive => "MANDATE_INACTIVE".to_string(),
        AppError::InsufficientFunds => "INSUFFICIENT_FUNDS".to_string(),
        AppError::InvalidAccountStatus => "INVALID_ACCOUNT_STATUS".to_string(),
        AppError::BadRequest(_) => "BAD_REQUEST".to_string(),
        AppError::NotFound(_) => "NOT_FOUND".to_string(),
        AppError::TransactionLimitExceeded => "ACCOUNT_LIMIT_EXCEEDED".to_string(),
        // Fraud-engine refusals. Without these arms they audited as INTERNAL,
        // which told the owner nothing — and, next to the gate's own row, told it
        // twice and inconsistently. The engine's reason codes still never leave
        // the engine; these are the bank's own coarse categories.
        AppError::TransactionDeclined => "RISK_DECLINED".to_string(),
        AppError::TransactionUnderReview(_) => "RISK_REVIEW".to_string(),
        AppError::ServiceUnavailable(_) => "RISK_UNAVAILABLE".to_string(),
        _ => "INTERNAL".to_string(),
    }
}

/// Park an over-cap transfer as a pending approval, inside the caller's
/// transaction so the ask and the audit describing it commit as one unit.
///
/// Race-safe on the partial unique index (one open ask per mandate +
/// idempotency key). `DO UPDATE` rather than `DO NOTHING`, on a no-op
/// assignment, and the difference is the whole point (#36 item 2): `DO NOTHING`
/// does not block on an **uncommitted** conflicting row — it returns nothing
/// immediately, and a follow-up `SELECT` cannot see that row either, so a
/// benign duplicate race surfaced as a 500 that read like a server fault.
/// `DO UPDATE` takes the lock, waits for the racing transaction to resolve, and
/// then returns whichever row won.
///
/// The self-assignment is deliberate: it must touch a column to be an UPDATE,
/// and touching `mandate_id` with its own value changes nothing while leaving
/// every meaningful field — amount, reason, expiry — as the winner wrote them.
/// A duplicate must adopt the existing ask, never quietly rewrite it.
///
/// Because `DO UPDATE` always returns a row, there is no losing branch left to
/// handle: the fallback `SELECT` this used to need is gone.
async fn park_pending_approval(
    tx: &mut Tx<'_>,
    agent: &AuthenticatedAgent,
    req: &AgentTransferRequest,
    amount: rust_decimal::Decimal,
    reason: &str,
    ttl_minutes: i64,
) -> Result<AgentApprovalStatus, AppError> {
    let parked = sqlx::query_as::<_, AgentApprovalStatus>(&format!(
        "INSERT INTO pending_approvals \
         (mandate_id, agent_id, customer_id, account_id, to_account_id, amount, \
          description, idempotency_key, reason, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, \
                 CURRENT_TIMESTAMP + $10 * INTERVAL '1 minute') \
         ON CONFLICT (mandate_id, idempotency_key) \
         WHERE status IN ('pending', 'executing') \
         DO UPDATE SET mandate_id = pending_approvals.mandate_id \
         RETURNING {AGENT_APPROVAL_COLUMNS}",
    ))
    .bind(agent.mandate_id)
    .bind(agent.agent_id)
    .bind(agent.customer_id)
    .bind(agent.account_id)
    .bind(req.to_account_id)
    .bind(amount)
    .bind(&req.description)
    .bind(&req.idempotency_key)
    .bind(reason)
    .bind(ttl_minutes)
    .fetch_one(&mut **tx)
    .await
    .map_err(AppError::Database)?;

    tracing::info!(approval_id = %parked.approval_id, mandate_id = %agent.mandate_id,
        %amount, reason, "⏸ transfer parked for step-up approval");
    Ok(parked)
}

/// Poll the fate of a parked transfer (Phase 3). Pinned to the requesting
/// mandate — another mandate's approval is a 404. Deliberately NOT audited:
/// checking the status of one's own ask reads no account data.
async fn get_approval_status(
    State(state): State<AppState>,
    agent: AuthenticatedAgent,
    Path(approval_id): Path<Uuid>,
) -> Result<Json<AgentApprovalStatus>, AppError> {
    // Lazy reclaim-then-expire — the agent polling is the other liveness path
    // (an abandoned claim must become actionable even if the customer never
    // opens /app). Through the customer plane's helper, not a copy: this used
    // to be two inline UPDATEs here, and the expiry one wrote no audit at all,
    // so an agent that polled first destroyed the ask's ending. Scoped to the
    // polling mandate too — this handler must not write a row it may not read.
    crate::handlers::approvals::reclaim_and_expire(
        &state.pool,
        crate::handlers::approvals::ExpiryScope::Ask {
            customer_id: agent.customer_id,
            mandate_id: agent.mandate_id,
            approval_id,
        },
    )
    .await?;

    let approval = sqlx::query_as::<_, AgentApprovalStatus>(&format!(
        "SELECT {AGENT_APPROVAL_COLUMNS} FROM pending_approvals \
         WHERE approval_id = $1 AND mandate_id = $2",
    ))
    .bind(approval_id)
    .bind(agent.mandate_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound("approval not found".to_string()))?;

    Ok(Json(approval))
}
