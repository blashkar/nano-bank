"use server";

import { cookies } from "next/headers";
import { revalidatePath } from "next/cache";
import { API_BASE_URL } from "@/lib/config";
import { friendlyErrorMessage, type ApiErrorBody } from "@/lib/errors";

/** Mirrors the API's `AccountResponse` (api/src/models/account.rs); numeric
 * fields come back as JSON strings (rust_decimal), not numbers. */
interface AccountResponseBody {
  account_id: string;
  account_number: string;
  account_type: "chequing" | "savings" | "credit_card";
}

export interface CreateAccountResult {
  success: boolean;
  message: string;
  accountId?: string;
}

/** The only account types this form opens — credit cards are opened through a
 * separate flow (see api/src/handlers/accounts.rs `opening_terms`). */
const OPENABLE_ACCOUNT_TYPES = new Set(["chequing", "savings"]);

export async function createAccountAction(formData: FormData): Promise<CreateAccountResult> {
  const accountType = formData.get("accountType");

  if (typeof accountType !== "string" || !OPENABLE_ACCOUNT_TYPES.has(accountType)) {
    return { success: false, message: "Please select an account type." };
  }

  // One key per form mount (see CreateAccountForm) so a double-click or a
  // retry after a dropped response replays the original account instead of
  // opening a second one.
  const idempotencyKey = formData.get("idempotencyKey");

  const cookieStore = await cookies();
  const accessToken = cookieStore.get("access_token")?.value;
  if (!accessToken) {
    return { success: false, message: "Your session has expired. Please sign in again." };
  }

  let response: Response;
  try {
    response = await fetch(`${API_BASE_URL}/api/v1/accounts`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        Authorization: `Bearer ${accessToken}`,
      },
      body: JSON.stringify({
        account_type: accountType,
        idempotency_key: typeof idempotencyKey === "string" ? idempotencyKey : undefined,
      }),
      cache: "no-store",
    });
  } catch (error) {
    console.error("Account creation request failed:", error);
    return { success: false, message: "Unable to reach the server. Please try again." };
  }

  if (!response.ok) {
    let message = "Unable to open account.";
    try {
      const errorBody: ApiErrorBody = await response.json();
      message = friendlyErrorMessage(errorBody, message);
    } catch (error) {
      console.error("Failed to parse account creation error response:", error);
    }
    return { success: false, message };
  }

  const account: AccountResponseBody = await response.json();
  revalidatePath("/dashboard/accounts");

  return {
    success: true,
    message: `Your new ${accountType} account is ready!`,
    accountId: account.account_id,
  };
}

export interface TransferMoneyResult {
  success: boolean;
  message: string;
}

export async function transferMoneyAction(formData: FormData): Promise<TransferMoneyResult> {
  const fromAccountId = formData.get("fromAccountId");
  const toAccountId = formData.get("toAccountId");
  const amountRaw = formData.get("amount");
  const description = formData.get("description");
  // One key per form mount (see TransferForm) so a double-click or a retry
  // after a dropped response replays the original transfer instead of moving
  // the money twice.
  const idempotencyKey = formData.get("idempotencyKey");

  if (typeof fromAccountId !== "string" || !fromAccountId) {
    return { success: false, message: "Please choose an account to transfer from." };
  }
  if (typeof toAccountId !== "string" || !toAccountId) {
    return { success: false, message: "Please choose an account to transfer to." };
  }
  if (fromAccountId === toAccountId) {
    return { success: false, message: "The from and to accounts must be different." };
  }

  const amount = typeof amountRaw === "string" ? Number(amountRaw) : NaN;
  if (!Number.isFinite(amount) || amount <= 0) {
    return { success: false, message: "Enter an amount greater than $0.00." };
  }

  const cookieStore = await cookies();
  const accessToken = cookieStore.get("access_token")?.value;
  if (!accessToken) {
    return { success: false, message: "Your session has expired. Please sign in again." };
  }

  let response: Response;
  try {
    response = await fetch(`${API_BASE_URL}/api/v1/transactions/transfer`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        Authorization: `Bearer ${accessToken}`,
      },
      body: JSON.stringify({
        from_account_id: fromAccountId,
        to_account_id: toAccountId,
        amount,
        description: typeof description === "string" && description.trim() ? description.trim() : "Transfer between own accounts",
        idempotency_key: typeof idempotencyKey === "string" ? idempotencyKey : undefined,
      }),
      cache: "no-store",
    });
  } catch (error) {
    console.error("Transfer request failed:", error);
    return { success: false, message: "Unable to reach the server. Please try again." };
  }

  if (!response.ok) {
    let message = "Unable to complete the transfer.";
    try {
      const errorBody: ApiErrorBody = await response.json();
      message = friendlyErrorMessage(errorBody, message);
    } catch (error) {
      console.error("Failed to parse transfer error response:", error);
    }
    return { success: false, message };
  }

  revalidatePath("/dashboard");
  revalidatePath("/dashboard/accounts");
  revalidatePath(`/dashboard/accounts/${fromAccountId}`);
  revalidatePath(`/dashboard/accounts/${toAccountId}`);

  return { success: true, message: "Transfer complete." };
}

export interface DepositMoneyResult {
  success: boolean;
  message: string;
}

export async function depositMoneyAction(formData: FormData): Promise<DepositMoneyResult> {
  const accountId = formData.get("accountId");
  const amountRaw = formData.get("amount");
  const description = formData.get("description");

  if (typeof accountId !== "string" || !accountId) {
    return { success: false, message: "Please choose an account to deposit into." };
  }

  const amount = typeof amountRaw === "string" ? Number(amountRaw) : NaN;
  if (!Number.isFinite(amount) || amount <= 0) {
    return { success: false, message: "Enter an amount greater than $0.00." };
  }

  const cookieStore = await cookies();
  const accessToken = cookieStore.get("access_token")?.value;
  if (!accessToken) {
    return { success: false, message: "Your session has expired. Please sign in again." };
  }

  let response: Response;
  try {
    response = await fetch(`${API_BASE_URL}/api/v1/transactions/deposit`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        Authorization: `Bearer ${accessToken}`,
      },
      body: JSON.stringify({
        account_id: accountId,
        amount,
        description: typeof description === "string" && description.trim() ? description.trim() : "Deposit",
      }),
      cache: "no-store",
    });
  } catch (error) {
    console.error("Deposit request failed:", error);
    return { success: false, message: "Unable to reach the server. Please try again." };
  }

  if (!response.ok) {
    let message = "Unable to complete the deposit.";
    try {
      const errorBody: ApiErrorBody = await response.json();
      message = friendlyErrorMessage(errorBody, message);
    } catch (error) {
      console.error("Failed to parse deposit error response:", error);
    }
    return { success: false, message };
  }

  revalidatePath("/dashboard");
  revalidatePath("/dashboard/accounts");
  revalidatePath(`/dashboard/accounts/${accountId}`);

  return { success: true, message: "Deposit complete." };
}
