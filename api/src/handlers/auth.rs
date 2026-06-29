use axum::{extract::State, http::StatusCode, response::Json, routing::post, Router};
use uuid::Uuid;
use validator::Validate;

use crate::errors::AppError;
use crate::handlers::AppState;
use crate::models::auth::{LoginRequest, LoginResponse, ServiceTokenRequest};
use crate::utils::jwt::{encode_access_token, encode_service_token};
use crate::utils::password::verify_password;

pub fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/service-token", post(issue_service_token))
        .route("/refresh", post(refresh_token))
        .route("/logout", post(logout))
}

/// Constant-time byte comparison, to avoid leaking the secret via timing.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Issue a network-plane service token (OAuth client-credentials style).
///
/// The card network/processor presents the shared `client_secret`; on a match we
/// mint a short-lived service JWT (`role: service`, no customer subject). The
/// signing key never leaves the server. Used by the rails (`/cards/*`).
async fn issue_service_token(
    State(state): State<AppState>,
    Json(payload): Json<ServiceTokenRequest>,
) -> Result<(StatusCode, Json<LoginResponse>), AppError> {
    payload.validate()?;

    if !constant_time_eq(
        payload.client_secret.as_bytes(),
        state.settings.security.service_client_secret.as_bytes(),
    ) {
        return Err(AppError::Authentication(
            "Invalid client credentials".to_string(),
        ));
    }

    let access_token = encode_service_token(&state.settings.jwt)?;

    tracing::info!("🔑 service token issued (network plane)");

    Ok((
        StatusCode::OK,
        Json(LoginResponse {
            access_token,
            token_type: "Bearer".to_string(),
            expires_in: state.settings.jwt.expires_in,
        }),
    ))
}

/// Authenticate a customer and issue a short-lived JWT access token.
///
/// Every failure path — unknown email, no stored credential, or a bad password —
/// returns the same generic 401 so the endpoint can't be used to discover which
/// emails are registered (account enumeration).
async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<(StatusCode, Json<LoginResponse>), AppError> {
    payload.validate()?;

    let invalid = || AppError::Authentication("Invalid email or password".to_string());

    // (customer_id, password_hash) for the email, if both the customer and a
    // credential row exist.
    let row: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT c.customer_id, cr.password_hash
         FROM customers c
         JOIN customer_credentials cr ON cr.customer_id = c.customer_id
         WHERE c.email = $1",
    )
    .bind(&payload.email)
    .fetch_optional(&state.pool)
    .await
    .map_err(AppError::Database)?;

    let (customer_id, password_hash) = row.ok_or_else(invalid)?;

    if !verify_password(&payload.password, &password_hash)? {
        return Err(invalid());
    }

    let access_token = encode_access_token(customer_id, &state.settings.jwt)?;

    tracing::info!(%customer_id, "🔑 customer logged in");

    Ok((
        StatusCode::OK,
        Json(LoginResponse {
            access_token,
            token_type: "Bearer".to_string(),
            expires_in: state.settings.jwt.expires_in,
        }),
    ))
}

async fn refresh_token() -> &'static str {
    "Refresh token endpoint - TODO: implement"
}

async fn logout() -> &'static str {
    "Logout endpoint - TODO: implement"
}
