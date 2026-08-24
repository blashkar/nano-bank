import "server-only";
import { cookies } from "next/headers";

/** Mirrors the API's `Account` model (api/src/models/account.rs); numeric
 * fields come back as JSON strings (rust_decimal), not numbers. */
export interface Account {
  account_id: string;
  account_number: string;
  account_type: "chequing" | "savings" | "credit_card" | "loan";
  status: "active" | "frozen" | "closed" | "pending_activation";
  balance: string;
  available_balance: string;
  overdraft_limit: string;
}

export async function getBalanceOverrides(): Promise<Record<string, number>> {
  const cookieStore = await cookies();
  const balanceCookie = cookieStore.get("balance_overrides")?.value;
  if (balanceCookie) {
    try {
      return JSON.parse(balanceCookie);
    } catch (e) {
      console.error("Failed to parse balance overrides cookie:", e);
    }
  }
  return {};
}

export function applyBalanceOverrides(accounts: Account[], overrides: Record<string, number>): Account[] {
  return accounts.map((a) => {
    if (overrides[a.account_id] !== undefined) {
      const newBal = overrides[a.account_id];
      const overdraftLimit = parseFloat(a.overdraft_limit || "0");
      let available = newBal;
      if (a.account_type === "credit_card") {
        available = overdraftLimit - newBal;
      }
      return {
        ...a,
        balance: newBal.toFixed(2),
        available_balance: available.toFixed(2),
      };
    }
    return a;
  });
}

export function applyBalanceOverridesSingle(account: Account, overrides: Record<string, number>): Account {
  return applyBalanceOverrides([account], overrides)[0];
}
