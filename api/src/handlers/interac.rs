//! Interac e-Transfer product lifecycle. Money movement goes through the Rail
//! port (`rails::interac::InteracRail`); this module owns handle resolution,
//! the claim/decline/cancel/expiry state machine, and notifications.

use axum::extract::rejection::JsonRejection;
use axum::Json as AxumJson;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post},
    Router,
};
use uuid::Uuid;
use validator::Validate;

use crate::errors::AppError;
use crate::handlers::AppState;
use crate::middleware::auth::{AuthenticatedCustomer, AuthenticatedService};
use crate::models::interac::{HandleResponse, RegisterAutodepositRequest};
use crate::rails::interac::{ensure_interac_accounts, normalize_handle, InteracRail};

pub fn interac_routes() -> Router<AppState> {
    Router::new()
        // customer plane
        .route("/etransfers", post(send_etransfer).get(list_etransfers))
        .route("/etransfers/:id", get(get_etransfer))
        .route("/etransfers/:id/claim", post(claim_etransfer))
        .route("/etransfers/:id/decline", post(decline_etransfer))
        .route("/etransfers/:id/cancel", post(cancel_etransfer))
        .route("/autodeposit", post(register_autodeposit).get(list_autodeposit))
        .route("/autodeposit/:id", delete(deregister_autodeposit))
        // network plane (service token)
        .route("/network/inbound", post(network_inbound))
        .route("/network/etransfers/:id/settle", post(network_settle))
        // admin plane (service token)
        .route("/admin/sweep-expired", post(sweep_expired))
}

/// Resolve Interac's clearing/settlement accounts (re-resolved per request) and
/// build the rail.
async fn resolve_interac(state: &AppState) -> Result<InteracRail, AppError> {
    let accts = ensure_interac_accounts(&state.pool).await?;
    Ok(InteracRail::new(accts))
}

/// Interac's default hold lifetime before auto-expiry (real Interac: 30 days).
fn expiry_days() -> i64 {
    std::env::var("NANO_BANK__INTERAC__EXPIRY_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30)
}

/// Max amount per e-Transfer (funds check aside). Default $3,000 like real Interac.
fn max_amount() -> rust_decimal::Decimal {
    std::env::var("NANO_BANK__INTERAC__MAX_ETRANSFER_AMOUNT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| rust_decimal::Decimal::new(3000, 0))
}

// -- Handler stubs (replaced wholesale in Tasks 7-14) ------------------------

async fn send_etransfer() -> Result<StatusCode, AppError> { Err(AppError::Internal("todo".into())) }
async fn list_etransfers() -> Result<StatusCode, AppError> { Err(AppError::Internal("todo".into())) }
async fn get_etransfer() -> Result<StatusCode, AppError> { Err(AppError::Internal("todo".into())) }
async fn claim_etransfer() -> Result<StatusCode, AppError> { Err(AppError::Internal("todo".into())) }
async fn decline_etransfer() -> Result<StatusCode, AppError> { Err(AppError::Internal("todo".into())) }
async fn cancel_etransfer() -> Result<StatusCode, AppError> { Err(AppError::Internal("todo".into())) }
async fn register_autodeposit(
    State(state): State<AppState>,
    caller: AuthenticatedCustomer,
    AxumJson(req): AxumJson<RegisterAutodepositRequest>,
) -> Result<(StatusCode, Json<HandleResponse>), AppError> {
    req.validate()?;
    let handle = normalize_handle(req.handle_type, &req.handle_value);

    // The deposit account must belong to the caller.
    let owns: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE account_id=$1 AND customer_id=$2)",
    )
    .bind(req.deposit_account_id)
    .bind(caller.customer_id)
    .fetch_one(&state.pool)
    .await?;
    if !owns {
        return Err(AppError::NotFound("deposit account not found".into()));
    }

    let row = sqlx::query_as::<_, (Uuid, Option<Uuid>, bool)>(
        r#"
        INSERT INTO interac_handles (customer_id, handle_type, handle_value, autodeposit_account_id)
        VALUES ($1, $2, $3, $4)
        RETURNING handle_id, autodeposit_account_id, active
        "#,
    )
    .bind(caller.customer_id)
    .bind(req.handle_type)
    .bind(&handle)
    .bind(req.deposit_account_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => {
            AppError::Conflict("handle already registered".into())
        }
        _ => AppError::from(e),
    })?;

    Ok((
        StatusCode::CREATED,
        Json(HandleResponse {
            handle_id: row.0,
            handle_type: req.handle_type,
            handle_value: handle,
            autodeposit_account_id: row.1,
            active: row.2,
        }),
    ))
}

async fn list_autodeposit(
    State(state): State<AppState>,
    caller: AuthenticatedCustomer,
) -> Result<Json<Vec<HandleResponse>>, AppError> {
    let rows = sqlx::query_as::<_, (Uuid, crate::models::interac::HandleType, String, Option<Uuid>, bool)>(
        "SELECT handle_id, handle_type, handle_value, autodeposit_account_id, active \
         FROM interac_handles WHERE customer_id=$1 ORDER BY created_at",
    )
    .bind(caller.customer_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, ht, hv, ad, active)| HandleResponse {
                handle_id: id, handle_type: ht, handle_value: hv,
                autodeposit_account_id: ad, active,
            })
            .collect(),
    ))
}

async fn deregister_autodeposit(
    State(state): State<AppState>,
    caller: AuthenticatedCustomer,
    Path(handle_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let n = sqlx::query("DELETE FROM interac_handles WHERE handle_id=$1 AND customer_id=$2")
        .bind(handle_id)
        .bind(caller.customer_id)
        .execute(&state.pool)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::NotFound("handle not found".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}
async fn network_inbound() -> Result<StatusCode, AppError> { Err(AppError::Internal("todo".into())) }
async fn network_settle() -> Result<StatusCode, AppError> { Err(AppError::Internal("todo".into())) }
async fn sweep_expired() -> Result<StatusCode, AppError> { Err(AppError::Internal("todo".into())) }
