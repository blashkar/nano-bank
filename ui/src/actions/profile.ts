"use server";

import { revalidatePath } from "next/cache";
import { requireSession } from "@/lib/session";
import { API_BASE_URL } from "@/lib/config";

export interface UpdateSettingsResult {
  success: boolean;
  message: string;
}

export async function updateSettingsAction(formData: FormData): Promise<UpdateSettingsResult> {
  try {
    const session = await requireSession();
    if (!session) {
      return { success: false, message: "Session expired. Please sign in again." };
    }

    const firstName = formData.get("firstName");
    const lastName = formData.get("lastName");
    const email = formData.get("email");
    const phoneNumber = formData.get("phoneNumber");
    const dateOfBirth = formData.get("dateOfBirth");
    const sin = formData.get("sin");

    const currentPassword = formData.get("currentPassword");
    const newPassword = formData.get("newPassword");
    const confirmPassword = formData.get("confirmPassword");

    // Simple validation
    if (!firstName || !lastName || !email || !phoneNumber) {
      return { success: false, message: "Required fields cannot be empty." };
    }

    // Password validation & verification
    if (newPassword || confirmPassword || currentPassword) {
      if (!currentPassword) {
        return { success: false, message: "Current password is required to change settings." };
      }
      if (newPassword || confirmPassword) {
        if (newPassword !== confirmPassword) {
          return { success: false, message: "New passwords do not match." };
        }
        if (String(newPassword).length < 8) {
          return { success: false, message: "New password must be at least 8 characters." };
        }
        if (String(newPassword) === String(currentPassword)) {
          return { success: false, message: "New password cannot be the same as your current password." };
        }
      }

      // Verify current password against actual backend credentials by dry-running a sign-in check
      try {
        const authResponse = await fetch(`${API_BASE_URL}/api/v1/auth/login`, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ email: session.profile.email, password: String(currentPassword) }),
          cache: "no-store",
        });

        if (!authResponse.ok) {
          return { success: false, message: "The current password you entered is incorrect." };
        }
      } catch (err) {
        console.error("Password verification failed:", err);
        return { success: false, message: "Failed to verify current password. Please try again." };
      }
    }

    const payload = {
      first_name: String(firstName),
      last_name: String(lastName),
      email: String(email),
      phone_number: String(phoneNumber),
      date_of_birth: String(dateOfBirth),
      sin: sin && String(sin).trim() ? String(sin).trim() : null,
      new_password: newPassword ? String(newPassword) : null,
    };

    let response: Response;
    try {
      response = await fetch(`${API_BASE_URL}/api/v1/customers/profile`, {
        method: "PUT",
        headers: {
          "content-type": "application/json",
          Authorization: `Bearer ${session.accessToken}`,
        },
        body: JSON.stringify(payload),
        cache: "no-store",
      });
    } catch (err) {
      console.error("PUT profile request failed:", err);
      return { success: false, message: "Unable to reach the server. Please try again." };
    }

    if (!response.ok) {
      let message = "Failed to update profile settings.";
      try {
        const errorBody = await response.json();
        const { friendlyErrorMessage } = await import("@/lib/errors");
        message = friendlyErrorMessage(errorBody, message);
      } catch (e) {
        console.error("Failed to parse PUT profile error response:", e);
      }
      return { success: false, message };
    }

    revalidatePath("/", "layout");
    revalidatePath("/dashboard");
    revalidatePath("/dashboard/settings");

    return { success: true, message: "Profile settings updated successfully!" };
  } catch (error) {
    console.error("Failed to update settings:", error);
    return { success: false, message: "Failed to update settings. Please try again." };
  }
}
