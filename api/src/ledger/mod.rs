//! The **Ledger port**: nano-bank's backend-agnostic interface to an accounting
//! core. Two interchangeable adapters implement it over HTTP — the modern Rust
//! core and the legacy core — selected at startup by `CORE_BACKEND`.
//!
//! The port speaks neutral, semantic terms (an [`Account`] role, a [`Direction`],
//! `Decimal` money). Each adapter maps those onto its backend's real account
//! identifiers, so nano-bank never needs to know either backend's numbering.

pub mod legacy;
pub mod modern;

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// A semantic general-ledger account, independent of any backend's numbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Account {
    Bank,
    Receivable,
    Payable,
    Revenue,
    Expense,
    // Bank-economics chart (spec: GL chart-of-accounts expansion). Additive only.
    CashReserves,
    CardReceivable,
    OverdraftReceivable,
    LoansReceivable,
    TreasuryPlacement,
    CustomerDeposits,
    Capital,
    RetainedEarnings,
    InterestIncome,
    InterchangeIncome,
    FeeIncome,
    InterestExpense,
    OperatingExpense,
    // Interest / NIM engine (spec #2). Daily-accrual holding accounts.
    AccruedInterestReceivable,
    AccruedInterestPayable,
}

impl Account {
    /// The modern core's GL code for this account.
    pub fn modern_code(self) -> &'static str {
        match self {
            Account::Bank => "BANK",
            Account::Receivable => "AR",
            Account::Payable => "AP",
            Account::Revenue => "REVENUE",
            Account::Expense => "EXPENSE",
            Account::CashReserves => "CASH_RESERVES",
            Account::CardReceivable => "CARD_AR",
            Account::OverdraftReceivable => "OVERDRAFT_AR",
            Account::LoansReceivable => "LOANS_AR",
            Account::TreasuryPlacement => "TREASURY",
            Account::CustomerDeposits => "DEPOSITS",
            Account::Capital => "CAPITAL",
            Account::RetainedEarnings => "RETAINED",
            Account::InterestIncome => "INT_INCOME",
            Account::InterchangeIncome => "INTERCHANGE",
            Account::FeeIncome => "FEE_INCOME",
            Account::InterestExpense => "INT_EXPENSE",
            Account::OperatingExpense => "OPEX",
            Account::AccruedInterestReceivable => "ACCR_INT_RECV",
            Account::AccruedInterestPayable => "ACCR_INT_PAY",
        }
    }

    /// The legacy core's GL account number for this account.
    pub fn legacy_account(self) -> &'static str {
        match self {
            Account::Bank => "0000113100",
            Account::Receivable => "0000140000",
            Account::Payable => "0000160000",
            Account::Revenue => "0000800000",
            Account::Expense => "0000400000",
            Account::CashReserves => "0000105000",
            Account::CardReceivable => "0000141000",
            Account::OverdraftReceivable => "0000141500",
            Account::LoansReceivable => "0000142000",
            Account::TreasuryPlacement => "0000150000",
            Account::CustomerDeposits => "0000210000",
            Account::Capital => "0000300000",
            Account::RetainedEarnings => "0000330000",
            Account::InterestIncome => "0000800100",
            Account::InterchangeIncome => "0000800200",
            Account::FeeIncome => "0000800300",
            Account::InterestExpense => "0000400100",
            Account::OperatingExpense => "0000400200",
            Account::AccruedInterestReceivable => "0000141900",
            Account::AccruedInterestPayable => "0000220000",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Debit,
    Credit,
}

impl Direction {
    pub fn modern(self) -> &'static str {
        match self {
            Direction::Debit => "debit",
            Direction::Credit => "credit",
        }
    }

    /// The legacy core's debit/credit indicator (S = debit, H = credit).
    pub fn legacy(self) -> &'static str {
        match self {
            Direction::Debit => "S",
            Direction::Credit => "H",
        }
    }
}

#[derive(Debug, Clone)]
pub struct EntryLine {
    pub account: Account,
    pub direction: Direction,
    pub amount: Decimal,
}

#[derive(Debug, Clone)]
pub struct NewEntry {
    pub reference: Option<String>,
    pub description: Option<String>,
    pub lines: Vec<EntryLine>,
}

#[derive(Debug, Serialize)]
pub struct PostedEntry {
    /// The backend's document id (modern: numeric id; legacy: `belnr`).
    pub id: String,
    pub backend: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccountBalance {
    pub account: String,
    pub balance: Decimal,
}

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("ledger backend returned {status}: {body}")]
    Backend { status: u16, body: String },
    #[error("ledger transport error: {0}")]
    Transport(String),
    #[error("local database error: {0}")]
    Database(String),
}

impl From<sqlx::Error> for LedgerError {
    fn from(e: sqlx::Error) -> Self {
        LedgerError::Database(e.to_string())
    }
}

/// The accounting core seen by nano-bank. Kept intentionally small for this pass
/// (post + read balances); reversal/clearing/dunning can be added the same way.
#[async_trait]
pub trait Ledger: Send + Sync {
    /// Which backend this is ("modern" | "legacy"), for diagnostics.
    fn backend(&self) -> &'static str;

    /// Post a balanced journal entry; returns the backend's document id.
    async fn post_entry(&self, entry: NewEntry) -> Result<PostedEntry, LedgerError>;

    /// Trial-balance style totals per account, in company-code currency.
    async fn balances(&self) -> Result<Vec<AccountBalance>, LedgerError>;
}

