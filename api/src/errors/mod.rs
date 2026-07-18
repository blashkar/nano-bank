use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Authorization error: {0}")]
    Authorization(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    /// An error proxied from an upstream service (e.g. the ledger core), keeping
    /// its HTTP status so client/business errors aren't masked as 5xx.
    #[error("{message}")]
    Upstream { status: u16, message: String },

    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),

    #[error("Insufficient funds")]
    InsufficientFunds,

    #[error("Account frozen")]
    AccountFrozen,

    #[error("Transaction limit exceeded")]
    TransactionLimitExceeded,

    #[error("Invalid account status")]
    InvalidAccountStatus,

    #[error("Duplicate transaction")]
    DuplicateTransaction,

    #[error("KYC not completed")]
    KycNotCompleted,

    #[error("MFA required")]
    MfaRequired,

    #[error("Session expired")]
    SessionExpired,

    /// The mandate behind an agent token is revoked, expired, or its agent is
    /// disabled — the credential chain is dead (401, like `SessionExpired`).
    #[error("Mandate is not active")]
    MandateInactive,

    /// A valid agent identity asked for something its mandate doesn't allow.
    /// Carries a machine-readable reason code (e.g. `SCOPE_MISSING`).
    #[error("Denied by policy: {0}")]
    PolicyDenied(String),

    #[error("Device not trusted")]
    DeviceNotTrusted,

    /// The fraud engine declined this movement. Deliberately generic: reason
    /// codes live only in the engine's decision log (never leaked to callers,
    /// or transitively to agents).
    #[error(
        "This transaction cannot be completed. Contact support if you believe this is an error."
    )]
    TransactionDeclined,

    /// The fraud engine wants review/step-up before this movement completes.
    /// Carries the engine's customer-safe message verbatim.
    #[error("{0}")]
    TransactionUnderReview(String),

    #[error("Suspicious activity detected")]
    SuspiciousActivity,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_code, message) = match &self {
            AppError::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                "A database error occurred",
            ),
            AppError::Validation(msg) => {
                (StatusCode::BAD_REQUEST, "VALIDATION_ERROR", msg.as_str())
            }
            AppError::Authentication(msg) => (StatusCode::UNAUTHORIZED, "AUTH_ERROR", msg.as_str()),
            AppError::Authorization(msg) => {
                (StatusCode::FORBIDDEN, "AUTHORIZATION_ERROR", msg.as_str())
            }
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg.as_str()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, "CONFLICT", msg.as_str()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg.as_str()),
            AppError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                msg.as_str(),
            ),
            AppError::ServiceUnavailable(msg) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "SERVICE_UNAVAILABLE",
                msg.as_str(),
            ),
            AppError::Upstream { status, message } => (
                StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_GATEWAY),
                "UPSTREAM_ERROR",
                message.as_str(),
            ),
            AppError::RateLimit(msg) => (StatusCode::TOO_MANY_REQUESTS, "RATE_LIMIT", msg.as_str()),
            AppError::InsufficientFunds => (
                StatusCode::BAD_REQUEST,
                "INSUFFICIENT_FUNDS",
                "Insufficient funds for this transaction",
            ),
            AppError::AccountFrozen => (
                StatusCode::FORBIDDEN,
                "ACCOUNT_FROZEN",
                "Account is frozen and cannot be used",
            ),
            AppError::TransactionLimitExceeded => (
                StatusCode::BAD_REQUEST,
                "TRANSACTION_LIMIT_EXCEEDED",
                "Transaction limit exceeded",
            ),
            AppError::InvalidAccountStatus => (
                StatusCode::BAD_REQUEST,
                "INVALID_ACCOUNT_STATUS",
                "Account status does not allow this operation",
            ),
            AppError::DuplicateTransaction => (
                StatusCode::CONFLICT,
                "DUPLICATE_TRANSACTION",
                "Duplicate transaction detected",
            ),
            AppError::KycNotCompleted => (
                StatusCode::FORBIDDEN,
                "KYC_NOT_COMPLETED",
                "KYC verification must be completed",
            ),
            AppError::MfaRequired => (
                StatusCode::UNAUTHORIZED,
                "MFA_REQUIRED",
                "Multi-factor authentication required",
            ),
            AppError::SessionExpired => (
                StatusCode::UNAUTHORIZED,
                "SESSION_EXPIRED",
                "Session has expired",
            ),
            AppError::MandateInactive => (
                StatusCode::UNAUTHORIZED,
                "MANDATE_INACTIVE",
                "The mandate behind this token is not active",
            ),
            AppError::TransactionDeclined => (
                StatusCode::FORBIDDEN,
                "TRANSACTION_DECLINED",
                "This transaction cannot be completed. Contact support if you believe this is an error.",
            ),
            AppError::TransactionUnderReview(msg) => {
                (StatusCode::FORBIDDEN, "TRANSACTION_UNDER_REVIEW", msg.as_str())
            }
            AppError::PolicyDenied(reason) => {
                (StatusCode::FORBIDDEN, "POLICY_DENIED", reason.as_str())
            }
            AppError::DeviceNotTrusted => (
                StatusCode::FORBIDDEN,
                "DEVICE_NOT_TRUSTED",
                "Device is not trusted",
            ),
            AppError::SuspiciousActivity => (
                StatusCode::FORBIDDEN,
                "SUSPICIOUS_ACTIVITY",
                "Suspicious activity detected",
            ),
        };

        let body = json!({
            "error": {
                "code": error_code,
                "message": message,
                "details": self.to_string()
            }
        });

        // Log the error for monitoring
        tracing::error!(
            error_code = error_code,
            message = message,
            details = %self,
            "API error occurred"
        );

        (status, axum::Json(body)).into_response()
    }
}

// Helper for validation errors
impl From<validator::ValidationErrors> for AppError {
    fn from(errors: validator::ValidationErrors) -> Self {
        let error_messages: Vec<String> = errors
            .field_errors()
            .iter()
            .map(|(field, errors)| {
                let messages: Vec<String> = errors
                    .iter()
                    .map(|error| match error.message.as_ref() {
                        Some(msg) => format!("{}: {}", field, msg),
                        None => format!("{}: validation failed", field),
                    })
                    .collect();
                messages.join(", ")
            })
            .collect();

        AppError::Validation(error_messages.join("; "))
    }
}

// Helper for JWT errors
impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        match err.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => AppError::SessionExpired,
            _ => AppError::Authentication("Invalid token".to_string()),
        }
    }
}

// Banking-specific error types
pub type BankingResult<T> = Result<T, AppError>;

// Helper macros for common banking errors
#[macro_export]
macro_rules! insufficient_funds {
    () => {
        return Err(crate::errors::AppError::InsufficientFunds)
    };
}

#[macro_export]
macro_rules! account_frozen {
    () => {
        return Err(crate::errors::AppError::AccountFrozen)
    };
}

#[macro_export]
macro_rules! kyc_required {
    () => {
        return Err(crate::errors::AppError::KycNotCompleted)
    };
}

pub use {account_frozen, insufficient_funds, kyc_required};
