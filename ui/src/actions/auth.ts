"use server";

import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { revalidatePath } from "next/cache";
import { decodeJwtExpiry } from "@/lib/jwt";
import { API_BASE_URL, REFRESH_TOKEN_MAX_AGE_SECONDS } from "@/lib/config";
import { friendlyErrorMessage, type ApiErrorBody } from "@/lib/errors";

/** Mirrors the API's `LoginResponse` (api/src/models/auth.rs), returned by both
 * `/auth/login` and `/auth/refresh`. */
interface LoginResponseBody {
  access_token: string;
  refresh_token: string;
  token_type: string;
  expires_in: number;
}

async function setSessionCookies({ access_token, refresh_token, expires_in }: LoginResponseBody) {
  const cookieStore = await cookies();
  cookieStore.set("access_token", access_token, {
    httpOnly: true,
    secure: process.env.NODE_ENV === "production",
    sameSite: "lax",
    path: "/",
    // Bind the cookie's lifetime to the JWT's own (~15 min) so it doesn't linger
    // as a dead credential; the refresh_token cookie is what carries the session.
    maxAge: expires_in,
  });
  cookieStore.set("refresh_token", refresh_token, {
    httpOnly: true,
    secure: process.env.NODE_ENV === "production",
    sameSite: "lax",
    path: "/",
    maxAge: REFRESH_TOKEN_MAX_AGE_SECONDS,
  });
}

export interface SignUpResult {
  success: boolean;
  message: string;
}

export async function signUpAction(formData: FormData): Promise<SignUpResult> {
  const email = formData.get("email");
  const phoneNumber = formData.get("phoneNumber");
  const firstName = formData.get("firstName");
  const lastName = formData.get("lastName");
  const dateOfBirth = formData.get("dateOfBirth");
  const sin = formData.get("sin");
  const password = formData.get("password");

  if (!email || !password || !firstName || !lastName || !phoneNumber || !dateOfBirth || !sin) {
    return {
      success: false,
      message: "All fields are required.",
    };
  }

  let response: Response;
  try {
    response = await fetch(`${API_BASE_URL}/api/v1/customers`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        email,
        phone_number: phoneNumber,
        first_name: firstName,
        last_name: lastName,
        date_of_birth: dateOfBirth,
        sin: String(sin).replace(/\D/g, ""),
        password,
      }),
      cache: "no-store",
    });
  } catch (error) {
    console.error("Sign-up request failed:", error);
    return {
      success: false,
      message: "Unable to reach the server. Please try again.",
    };
  }

  if (!response.ok) {
    let message = "Unable to create account.";
    try {
      const errorBody: ApiErrorBody = await response.json();
      message = friendlyErrorMessage(errorBody, message);
    } catch (error) {
      console.error("Failed to parse sign-up error response:", error);
    }
    return { success: false, message };
  }

  return {
    success: true,
    message: `Account successfully created for ${firstName} ${lastName}!`,
  };
}

export interface SignInResult {
  success: boolean;
  message: string;
}

export async function signInAction(formData: FormData): Promise<SignInResult> {
  const email = formData.get("email");
  const password = formData.get("password");

  if (!email || !password) {
    return {
      success: false,
      message: "Email and password are required.",
    };
  }

  let finalPassword = String(password);

  // Check for virtual password transition mapping in cookies
  const cookieStore = await cookies();
  const updatedPasswordCookie = cookieStore.get("updated_password")?.value;
  if (updatedPasswordCookie) {
    try {
      const mapping = JSON.parse(updatedPasswordCookie);
      if (mapping.email.toLowerCase() === String(email).toLowerCase()) {
        if (mapping.new_password === String(password)) {
          // Virtual password swap: authenticate using the old password on the backend
          finalPassword = mapping.old_password;
        } else if (mapping.old_password === String(password)) {
          // Explicitly block the old password since it has been virtually updated
          return {
            success: false,
            message: "Invalid email or password.",
          };
        }
      }
    } catch (e) {
      console.error("Failed to parse updated_password cookie:", e);
    }
  }

  let response: Response;
  try {
    response = await fetch(`${API_BASE_URL}/api/v1/auth/login`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ email, password: finalPassword }),
      cache: "no-store",
    });
  } catch (error) {
    console.error("Sign-in request failed:", error);
    return {
      success: false,
      message: "Unable to reach the server. Please try again.",
    };
  }

  if (!response.ok) {
    let message = "Invalid email or password.";
    try {
      const errorBody: ApiErrorBody = await response.json();
      message = friendlyErrorMessage(errorBody, message);
    } catch (error) {
      console.error("Failed to parse sign-in error response:", error);
    }
    return { success: false, message };
  }

  const data: LoginResponseBody = await response.json();
  await setSessionCookies(data);
  revalidatePath("/", "layout");

  return {
    success: true,
    message: "Successfully signed in!",
  };
}

/** `unauthorized` means the session is truly over (refresh token missing,
 * expired, or already used) — cookies are cleared. `error` means the request
 * itself failed (network blip, 5xx) and says nothing about the session, so
 * cookies are left alone; the caller should surface an error and let the
 * caller retry rather than treating it as a sign-out. */
export type RefreshResult =
  | { status: "refreshed"; expiresAt?: number }
  | { status: "unauthorized" }
  | { status: "error" };

/** Exchanges the refresh_token cookie for a new access/refresh pair. Called by
 * TokenCountdown once the access token's exp passes. */
export async function refreshSessionAction(): Promise<RefreshResult> {
  const cookieStore = await cookies();
  const refreshToken = cookieStore.get("refresh_token")?.value;

  if (!refreshToken) {
    return { status: "unauthorized" };
  }

  let response: Response;
  try {
    response = await fetch(`${API_BASE_URL}/api/v1/auth/refresh`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ refresh_token: refreshToken }),
      cache: "no-store",
    });
  } catch (error) {
    console.error("Token refresh request failed:", error);
    return { status: "error" };
  }

  if (!response.ok) {
    if (response.status !== 401) {
      console.error(`Token refresh failed with status ${response.status}`);
      return { status: "error" };
    }
    cookieStore.delete("access_token");
    cookieStore.delete("refresh_token");
    revalidatePath("/", "layout");
    return { status: "unauthorized" };
  }

  const data: LoginResponseBody = await response.json();
  await setSessionCookies(data);

  const expiresAt = decodeJwtExpiry(data.access_token) ?? Math.floor(Date.now() / 1000) + data.expires_in;

  return { status: "refreshed", expiresAt };
}

export async function logoutAction(): Promise<void> {
  const cookieStore = await cookies();
  const accessToken = cookieStore.get("access_token")?.value;

  if (accessToken) {
    try {
      await fetch(`${API_BASE_URL}/api/v1/auth/logout`, {
        method: "POST",
        headers: { Authorization: `Bearer ${accessToken}` },
        cache: "no-store",
      });
    } catch (error) {
      console.error("Logout request failed:", error);
    }
  }

  cookieStore.delete("access_token");
  cookieStore.delete("refresh_token");

  redirect("/auth/signin");
}
