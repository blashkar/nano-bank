import { requireSession } from "@/lib/session";
import { Metadata } from "next";
import { AlertCircle, ChevronLeft, ChevronRight } from "lucide-react";
import { API_BASE_URL } from "@/lib/config";
import { Account } from "@/lib/accounts";
import { TransactionResponse, TransactionHistoryResponse } from "@/lib/transactions";
import BackLink from "@/components/BackLink";
import GlassCard from "@/components/GlassCard";
import GradientHeading from "@/components/GradientHeading";
import Link from "next/link";
import TransactionsToolbar from "./TransactionsToolbar";

export const metadata: Metadata = {
  title: "Nano-Bank - Account Transactions",
};

type Props = {
  params: Promise<{ id: string }>;
  searchParams: Promise<{ page?: string; pageSize?: string; q?: string }>;
};

const PAGE_SIZES = [10, 25, 50, 100] as const;
const DEFAULT_PAGE_SIZE = 25;

const formatCurrency = (val: number) => {
  return new Intl.NumberFormat("en-CA", {
    style: "currency",
    currency: "CAD",
  }).format(val);
};

const formatDate = (iso: string) => {
  return new Intl.DateTimeFormat("en-CA", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(iso));
};

const transactionTypeLabel = (type: string) => {
  return type
    .split("_")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
};

const statusBadgeClasses = (status: string) => {
  switch (status) {
    case "Completed":
      return "bg-emerald-500/10 text-emerald-400 border border-emerald-500/20";
    case "Pending":
      return "bg-nanobank-blue-sky/10 text-nanobank-blue-sky border border-nanobank-blue-sky/20";
    case "Failed":
    case "Cancelled":
      return "bg-rose-500/10 text-rose-400 border border-rose-500/20";
    default:
      return "bg-slate-500/10 text-slate-400 border border-slate-500/20";
  }
};

const parsePageSize = (raw?: string): number => {
  const n = Number(raw);
  return (PAGE_SIZES as readonly number[]).includes(n) ? n : DEFAULT_PAGE_SIZE;
};

const parsePage = (raw?: string): number => {
  const n = Number(raw);
  return Number.isInteger(n) && n > 0 ? n : 1;
};

