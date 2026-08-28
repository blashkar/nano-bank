import "server-only";

/** Mirrors the API's `TransactionResponse` / entry models
 * (api/src/models/transaction.rs); numeric fields come back as JSON strings
 * (rust_decimal), not numbers. Enums are PascalCase in JSON (sqlx snake_case
 * mapping has no serde rename) — see the "Enum serialisation quirk" in
 * CLAUDE.md. */
export type TransactionStatus =
  | "Pending"
  | "Completed"
  | "Failed"
  | "Reversed"
  | "Cancelled";

export type EntryType = "Debit" | "Credit";

export interface TransactionEntryResponse {
  entry_id: string;
  account_id: string;
  entry_type: EntryType;
  amount: string;
  balance_after: string;
}

export interface TransactionResponse {
  transaction_id: string;
  reference_number: string;
  transaction_type: string;
  amount: string;
  currency: string;
  description: string | null;
  status: TransactionStatus;
  created_at: string;
  completed_at: string | null;
  entries: TransactionEntryResponse[];
}

export interface TransactionHistoryResponse {
  transactions: TransactionResponse[];
  total_count: number;
  has_more: boolean;
  next_offset: number | null;
}
