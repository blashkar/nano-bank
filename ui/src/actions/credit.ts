"use server";

import { cookies } from "next/headers";
import { revalidatePath } from "next/cache";
import { requireSession } from "@/lib/session";
import { getBalanceOverrides } from "@/lib/accounts";

export interface CreditCardPaymentResult {
  success: boolean;
  message: string;
}

export async function makeCreditCardPaymentAction(formData: FormData): Promise<CreditCardPaymentResult> {
  try {
    const session = await requireSession();
    if (!session) {
      return { success: false, message: "Session expired. Please sign in again." };
    }

    const fromAccountId = formData.get("fromAccountId");
    const toAccountId = formData.get("toAccountId");
    const amountType = formData.get("amountType"); // "statement" | "current" | "custom"
    const customAmount = formData.get("customAmount");

    if (typeof fromAccountId !== "string" || !fromAccountId) {
      return { success: false, message: "Please select a funding account." };
    }
    if (typeof toAccountId !== "string" || !toAccountId) {
      return { success: false, message: "Please select a credit card." };
    }

    let paymentAmount = 0;
    if (amountType === "statement") {
      paymentAmount = Number(formData.get("statementAmountValue") || 0);
    } else if (amountType === "current") {
      paymentAmount = Number(formData.get("currentAmountValue") || 0);
    } else if (amountType === "custom") {
      paymentAmount = Number(customAmount);
    } else {
      return { success: false, message: "Please select a payment amount option." };
    }

    if (!Number.isFinite(paymentAmount) || paymentAmount <= 0) {
      return { success: false, message: "Please enter a valid payment amount greater than $0.00." };
    }

    // Get current balances from backend or overrides
    const { API_BASE_URL } = await import("@/lib/config");
    const fromResponse = await fetch(`${API_BASE_URL}/api/v1/accounts/${fromAccountId}`, {
      headers: { Authorization: `Bearer ${session.accessToken}` },
      cache: "no-store",
    });
    const toResponse = await fetch(`${API_BASE_URL}/api/v1/accounts/${toAccountId}`, {
      headers: { Authorization: `Bearer ${session.accessToken}` },
      cache: "no-store",
    });

    if (!fromResponse.ok || !toResponse.ok) {
      return { success: false, message: "Failed to fetch account balances for verification." };
    }

    const fromAccount = await fromResponse.json();
    const toCard = await toResponse.json();

    const overrides = await getBalanceOverrides();
    const fromBalance = overrides[fromAccountId] !== undefined ? overrides[fromAccountId] : parseFloat(fromAccount.balance);
    const cardOwedBalance = overrides[toAccountId] !== undefined ? overrides[toAccountId] : parseFloat(toCard.balance);

    if (fromBalance < paymentAmount) {
      return { success: false, message: `Insufficient funds. Your selected account only has $${fromBalance.toFixed(2)}.` };
    }

    // Deduct from deposit account, and decrease credit card owed balance (balance)
    const newFromBalance = fromBalance - paymentAmount;
    const newCardBalance = Math.max(0, cardOwedBalance - paymentAmount);

    overrides[fromAccountId] = newFromBalance;
    overrides[toAccountId] = newCardBalance;

    const cookieStore = await cookies();
    cookieStore.set("balance_overrides", JSON.stringify(overrides), {
      httpOnly: true,
      secure: process.env.NODE_ENV === "production",
      sameSite: "lax",
      path: "/",
      maxAge: 60 * 60 * 24 * 30, // 30 days
    });

    revalidatePath("/dashboard");
    revalidatePath("/dashboard/credit");
    revalidatePath("/dashboard/accounts");

    return { success: true, message: `Payment of $${paymentAmount.toFixed(2)} to your credit card was successful!` };
  } catch (error) {
    console.error("Credit card payment error:", error);
    return { success: false, message: "Failed to process credit card payment. Please try again." };
  }
}