/// A fixed, arbitrary key for the Postgres session advisory lock that
/// serializes [`ensure_seed_capital`] across concurrent boots. Any i64 works;
/// this one is just `"CAPITAL-SEED"`'s first 8 bytes as an i64, so it doesn't
/// collide with other advisory locks in the codebase by chance.
const SEED_CAPITAL_LOCK_KEY: i64 = 0x4341_5049_5441_4c2d;

/// Idempotent boot-time bootstrap: if the bank has never been capitalized (no
/// nonzero `Capital` balance yet), post a balanced founding journal entry
/// (debit `Bank`, credit `Capital`) for `amount` — so a fresh boot starts as a
/// properly capitalized bank instead of a hollow shell running purely on
/// customer liabilities (that gap is what made every leverage/RWA-capital
/// ratio the CFO reports read as ~0.1% instead of a real bank's ~10%+).
/// Safe to call on every boot: once `Capital` is nonzero — from this bootstrap
/// or a later real capital event — it is left alone. Returns whether it
/// actually seeded, for startup logging.
///
/// The read-then-post is a check-then-act over an HTTP-backed ledger (no
/// local transaction can wrap it), so a Postgres session advisory lock on
/// `pool` — the same local DB every boot already talks to — serializes
/// concurrent callers (e.g. two pods alive during a k8s `RollingUpdate`): a
/// racing caller blocks here until the winner's post has landed, then
/// observes the now-nonzero balance and safely no-ops, instead of both
/// racing the check and both posting.
pub async fn ensure_seed_capital(
    ledger: &dyn Ledger,
    pool: &sqlx::PgPool,
    amount: Decimal,
) -> Result<bool, LedgerError> {
    let mut conn = pool.acquire().await?;
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(SEED_CAPITAL_LOCK_KEY)
        .execute(&mut *conn)
        .await?;

    let result = ensure_seed_capital_locked(ledger, amount).await;

    // Always release, even if the check/post above failed, so a failed boot
    // doesn't strand the lock on this pooled connection for its next borrower.
    let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(SEED_CAPITAL_LOCK_KEY)
        .execute(&mut *conn)
        .await;

    result
}

async fn ensure_seed_capital_locked(
    ledger: &dyn Ledger,
    amount: Decimal,
) -> Result<bool, LedgerError> {
    let balances = ledger.balances().await?;
    let cap_modern = Account::Capital.modern_code();
    let cap_legacy = Account::Capital.legacy_account();
    let already_capitalized = balances
        .iter()
        .any(|b| (b.account == cap_modern || b.account == cap_legacy) && !b.balance.is_zero());
    if already_capitalized {
        return Ok(false);
    }
    ledger
        .post_entry(NewEntry {
            reference: Some("CAPITAL-SEED".into()),
            description: Some("Founding shareholder capital injection (boot bootstrap)".into()),
            lines: vec![
                EntryLine {
                    account: Account::Bank,
                    direction: Direction::Debit,
                    amount,
                },
                EntryLine {
                    account: Account::Capital,
                    direction: Direction::Credit,
                    amount,
                },
            ],
        })
        .await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::Account;

    /// Every new role maps to the agreed modern code and legacy saknr.
    /// Existing roles are asserted too, to catch accidental edits.
    #[test]
    fn account_mappings_are_stable() {
        let cases = [
            // (role, modern_code, legacy_account)
            (Account::Bank, "BANK", "0000113100"),
            (Account::Receivable, "AR", "0000140000"),
            (Account::Payable, "AP", "0000160000"),
            (Account::Revenue, "REVENUE", "0000800000"),
            (Account::Expense, "EXPENSE", "0000400000"),
            (Account::CashReserves, "CASH_RESERVES", "0000105000"),
            (Account::CardReceivable, "CARD_AR", "0000141000"),
            (Account::OverdraftReceivable, "OVERDRAFT_AR", "0000141500"),
            (Account::LoansReceivable, "LOANS_AR", "0000142000"),
            (Account::TreasuryPlacement, "TREASURY", "0000150000"),
            (Account::CustomerDeposits, "DEPOSITS", "0000210000"),
            (Account::Capital, "CAPITAL", "0000300000"),
            (Account::RetainedEarnings, "RETAINED", "0000330000"),
            (Account::InterestIncome, "INT_INCOME", "0000800100"),
            (Account::InterchangeIncome, "INTERCHANGE", "0000800200"),
            (Account::FeeIncome, "FEE_INCOME", "0000800300"),
            (Account::InterestExpense, "INT_EXPENSE", "0000400100"),
            (Account::OperatingExpense, "OPEX", "0000400200"),
            (
                Account::AccruedInterestReceivable,
                "ACCR_INT_RECV",
                "0000141900",
            ),
            (
                Account::AccruedInterestPayable,
                "ACCR_INT_PAY",
                "0000220000",
            ),
        ];
        for (role, modern, legacy) in cases {
            assert_eq!(role.modern_code(), modern, "modern_code for {role:?}");
            assert_eq!(role.legacy_account(), legacy, "legacy_account for {role:?}");
        }
    }

    /// The JSON wire name for each new role is its snake_case identifier,
    /// which is what `/ledger/journal` accepts.
    #[test]
    fn new_roles_deserialize_from_snake_case() {
        let json = r#"["cash_reserves","customer_deposits","interest_income","interest_expense","retained_earnings"]"#;
        let roles: Vec<Account> = serde_json::from_str(json).expect("valid roles");
        assert_eq!(
            roles,
            vec![
                Account::CashReserves,
                Account::CustomerDeposits,
                Account::InterestIncome,
                Account::InterestExpense,
                Account::RetainedEarnings,
            ]
        );
    }
}
