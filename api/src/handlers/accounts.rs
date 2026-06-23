use axum::{extract::State, http::StatusCode, response::Json, routing::get, Router};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::errors::AppError;
use crate::handlers::AppState;
use crate::models::account::{Account, AccountResponse, AccountType, CreateAccountRequest};

/// Interest ("return") rate applied to each account type, as a fraction (3% = 0.03).
/// Chequing earns 3%; savings has no rate set yet.
fn interest_rate_for(account_type: &AccountType) -> Decimal {
    match account_type {
        AccountType::Checking => Decimal::new(300, 4), // 0.0300 = 3%
        AccountType::Savings => Decimal::ZERO,
    }
}

/// Generate a 12-digit numeric account number (Canadian format, `^[0-9]{12}$`).
/// Derived from a v4 UUID's low bits — no `rand` dependency needed. Collisions
/// are astronomically unlikely and handled by a retry on the unique constraint.
fn generate_account_number() -> String {
    let n = (Uuid::new_v4().as_u128() % 1_000_000_000_000) as u64;
    format!("{:012}", n)
}

pub fn account_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_accounts).post(create_account))
        .route("/:id", get(get_account))
        .route("/:id/balance", get(get_balance))
}

async fn get_accounts() -> &'static str {
    "Get accounts endpoint - TODO: implement"
}

/// Open a new account for a customer.
///
/// Inserts a row into `accounts` with the per-type interest rate and an `active`
/// status. The account starts with a zero balance — funding happens through the
/// (not-yet-implemented) transaction ledger, so `initial_deposit` is accepted
/// but ignored for now to keep the double-entry invariant intact.
async fn create_account(
    State(state): State<AppState>,
    Json(payload): Json<CreateAccountRequest>,
) -> Result<(StatusCode, Json<AccountResponse>), AppError> {
    let interest_rate = interest_rate_for(&payload.account_type);

    // Retry a few times in the (vanishingly rare) event of an account-number clash.
    let mut last_err = None;
    for _ in 0..5 {
        let account_number = generate_account_number();
        let result = sqlx::query_as::<_, Account>(
            r#"
            INSERT INTO accounts
                (customer_id, account_number, account_type, interest_rate,
                 status, activated_at)
            VALUES ($1, $2, $3, $4, 'active', CURRENT_TIMESTAMP)
            RETURNING
                account_id, customer_id, account_number, account_type, currency,
                balance, available_balance, status, interest_rate, overdraft_limit,
                minimum_balance, created_at, updated_at, activated_at, closed_at
            "#,
        )
        .bind(payload.customer_id)
        .bind(&account_number)
        .bind(&payload.account_type)
        .bind(interest_rate)
        .fetch_one(&state.pool)
        .await;

        match result {
            Ok(account) => {
                tracing::info!(
                    account_id = %account.account_id,
                    customer_id = %account.customer_id,
                    account_number = %account.account_number,
                    account_type = ?account.account_type,
                    interest_rate = %account.interest_rate,
                    "✅ account created"
                );
                return Ok((StatusCode::CREATED, Json(account.into())));
            }
            Err(sqlx::Error::Database(db)) => match db.code().as_deref() {
                // unique_violation on account_number: regenerate and retry.
                Some("23505") => {
                    last_err = Some(sqlx::Error::Database(db));
                    continue;
                }
                // foreign_key_violation: the customer_id doesn't exist.
                Some("23503") => {
                    return Err(AppError::BadRequest(
                        "No customer exists with that customer_id".to_string(),
                    ))
                }
                // check_violation: e.g. bad account-number format or balance rule.
                Some("23514") => {
                    return Err(AppError::BadRequest(format!(
                        "Account data rejected: {}",
                        db.message()
                    )))
                }
                _ => return Err(AppError::Database(sqlx::Error::Database(db))),
            },
            Err(e) => return Err(AppError::Database(e)),
        }
    }

    Err(AppError::Database(last_err.unwrap_or_else(|| {
        sqlx::Error::Protocol("could not allocate a unique account number".into())
    })))
}

async fn get_account() -> &'static str {
    "Get account endpoint - TODO: implement"
}

async fn get_balance() -> &'static str {
    "Get balance endpoint - TODO: implement"
}