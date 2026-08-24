"use server";

import { revalidatePath } from "next/cache";
import { requireSession } from "@/lib/session";
import { API_BASE_URL } from "@/lib/config";

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
    const notes = formData.get("notes");

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

    const description = notes && String(notes).trim() ? String(notes).trim() : `Credit card payment of $${paymentAmount.toFixed(2)}`;

    const payload = {
      from_account_id: fromAccountId,
      to_card_id: toAccountId,
      amount: paymentAmount,
      description,
    };

    let response: Response;
    try {
      response = await fetch(`${API_BASE_URL}/api/v1/cards/payment`, {
        method: "POST",
        headers: {
          "content-type": "application/json",
          Authorization: `Bearer ${session.accessToken}`,
        },
        body: JSON.stringify(payload),
        cache: "no-store",
      });
    } catch (err) {
      console.error("POST card-payment request failed:", err);
      return { success: false, message: "Unable to reach the server. Please try again." };
    }

    if (!response.ok) {
      let message = "Failed to process card payment.";
      try {
        const errorBody = await response.json();
        const { friendlyErrorMessage } = await import("@/lib/errors");
        message = friendlyErrorMessage(errorBody, message);
      } catch (e) {
        console.error("Failed to parse card payment error response:", e);
      }
      return { success: false, message };
    }

    revalidatePath("/dashboard");
    revalidatePath("/dashboard/credit");
    revalidatePath("/dashboard/accounts");

    return { success: true, message: `Payment of $${paymentAmount.toFixed(2)} to your credit card was successful!` };
  } catch (error) {
    console.error("Credit card payment error:", error);
    return { success: false, message: "Failed to process credit card payment. Please try again." };
  }
}
