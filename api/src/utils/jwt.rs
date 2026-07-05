//! JWT access-token encoding/decoding.
//!
//! Three token kinds share one signing key and claim shape, distinguished by
//! `role`:
//!   - **customer** tokens (`sub` = customer id) authenticate the consumer-app
//!     plane (`/customers/*`, `/accounts/*`). Issued by `POST /auth/login`.
//!   - **service** tokens (no `sub`) authenticate the network plane
//!     (`/cards/*`), where the caller is the card network/processor, not a
//!     cardholder. Issued by `POST /auth/service-token`.
//!   - **agent** tokens (`sub` = agent id, `act` = the customer acted for,
//!     `mnd` = the mandate) authenticate the agent plane (`/agent/*`). Issued
//!     by `POST /auth/agent-token`. Deliberately a *pointer*: scopes and limits
//!     live only in the mandate row, which is re-read on every request — so
//!     revoking the mandate kills every outstanding token immediately.
//!
//! Access tokens are short-lived (customer/service TTL = `jwt.expires_in`,
//! agent TTL = `jwt.agent_expires_in`) and stateless.

use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::JwtSettings;
use crate::errors::AppError;

/// Which trust plane a token belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TokenRole {
    /// A cardholder using the consumer app.
    Customer,
    /// A machine principal (the card network/processor) driving the rails.
    Service,
    /// An AI agent acting for a customer under a mandate.
    Agent,
}

/// Registered + custom claims carried by an access token.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — the customer's id for customer tokens; absent for service tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<Uuid>,
    /// Session id (the revocable `user_sessions` row) for customer tokens; absent
    /// for service tokens. Lets logout terminate exactly this session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sid: Option<Uuid>,
    /// Acting-for customer id — agent tokens only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub act: Option<Uuid>,
    /// Mandate id (the revocable consent row) — agent tokens only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mnd: Option<Uuid>,
    /// Trust plane this token authenticates.
    pub role: TokenRole,
    /// Issuer — must match `jwt.issuer`.
    pub iss: String,
    /// Issued-at (unix seconds).
    pub iat: i64,
    /// Expiry (unix seconds).
    pub exp: i64,
}

fn encode_claims(claims: &Claims, jwt: &JwtSettings) -> Result<String, AppError> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(jwt.secret.as_bytes()),
    )
    .map_err(AppError::from)
}

fn base_claims(role: TokenRole, ttl: i64, jwt: &JwtSettings) -> Claims {
    let now = Utc::now().timestamp();
    Claims {
        sub: None,
        sid: None,
        act: None,
        mnd: None,
        role,
        iss: jwt.issuer.clone(),
        iat: now,
        exp: now + ttl,
    }
}

/// Mint a customer (consumer-app) access token bound to a login session.
pub fn encode_access_token(
    customer_id: Uuid,
    session_id: Uuid,
    jwt: &JwtSettings,
) -> Result<String, AppError> {
    let claims = Claims {
        sub: Some(customer_id),
        sid: Some(session_id),
        ..base_claims(TokenRole::Customer, jwt.expires_in, jwt)
    };
    encode_claims(&claims, jwt)
}

/// Mint a service (network-plane) access token. No customer subject or session.
pub fn encode_service_token(jwt: &JwtSettings) -> Result<String, AppError> {
    encode_claims(&base_claims(TokenRole::Service, jwt.expires_in, jwt), jwt)
}

/// Mint an agent (agent-plane) access token — a short-lived *pointer* to a
/// mandate. Carries no scopes or limits: those are read from the mandate row
/// on every request so revocation is immediate.
pub fn encode_agent_token(
    agent_id: Uuid,
    customer_id: Uuid,
    mandate_id: Uuid,
    jwt: &JwtSettings,
) -> Result<String, AppError> {
    let claims = Claims {
        sub: Some(agent_id),
        act: Some(customer_id),
        mnd: Some(mandate_id),
        ..base_claims(TokenRole::Agent, jwt.agent_expires_in, jwt)
    };
    encode_claims(&claims, jwt)
}

/// Verify a token's signature, expiry, and issuer, returning its claims.
pub fn decode_access_token(token: &str, jwt: &JwtSettings) -> Result<Claims, AppError> {
    let mut validation = Validation::default();
    validation.set_issuer(&[&jwt.issuer]);
    // `exp` is validated by default.

    decode::<Claims>(
        token,
        &DecodingKey::from_secret(jwt.secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(AppError::from)
}