export default async function AccountTransactionsPage({ params, searchParams }: Props) {
  const { accessToken } = await requireSession();
  const { id } = await params;
  const { page: pageRaw, pageSize: pageSizeRaw, q: qRaw } = await searchParams;

  const pageSize = parsePageSize(pageSizeRaw);
  const page = parsePage(pageRaw);
  const q = (qRaw ?? "").trim();
  const offset = (page - 1) * pageSize;

  let account: Account | null = null;
  try {
    const response = await fetch(`${API_BASE_URL}/api/v1/accounts/${id}`, {
      headers: { Authorization: `Bearer ${accessToken}` },
      cache: "no-store",
    });
    if (response.ok) {
      account = await response.json();
    } else {
      console.error(`Failed to fetch account: ${response.status}`);
    }
  } catch (error) {
    console.error("Failed to fetch account:", error);
  }

  let transactions: TransactionResponse[] = [];
  let totalCount = 0;
  let transactionsError = false;
  try {
    const url = new URL(`${API_BASE_URL}/api/v1/transactions`);
    url.searchParams.set("account_id", id);
    url.searchParams.set("limit", String(pageSize));
    url.searchParams.set("offset", String(offset));
    if (q) url.searchParams.set("description", q);

    const response = await fetch(url, {
      headers: { Authorization: `Bearer ${accessToken}` },
      cache: "no-store",
    });
    if (response.ok) {
      const body: TransactionHistoryResponse = await response.json();
      transactions = body.transactions;
      totalCount = body.total_count;
    } else {
      console.error(`Failed to fetch transactions: ${response.status}`);
      transactionsError = true;
    }
  } catch (error) {
    console.error("Failed to fetch transactions:", error);
    transactionsError = true;
  }

  const totalPages = Math.max(1, Math.ceil(totalCount / pageSize));
  const rangeStart = offset + 1;
  const rangeEnd = offset + transactions.length;

  const buildHref = (targetPage: number) => {
    const params = new URLSearchParams();
    if (q) params.set("q", q);
    if (pageSize !== DEFAULT_PAGE_SIZE) params.set("pageSize", String(pageSize));
    if (targetPage !== 1) params.set("page", String(targetPage));
    const qs = params.toString();
    return `/dashboard/accounts/${id}/transactions${qs ? `?${qs}` : ""}`;
  };

  return (
    <main className="relative z-10 flex-1 flex flex-col items-center justify-center px-6 py-12">
      <div className="w-full max-w-5xl">
        <BackLink href={`/dashboard/accounts/${id}`}>Back to Account</BackLink>

        <GlassCard>
          <div className="mb-6 border-b border-white/10 pb-6 flex flex-col gap-5">
            <div>
              <GradientHeading>Transactions</GradientHeading>
              {account && (
                <p className="text-slate-400 text-xs mt-1 font-mono capitalize">
                  {account.account_type.replace("_", " ")} •••• {account.account_number.slice(-4)}
                </p>
              )}
            </div>
            <TransactionsToolbar
              accountId={id}
              initialQuery={q}
              initialPageSize={pageSize}
              defaultPageSize={DEFAULT_PAGE_SIZE}
              pageSizes={PAGE_SIZES}
            />
          </div>

          {transactionsError ? (
            <div className="flex items-center gap-3 p-4 rounded-xl border border-rose-500/20 bg-rose-500/10 text-rose-300 text-sm">
              <AlertCircle className="w-5 h-5 flex-shrink-0" />
              <div>
                <span className="font-semibold">Error fetching transactions</span>
              </div>
            </div>
          ) : (
            <>
              {transactions.length > 0 ? (
                <div className="overflow-x-auto -mx-2">
                  <table className="w-full text-sm border-collapse">
                    <thead>
                      <tr className="text-left text-[10px] uppercase tracking-wider text-slate-500 border-b border-white/10">
                        <th className="px-2 py-3 font-semibold whitespace-nowrap">Date</th>
                        <th className="px-2 py-3 font-semibold">Description</th>
                        <th className="px-2 py-3 font-semibold whitespace-nowrap">Type</th>
                        <th className="px-2 py-3 font-semibold whitespace-nowrap">Status</th>
                        <th className="px-2 py-3 font-semibold text-right whitespace-nowrap">Amount</th>
                      </tr>
                    </thead>
                    <tbody>
                      {transactions.map((txn) => {
                        const myEntry = txn.entries.find((e) => e.account_id === id);
                        const isCredit = myEntry?.entry_type === "Credit";
                        const signedAmount = (isCredit ? 1 : -1) * parseFloat(txn.amount);

                        return (
                          <tr
                            key={txn.transaction_id}
                            className="border-b border-white/5 hover:bg-white/5 transition-colors"
                          >
                            <td className="px-2 py-3 text-slate-400 whitespace-nowrap align-top">
                              {formatDate(txn.created_at)}
                            </td>
                            <td className="px-2 py-3 align-top">
                              <Link
                                href={`/dashboard/transactions/${txn.transaction_id}?account=${id}`}
                                className="text-white font-medium hover:underline hover:text-nanobank-blue-sky transition-colors"
                              >
                                {txn.description || transactionTypeLabel(txn.transaction_type)}
                              </Link>
                            </td>
                            <td className="px-2 py-3 text-slate-400 whitespace-nowrap align-top">
                              {transactionTypeLabel(txn.transaction_type)}
                            </td>
                            <td className="px-2 py-3 align-top">
                              <span
                                className={`text-[10px] font-semibold px-2 py-0.5 rounded-full whitespace-nowrap ${statusBadgeClasses(txn.status)}`}
                              >
                                {txn.status}
                              </span>
                            </td>
                            <td
                              className={`px-2 py-3 text-right font-bold whitespace-nowrap align-top ${isCredit ? "text-emerald-400" : "text-white"}`}
                            >
                              {isCredit ? "+" : "-"}
                              {formatCurrency(Math.abs(signedAmount))}
                            </td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              ) : (
                <div className="rounded-xl border border-dashed border-slate-700 bg-slate-900/30 p-8 text-center text-sm text-slate-400">
                  {totalCount > 0
                    ? "This page has no transactions."
                    : q
                      ? `No transactions match “${q}”.`
                      : "No transactions on this account yet."}
                </div>
              )}

              {totalCount > 0 && (
                <div className="flex flex-col sm:flex-row items-center justify-between gap-3 mt-6 pt-4 border-t border-white/10 text-xs text-slate-400">
                  <p>
                    {transactions.length > 0
                      ? `Showing ${rangeStart}–${rangeEnd} of ${totalCount}`
                      : `${totalCount} total`}
                  </p>
                  <div className="flex items-center gap-2">
                    <Link
                      href={buildHref(page - 1)}
                      aria-disabled={page <= 1}
                      tabIndex={page <= 1 ? -1 : undefined}
                      className={`inline-flex items-center gap-1 px-3 py-1.5 rounded-lg border border-white/10 bg-white/5 font-semibold transition-all ${
                        page <= 1 ? "opacity-40 pointer-events-none" : "hover:bg-white/10 text-slate-200"
                      }`}
                    >
                      <ChevronLeft className="w-3.5 h-3.5" />
                      Previous
                    </Link>
                    <span className="px-2 font-semibold text-slate-300 whitespace-nowrap">
                      Page {page} of {totalPages}
                    </span>
                    <Link
                      href={buildHref(page + 1)}
                      aria-disabled={page >= totalPages}
                      tabIndex={page >= totalPages ? -1 : undefined}
                      className={`inline-flex items-center gap-1 px-3 py-1.5 rounded-lg border border-white/10 bg-white/5 font-semibold transition-all ${
                        page >= totalPages ? "opacity-40 pointer-events-none" : "hover:bg-white/10 text-slate-200"
                      }`}
                    >
                      Next
                      <ChevronRight className="w-3.5 h-3.5" />
                    </Link>
                  </div>
                </div>
              )}
            </>
          )}
        </GlassCard>
      </div>
    </main>
  );
}
