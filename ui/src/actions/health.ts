"use server";

import { API_BASE_URL } from "@/lib/config";

export interface HealthCheckResult {
  success: boolean;
}

export async function checkHealthAction(): Promise<HealthCheckResult> {
  try {
    const response = await fetch(`${API_BASE_URL}/health`, { cache: "no-store" });
    return { success: response.ok };
  } catch (error) {
    console.error("Health check failed:", error);
    return { success: false };
  }
}
