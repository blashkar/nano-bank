//! Bearer-token extractors for the three trust planes.
//!
//! - [`AuthenticatedCustomer`] turns a customer (`role: customer`) token into the
//!   caller's `customer_id`, for the consumer-app endpoints (`/customers/*`,
//!   `/accounts/*`). It replaces the old `?customer_id=` query param so identity
//!   is derived from the verified token, not trusted from the client.
//! - [`AuthenticatedService`] accepts only a service (`role: service`) token, for
//!   the network-plane rails (`/cards/*`), where the caller is the card
//!   network/processor rather than a cardholder.
//! - [`AuthenticatedAgent`] accepts only an agent (`role: agent`) token, for the
//!   agent plane (`/agent/*`). Deliberately **stateful**: the token is a pointer
//!   to a mandate, so the extractor re-reads the mandate (and its agent) on
//!   every request — revoking the mandate or disabling the agent takes effect
//!   on the very next call, regardless of outstanding token TTLs.
//!
//! All read only request headers (the agent extractor additionally hits the DB),
//! implement `FromRequestParts`, and can sit before a `Json` body extractor in a
//! handler signature.

use axum::async_trait;
use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::errors::AppError;
use crate::handlers::AppState;
use crate::utils::jwt::{decode_access_token, Claims, TokenRole};

/// Pull and verify the `Authorization: Bearer <jwt>` token, returning its claims.
fn bearer_claims(parts: &Parts, state: &AppState) -> Result<Claims, AppError> {
    let token = parts
        .headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            AppError::Authentication("Missing or malformed Authorization header".to_string())
        })?;

    decode_access_token(token, &state.settings.jwt)
}

/// A cardholder authenticated on the consumer-app plane.
pub struct AuthenticatedCustomer {
    pub customer_id: Uuid,
    /// The login session this token belongs to (`user_sessions.session_id`).
    /// `None` for tokens minted before sessions existed; logout no-ops on those.
    pub session_id: Option<Uuid>,
}

#[async_trait]
impl FromRequestParts<AppState> for AuthenticatedCustomer {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let claims = bearer_claims(parts, state)?;
        // A service token must never be accepted on a consumer endpoint.
        match (claims.role, claims.sub) {
            (TokenRole::Customer, Some(customer_id)) => Ok(AuthenticatedCustomer {
                customer_id,
                session_id: claims.sid,
            }),
            _ => Err(AppError::Authentication(
                "A customer access token is required".to_string(),
            )),
        }
    }
}

/// An AI agent authenticated on the agent plane, resolved to its live mandate.
///
/// `account_id` is the *only* account the agent surface can reach — handlers
/// never take an account parameter, which closes the confused-deputy hole.
pub struct AuthenticatedAgent {
    pub agent_id: Uuid,
    pub mandate_id: Uuid,
    /// The customer being acted for (the mandate's grantor).
    pub customer_id: Uuid,
    /// The single account the mandate covers.
    pub account_id: Uuid,
    /// Scopes as granted — read from the row, never from the token.
    pub scopes: Vec<String>,
}

#[async_trait]
impl FromRequestParts<AppState> for AuthenticatedAgent {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let claims = bearer_claims(parts, state)?;
        // A valid customer/service token on an agent endpoint is the wrong
        // plane → 403 (mirrors AuthenticatedService).
        let (agent_id, mandate_id) = match (claims.role, claims.sub, claims.mnd) {
            (TokenRole::Agent, Some(agent_id), Some(mandate_id)) => (agent_id, mandate_id),
            (TokenRole::Agent, _, _) => {
                return Err(AppError::Authentication(
                    "Malformed agent token".to_string(),
                ))
            }
            _ => {
                return Err(AppError::Authorization(
                    "An agent access token is required".to_string(),
                ))
            }
        };

        // Re-read the grant: the mandate row is the source of truth for
        // status, expiry, scopes, and the agent kill switch.
        let row: Option<(Uuid, Uuid, Vec<String>, String, String, bool)> = sqlx::query_as(
            "SELECT m.customer_id, m.account_id, m.scopes, m.status, a.status, \
                    m.expires_at <= CURRENT_TIMESTAMP \
             FROM mandates m JOIN agents a ON a.agent_id = m.agent_id \
             WHERE m.mandate_id = $1 AND m.agent_id = $2",
        )
        .bind(mandate_id)
        .bind(agent_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(AppError::Database)?;

        let Some((customer_id, account_id, scopes, m_status, a_status, expired)) = row else {
            // A signed token referencing a missing mandate row (e.g. wiped DB).
            return Err(AppError::MandateInactive);
        };

        if expired && m_status == "active" {
            // Lazy expiry: best-effort guarded flip so the row reflects reality.
            let _ = sqlx::query(
                "UPDATE mandates SET status = 'expired' \
                 WHERE mandate_id = $1 AND status = 'active'",
            )
            .bind(mandate_id)
            .execute(&state.pool)
            .await;
        }
        if m_status != "active" || a_status != "active" || expired {
            return Err(AppError::MandateInactive);
        }

        Ok(AuthenticatedAgent {
            agent_id,
            mandate_id,
            customer_id,
            account_id,
            scopes,
        })
    }
}

/// A machine principal (the card network/processor) authenticated on the
/// network plane. Carries no customer identity by design.
pub struct AuthenticatedService;

#[async_trait]
impl FromRequestParts<AppState> for AuthenticatedService {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let claims = bearer_claims(parts, state)?;
        // A valid customer token on a network endpoint is the wrong plane → 403.
        if claims.role == TokenRole::Service {
            Ok(AuthenticatedService)
        } else {
            Err(AppError::Authorization(
                "A service access token is required for the card rails".to_string(),
            ))
        }
    }
}
