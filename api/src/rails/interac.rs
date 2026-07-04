//! Interac e-Transfer rail: the clearing/settlement plumbing. The product
//! lifecycle lives in `handlers/interac.rs`.

use rust_decimal::Decimal;
use uuid::Uuid;

use crate::config::database::DatabasePool;

use super::{Hold, RailId};

// TEMP until Task 5 — delete when models::interac::HandleType exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleType {
    Email,
    Phone,
}

/// Interac's own synthetic system customer — SEPARATE from the card rails'
/// `system@nano.bank`, because GL accounts are keyed by (customer, account_type)
/// and that customer already uses its chequing/savings for VISA_CLEARING /
/// BANK_SETTLEMENT.
const INTERAC_CUSTOMER_EMAIL: &str = "interac@nano.bank";
const CLEARING_TYPE: &str = "chequing"; // INTERAC_CLEARING
const SETTLEMENT_TYPE: &str = "savings"; // INTERAC_SETTLEMENT

#[derive(Clone, Copy, Debug)]
pub struct InteracAccounts {
    pub clearing_id: Uuid,
    pub settlement_id: Uuid,
}

/// The Interac rail. Carries the resolved clearing/settlement ids (re-resolved
/// per request by the handler, because a data wipe rebuilds them).
#[derive(Clone, Copy, Debug)]
pub struct InteracRail {
    pub accounts: InteracAccounts,
}

impl InteracRail {
    pub fn new(accounts: InteracAccounts) -> Self {
        Self { accounts }
    }
    pub fn id(&self) -> RailId {
        RailId::Interac
    }
}

/// Normalise a handle for storage/lookup: emails lowercased+trimmed; phones
/// reduced to a leading '+' (if present) and digits.
pub fn normalize_handle(handle_type: HandleType, raw: &str) -> String {
    match handle_type {
        HandleType::Email => raw.trim().to_lowercase(),
        HandleType::Phone => {
            let mut out = String::new();
            for (i, c) in raw.trim().chars().enumerate() {
                if c == '+' && i == 0 {
                    out.push('+');
                } else if c.is_ascii_digit() {
                    out.push(c);
                }
            }
            out
        }
    }
}

/// Create Interac's system customer + two GL accounts if absent; return ids.
/// Idempotent — mirrors `handlers::cards::ensure_system_accounts`.
pub async fn ensure_interac_accounts(
    pool: &DatabasePool,
) -> Result<InteracAccounts, sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO customers (email, phone_number, first_name, last_name, date_of_birth, sin)
        VALUES ($1, '+10000000002', 'Nano', 'Interac', '1970-01-01', '000000002')
        ON CONFLICT (email) DO NOTHING
        "#,
    )
    .bind(INTERAC_CUSTOMER_EMAIL)
    .execute(pool)
    .await?;

    let customer_id: Uuid =
        sqlx::query_scalar("SELECT customer_id FROM customers WHERE email = $1")
            .bind(INTERAC_CUSTOMER_EMAIL)
            .fetch_one(pool)
            .await?;

    let clearing_id = ensure_gl_account(pool, customer_id, CLEARING_TYPE).await?;
    let settlement_id = ensure_gl_account(pool, customer_id, SETTLEMENT_TYPE).await?;
    tracing::info!(%clearing_id, %settlement_id, "✅ Interac GL accounts ready");
    Ok(InteracAccounts { clearing_id, settlement_id })
}

async fn ensure_gl_account(
    pool: &DatabasePool,
    customer_id: Uuid,
    account_type: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO accounts
            (customer_id, account_number, account_type, status, overdraft_limit, activated_at)
        SELECT $1, '000000000000', $2::account_type, 'active', 1000000000000, CURRENT_TIMESTAMP
        WHERE NOT EXISTS (
            SELECT 1 FROM accounts WHERE customer_id = $1 AND account_type = $2::account_type
        )
        "#,
    )
    .bind(customer_id)
    .bind(account_type)
    .execute(pool)
    .await?;

    sqlx::query_scalar(
        "SELECT account_id FROM accounts WHERE customer_id = $1 AND account_type = $2::account_type \
         ORDER BY created_at LIMIT 1",
    )
    .bind(customer_id)
    .bind(account_type)
    .fetch_one(pool)
    .await
}

// keep the Hold type referenced so imports don't rot before Task 4
#[allow(unused_imports)]
use super::Destination as _Destination;
#[allow(dead_code)]
fn _hold_marker(_: &Hold) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_handles_are_lowercased_and_trimmed() {
        assert_eq!(normalize_handle(HandleType::Email, "  Alice@Example.COM "), "alice@example.com");
    }

    #[test]
    fn phone_handles_keep_only_digits_and_plus() {
        assert_eq!(normalize_handle(HandleType::Phone, "+1 (416) 555-0199"), "+14165550199");
    }
}
