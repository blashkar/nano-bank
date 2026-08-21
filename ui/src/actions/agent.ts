"use server";

export interface AgentChatResult {
  success: boolean;
  reply: string;
}

/** Stub until the Agentic Branch API is wired in (see agent/README.md):
 * `POST /branch/clients/{customer_id}/message`, service-token authenticated,
 * so the real call has to happen server-side — this action is the seam. */
export async function sendAgentMessageAction(message: string): Promise<AgentChatResult> {
  if (!message.trim()) {
    return { success: false, reply: "Please enter a message." };
  }

  return {
    success: true,
    reply: "I'm not connected to the agent backend yet — check back soon!",
  };
}
