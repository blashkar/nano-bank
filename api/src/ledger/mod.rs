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
