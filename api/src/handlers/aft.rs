//! AFT/EFT batch-rail product lifecycle. Money movement goes through the Rail
//! port (`rails::aft::AftRail`); this module owns batch accrual, the CPA-005
//! file emit/ingest, the settlement-window sweep, PAD mandates, and returns.

use std::collections::HashMap;

use axum::Json as AxumJson;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post},
    Router,
};
use rust_decimal::Decimal;
use uuid::Uuid;
use validator::Validate;

use crate::aft::cpa005;
use crate::errors::AppError;
use crate::handlers::cards::{fetch_account_for_update, normalize_amount, post_gl_entry, post_two_legged, reference_number};
use crate::handlers::AppState;
use crate::ledger::Account as GlAccount;
use crate::middleware::auth::{AuthenticatedCustomer, AuthenticatedService};
use crate::models::aft::{
    BatchResponse, CreateCreditRequest, CreateDebitRequest, CreateMandateRequest, EntryResponse,
    MandateResponse,
};
use crate::rails::aft::{ensure_aft_accounts, AftRail};
use crate::rails::{Destination, Rail};

pub fn aft_routes() -> Router<AppState> {
    Router::new()
        // customer plane
        .route("/mandates", post(create_mandate).get(list_mandates))
        .route("/mandates/:id", delete(revoke_mandate))
        .route("/credits", post(create_credit))
        .route("/debits", post(create_debit))
        .route("/batches", get(list_batches))
        .route("/batches/:id/submit", post(submit_batch))
        .route("/entries", get(list_entries))
        // network plane (service token) — the ACSS simulator
        .route("/network/settle/:batch", post(network_settle))
        .route("/network/inbound-batch", post(network_inbound_batch))
        .route("/network/returns", post(network_returns))
}

/// Resolve AFT's clearing/settlement accounts (re-resolved per request) and
/// build the rail.
async fn resolve_aft(state: &AppState) -> Result<AftRail, AppError> {
    let accts = ensure_aft_accounts(&state.pool).await?;
    Ok(AftRail::new(accts))
}

/// Directory the emitted CPA-005 files are written to.
fn aft_file_dir() -> String {
    std::env::var("NANO_BANK__AFT__FILE_DIR").unwrap_or_else(|_| "/tmp/nano-bank-aft".to_string())
}

// --- available_balance helpers (customer accounts only; NEVER the system
// clearing/settlement accounts) — copied from handlers/interac.rs. ---

async fn zero_available(tx: &mut crate::rails::PgTx<'_>, account_id: Uuid) -> Result<(), AppError> {
    sqlx::query("UPDATE accounts SET available_balance = 0 WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Recompute a deposit account's available balance: `balance + overdraft − open holds`.
async fn recompute_available(
    tx: &mut crate::rails::PgTx<'_>,
    account_id: Uuid,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE accounts SET available_balance = balance + overdraft_limit \
         - COALESCE((SELECT sum(amount) FROM account_holds \
                     WHERE account_id=$1 AND released_at IS NULL), 0), \
         updated_at = CURRENT_TIMESTAMP WHERE account_id = $1",
    )
    .bind(account_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// --- handlers (stubs replaced task-by-task) ---

async fn create_mandate() -> Result<StatusCode, AppError> {
    Err(AppError::Internal("todo".into()))
}
async fn list_mandates() -> Result<StatusCode, AppError> {
    Err(AppError::Internal("todo".into()))
}
async fn revoke_mandate() -> Result<StatusCode, AppError> {
    Err(AppError::Internal("todo".into()))
}
async fn create_credit() -> Result<StatusCode, AppError> {
    Err(AppError::Internal("todo".into()))
}
async fn create_debit() -> Result<StatusCode, AppError> {
    Err(AppError::Internal("todo".into()))
}
async fn list_batches() -> Result<StatusCode, AppError> {
    Err(AppError::Internal("todo".into()))
}
async fn submit_batch() -> Result<StatusCode, AppError> {
    Err(AppError::Internal("todo".into()))
}
async fn list_entries() -> Result<StatusCode, AppError> {
    Err(AppError::Internal("todo".into()))
}
async fn network_settle() -> Result<StatusCode, AppError> {
    Err(AppError::Internal("todo".into()))
}
async fn network_inbound_batch() -> Result<StatusCode, AppError> {
    Err(AppError::Internal("todo".into()))
}
async fn network_returns() -> Result<StatusCode, AppError> {
    Err(AppError::Internal("todo".into()))
}
