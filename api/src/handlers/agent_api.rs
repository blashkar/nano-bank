//! The agent-facing surface (`/api/v1/agent/*`).
//!
//! Every handler takes [`AuthenticatedAgent`] — a live, re-validated mandate —
//! and **no account parameter**: the mandate pins the account, so an agent
//! token cannot name any other account (no confused-deputy surface). Each
//! operation passes through `policy::authorize_read`, which records the
//! decision (allow or deny) in `agent_actions` before anything is returned.
//!
//! Phase 2 adds `POST /transfers` here (mandatory `idempotency_key`, caps
//! checked under the mandate row lock).

use axum::{
    extract::{Query, State},
    response::Json,
    routing::get,
    Router,
};

use crate::errors::AppError;
use crate::handlers::transactions::fetch_history;
use crate::handlers::AppState;
use crate::middleware::auth::AuthenticatedAgent;
use crate::models::account::{Account, AccountBalanceResponse, ActiveHold};
use crate::models::agent::{SCOPE_READ_BALANCE, SCOPE_READ_TRANSACTIONS};
use crate::models::transaction::{TransactionHistoryQuery, TransactionHistoryResponse};
use crate::policy;

pub fn agent_api_routes() -> Router<AppState> {
    Router::new()
        .route("/account", get(get_mandated_account))
        .route("/transactions", get(get_mandated_transactions))
}

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
