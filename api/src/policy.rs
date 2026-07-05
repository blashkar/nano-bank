//! The deterministic policy engine for the agent plane.
//!
//! The agent's own AI reasoning never authorizes anything — it *proposes*
//! actions, and this module *disposes*, from the mandate row alone. One module
//! (not per-endpoint checks) so the decision logic and its audit trail can't
//! drift apart: every decision — allow **and** deny — is recorded append-only
//! in `agent_actions` before the caller proceeds.
//!
//! Decision vocabulary (`allowed` / `denied` / `step_up_required`) is fixed in
//! the schema from day one; Phase 3's step-up flow reuses it unchanged.
//!
//! Phase 2 adds `authorize_and_reserve_transfer(...)` here: a `SELECT … FOR
//! UPDATE` on the mandate row inside the transfer's DB transaction (mandate
//! first, then accounts — the global lock-order rule), re-checking status and
//! caps and bumping `daily_used` race-safely.

use rust_decimal::Decimal;
use uuid::Uuid;

use crate::config::database::DatabasePool;
use crate::errors::AppError;
use crate::middleware::auth::AuthenticatedAgent;

/// Machine-readable denial reasons (surfaced in `POLICY_DENIED` responses and
/// recorded in `agent_actions.reason`).
pub const REASON_SCOPE_MISSING: &str = "SCOPE_MISSING";

/// Append one decision to the `agent_actions` audit. Part of the request path
/// by design: if the audit can't be written, the action doesn't happen.
#[allow(clippy::too_many_arguments)]
pub async fn record_action(
    pool: &DatabasePool,
    mandate_id: Uuid,
    agent_id: Uuid,
    customer_id: Uuid,
    account_id: Uuid,
    operation: &str,
    amount: Option<Decimal>,
    decision: &str,
    reason: Option<&str>,
    transaction_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO agent_actions \
         (mandate_id, agent_id, customer_id, account_id, operation, amount, \
          decision, reason, transaction_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(mandate_id)
    .bind(agent_id)
    .bind(customer_id)
    .bind(account_id)
    .bind(operation)
    .bind(amount)
    .bind(decision)
    .bind(reason)
    .bind(transaction_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Authorize a read operation under the agent's mandate: the operation name is
/// its scope (`read:balance`, `read:transactions`). Records the decision either
/// way, then allows or returns 403 `POLICY_DENIED`.
pub async fn authorize_read(
    pool: &DatabasePool,
    agent: &AuthenticatedAgent,
    operation: &str,
) -> Result<(), AppError> {
    let allowed = agent.scopes.iter().any(|s| s == operation);
    record_action(
        pool,
        agent.mandate_id,
        agent.agent_id,
        agent.customer_id,
        agent.account_id,
        operation,
        None,
        if allowed { "allowed" } else { "denied" },
        (!allowed).then_some(REASON_SCOPE_MISSING),
        None,
    )
    .await
    .map_err(AppError::Database)?;

    if allowed {
        Ok(())
    } else {
        Err(AppError::PolicyDenied(REASON_SCOPE_MISSING.to_string()))
    }
}
