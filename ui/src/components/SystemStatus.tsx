"use client";

import { useEffect, useState } from "react";
import { checkHealthAction } from "@/actions/health";

export default function SystemStatus() {
  const [status, setStatus] = useState<"checking" | "healthy" | "unhealthy">("checking");

  useEffect(() => {
    let active = true;

    const check = async () => {
      try {
        const res = await checkHealthAction();
        if (!active) return;
        setStatus(res.success ? "healthy" : "unhealthy");
      } catch (err) {
        console.error("Failed to fetch system status:", err);
        if (!active) return;
        setStatus("unhealthy");
      }
    };

    check();
    const interval = setInterval(check, 30000); // Check every 30 seconds

    return () => {
      active = false;
      clearInterval(interval);
    };
  }, []);

  return (
    <div className="flex items-center gap-2 px-3 py-1.5 rounded-full border border-white/5 bg-slate-950/40 text-xs text-slate-400 select-none backdrop-blur-sm transition-all duration-300">
      <span className="relative flex h-2 w-2">
        {status === "checking" && (
          <span className="relative inline-flex rounded-full h-2 w-2 bg-slate-500 animate-pulse"></span>
        )}
        {status === "healthy" && (
          <>
            <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
            <span className="relative inline-flex rounded-full h-2 w-2 bg-emerald-500 shadow-[0_0_8px_rgba(16,185,129,0.6)]"></span>
          </>
        )}
        {status === "unhealthy" && (
          <>
            <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-rose-400 opacity-75"></span>
            <span className="relative inline-flex rounded-full h-2 w-2 bg-rose-500 shadow-[0_0_8px_rgba(244,63,94,0.6)]"></span>
          </>
        )}
      </span>
      <span>
        {status === "checking" && "Checking Status…"}
        {status === "healthy" && "System Healthy"}
        {status === "unhealthy" && "System Issue"}
      </span>
    </div>
  );
}
