"use client";

import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { Search } from "lucide-react";

export default function TransactionsToolbar({
  accountId,
  initialQuery,
  initialPageSize,
  defaultPageSize,
  pageSizes,
}: {
  accountId: string;
  initialQuery: string;
  initialPageSize: number;
  defaultPageSize: number;
  pageSizes: readonly number[];
}) {
  const router = useRouter();
  const [query, setQuery] = useState(initialQuery);
  const isFirstRender = useRef(true);

  const navigate = (overrides: { q?: string; pageSize?: number; page?: number }) => {
    const nextQuery = (overrides.q ?? query).trim();
    const nextPageSize = overrides.pageSize ?? initialPageSize;
    const nextPage = overrides.page ?? 1;

    const params = new URLSearchParams();
    if (nextQuery) params.set("q", nextQuery);
    if (nextPageSize !== defaultPageSize) params.set("pageSize", String(nextPageSize));
    if (nextPage !== 1) params.set("page", String(nextPage));

    const qs = params.toString();
    router.push(`/dashboard/accounts/${accountId}/transactions${qs ? `?${qs}` : ""}`);
  };

  // Debounce the search box so we don't fire a request on every keystroke.
  useEffect(() => {
    if (isFirstRender.current) {
      isFirstRender.current = false;
      return;
    }
    const handle = setTimeout(() => navigate({ q: query, page: 1 }), 350);
    return () => clearTimeout(handle);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query]);

  return (
    <div className="flex flex-col sm:flex-row sm:items-center gap-3">
      <div className="relative flex-1">
        <Search className="w-4 h-4 text-slate-500 absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none" />
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search by description..."
          aria-label="Search transactions by description"
          className="w-full pl-9 pr-3 py-2.5 rounded-lg border border-slate-700 bg-slate-900/50 text-sm text-white placeholder:text-slate-600 focus:outline-none focus:ring-2 focus:ring-nanobank-blue-sky/60"
        />
      </div>
      <div className="flex items-center gap-2 flex-shrink-0">
        <label htmlFor="pageSize" className="text-xs font-semibold text-slate-400 whitespace-nowrap">
          Per page
        </label>
        <select
          id="pageSize"
          value={initialPageSize}
          onChange={(e) => navigate({ pageSize: Number(e.target.value), page: 1 })}
          className="p-2.5 rounded-lg border border-slate-700 bg-slate-900/50 text-sm text-white focus:outline-none focus:ring-2 focus:ring-nanobank-blue-sky/60"
        >
          {pageSizes.map((size) => (
            <option key={size} value={size}>
              {size}
            </option>
          ))}
        </select>
      </div>
    </div>
  );
}
