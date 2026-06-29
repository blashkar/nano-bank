use serde::{Deserialize, Serialize};
use validator::Validate;

/// Login credentials submitted to `POST /api/v1/auth/login`.
#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(email)]
    pub email: String,

    #[validate(length(min = 1))]
    pub password: String,
}

/// Client-credentials request for a network-plane service token, submitted to
/// `POST /api/v1/auth/service-token` by the card network/processor.
#[derive(Debug, Deserialize, Validate)]
pub struct ServiceTokenRequest {
    #[validate(length(min = 1))]
    pub client_secret: String,
}

/// Issued access token. `token_type` is always `"Bearer"`; `expires_in` is the
/// token lifetime in seconds (mirrors `jwt.expires_in`). Used for both customer
/// login and service-token issuance.
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
}
