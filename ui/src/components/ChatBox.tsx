"use client";

import { useEffect, useRef, useState } from "react";
import { Bot, Send, User } from "lucide-react";
import { toast } from "sonner";
import { sendAgentMessageAction } from "@/actions/agent";

interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
}

export default function ChatBox() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: "smooth" });
  }, [messages, loading]);

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const text = input.trim();
    if (!text || loading) return;

    setMessages((prev) => [...prev, { id: crypto.randomUUID(), role: "user", content: text }]);
    setInput("");
    setLoading(true);

    try {
      const result = await sendAgentMessageAction(text);
      setMessages((prev) => [
        ...prev,
        { id: crypto.randomUUID(), role: "assistant", content: result.reply },
      ]);
    } catch (error) {
      console.error("Agent chat request failed:", error);
      toast.error("Unable to reach the assistant. Please try again.");
    }
    setLoading(false);
  };

  return (
    <div className="flex flex-col h-[420px]">
      <div className="flex items-center gap-3 pb-4 mb-4 border-b border-white/5">
        <div className="p-2 rounded-lg bg-nanobank-blue-sky/10 text-nanobank-blue-sky">
          <Bot className="w-5 h-5" />
        </div>
        <div>
          <h3 className="text-sm font-semibold text-white">Nano-Bank Assistant</h3>
          <p className="text-[11px] text-slate-400">AI agent — not yet connected</p>
        </div>
      </div>

      <div ref={scrollRef} className="flex-1 overflow-y-auto space-y-3 pr-1">
        {messages.length === 0 ? (
          <div className="h-full flex items-center justify-center text-center text-sm text-slate-500 px-6">
            Ask about your balances, transactions, or send money once I&apos;m connected.
          </div>
        ) : (
          messages.map((message) => (
            <div
              key={message.id}
              className={`flex items-end gap-2 ${message.role === "user" ? "justify-end" : "justify-start"}`}
            >
              {message.role === "assistant" && (
                <div className="p-1.5 rounded-full bg-nanobank-blue-sky/10 text-nanobank-blue-sky flex-shrink-0">
                  <Bot className="w-3.5 h-3.5" />
                </div>
              )}
              <div
                className={`max-w-[75%] rounded-xl px-3.5 py-2 text-sm ${
                  message.role === "user"
                    ? "bg-gradient-to-r from-nanobank-blue-sky to-nanobank-blue-green text-nanobank-blue-deep font-medium"
                    : "bg-slate-900/50 border border-white/5 text-slate-200"
                }`}
              >
                {message.content}
              </div>
              {message.role === "user" && (
                <div className="p-1.5 rounded-full bg-slate-800 text-slate-400 flex-shrink-0">
                  <User className="w-3.5 h-3.5" />
                </div>
              )}
            </div>
          ))
        )}
        {loading && (
          <div className="flex items-end gap-2 justify-start">
            <div className="p-1.5 rounded-full bg-nanobank-blue-sky/10 text-nanobank-blue-sky flex-shrink-0">
              <Bot className="w-3.5 h-3.5" />
            </div>
            <div className="rounded-xl px-3.5 py-2.5 bg-slate-900/50 border border-white/5 flex gap-1">
              <span className="w-1.5 h-1.5 rounded-full bg-slate-500 animate-bounce [animation-delay:-0.3s]" />
              <span className="w-1.5 h-1.5 rounded-full bg-slate-500 animate-bounce [animation-delay:-0.15s]" />
              <span className="w-1.5 h-1.5 rounded-full bg-slate-500 animate-bounce" />
            </div>
          </div>
        )}
      </div>

      <form onSubmit={handleSubmit} className="flex items-center gap-2 pt-4 mt-2 border-t border-white/5">
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="Ask your assistant..."
          maxLength={1000}
          disabled={loading}
          className="flex-1 p-2.5 rounded-lg border border-slate-700 bg-slate-900/50 text-sm text-white placeholder:text-slate-600 focus:outline-none focus:ring-2 focus:ring-nanobank-blue-sky/60 disabled:opacity-50"
        />
        <button
          type="submit"
          disabled={loading || !input.trim()}
          className="p-2.5 rounded-lg text-nanobank-blue-deep bg-gradient-to-r from-nanobank-blue-sky to-nanobank-blue-green hover:brightness-110 transition-all disabled:opacity-40 disabled:cursor-not-allowed flex-shrink-0"
          aria-label="Send message"
        >
          <Send className="w-4 h-4" />
        </button>
      </form>
    </div>
  );
}
