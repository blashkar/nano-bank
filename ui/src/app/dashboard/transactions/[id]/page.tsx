import { requireSession } from "@/lib/session";
import { Metadata } from 'next';
import { ArrowDownLeft, ArrowUpRight, AlertCircle } from "lucide-react";
import { API_BASE_URL } from "@/lib/config";
import { Account } from "@/lib/accounts";
import { TransactionResponse } from "@/lib/transactions";
import BackLink from "@/components/BackLink";
import GlassCard from "@/components/GlassCard";
import GradientHeading from "@/components/GradientHeading";
import Link from "next/link";

export const metadata: Metadata = {
  title: 'Nano-Bank - Transaction Details',
};

type Props = {
    params: Promise<{ id: string }>;
    searchParams: Promise<{ account?: string }>;
};

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

export default async function TransactionDetailsPage({ params, searchParams }: Props) {
    const { accessToken } = await requireSession();
    const { id } = await params;
    const { account: backAccountId } = await searchParams;

    let transaction: TransactionResponse | null = null;
    let notFound = false;
    let fetchError = false;
    try {
        const response = await fetch(`${API_BASE_URL}/api/v1/transactions/${id}`, {
            headers: { Authorization: `Bearer ${accessToken}` },
            cache: "no-store",
        });
        if (response.ok) {
            transaction = await response.json();
        } else if (response.status === 404) {
            notFound = true;
        } else {
            console.error(`Failed to fetch transaction: ${response.status}`);
            fetchError = true;
        }
    } catch (error) {
        console.error("Failed to fetch transaction:", error);
        fetchError = true;
    }

    // Used to tell which entries belong to one of the viewer's own accounts
    // (so they can be linked back to that account) vs. a counterparty/system
    // account (e.g. EXTERNAL_CASH on a deposit) that isn't theirs to view.
    let ownedAccountIds = new Set<string>();
    if (transaction) {
        try {
            const response = await fetch(`${API_BASE_URL}/api/v1/accounts`, {
                headers: { Authorization: `Bearer ${accessToken}` },
                cache: "no-store",
            });
            if (response.ok) {
                const accounts: Account[] = await response.json();
                ownedAccountIds = new Set(accounts.map((a) => a.account_id));
            }
        } catch (error) {
            console.error("Failed to fetch accounts:", error);
        }
    }

    const backHref = backAccountId
        ? `/dashboard/accounts/${backAccountId}/transactions`
        : "/dashboard/accounts";

    return (
        <main className="relative z-10 flex-1 flex flex-col items-center justify-center px-6 py-12">
            <div className="w-full max-w-3xl">
                <BackLink href={backHref}>
                    {backAccountId ? "Back to Transactions" : "Back"}
                </BackLink>

                <GlassCard>
                    <div className="mb-8 border-b border-white/10 pb-6">
                        <GradientHeading>Transaction Details</GradientHeading>
                        <p className="text-slate-400 text-xs mt-1 font-mono">
                            Transaction ID: {id}
                        </p>
                    </div>

                    {notFound ? (
                        <div className="flex items-center gap-3 p-4 rounded-xl border border-rose-500/20 bg-rose-500/10 text-rose-300 text-sm">
                            <AlertCircle className="w-5 h-5 flex-shrink-0" />
                            <div>
                                <span className="font-semibold">Transaction not found.</span>
                            </div>
                        </div>
                    ) : fetchError || !transaction ? (
                        <div className="flex items-center gap-3 p-4 rounded-xl border border-rose-500/20 bg-rose-500/10 text-rose-300 text-sm">
                            <AlertCircle className="w-5 h-5 flex-shrink-0" />
                            <div>
                                <span className="font-semibold">Error fetching transaction details</span>
                            </div>
                        </div>
                    ) : (
                        <>
                            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-white/10">
                                <div>
                                    <div className="flex items-center gap-2">
                                        <h3 className="text-lg font-bold text-white">
                                            {transaction.description || transactionTypeLabel(transaction.transaction_type)}
                                        </h3>
                                        <span className={`text-[10px] font-semibold px-2 py-0.5 rounded-full ${statusBadgeClasses(transaction.status)}`}>
                                            {transaction.status}
                                        </span>
                                    </div>
                                    <p className="text-xs text-slate-400 mt-1">
                                        {transactionTypeLabel(transaction.transaction_type)} &middot; {formatDate(transaction.created_at)}
                                    </p>
                                    <p className="text-xs text-slate-500 mt-1 font-mono">
                                        Ref. {transaction.reference_number}
                                    </p>
                                </div>
                                <div className="text-left sm:text-right">
                                    <span className="text-slate-500 text-[10px] font-semibold uppercase tracking-wider block sm:inline">
                                        Amount
                                    </span>
                                    <p className="text-2xl font-extrabold text-white mt-0.5">
                                        {formatCurrency(parseFloat(transaction.amount))} {transaction.currency}
                                    </p>
                                </div>
                            </div>

                            <div className="grid grid-cols-2 gap-4 py-6 border-b border-white/10 text-sm">
                                <div>
                                    <span className="text-slate-500 text-[10px] font-semibold uppercase tracking-wider block">
                                        Created
                                    </span>
                                    <p className="text-slate-200 mt-1">{formatDate(transaction.created_at)}</p>
                                </div>
                                <div>
                                    <span className="text-slate-500 text-[10px] font-semibold uppercase tracking-wider block">
                                        Completed
                                    </span>
                                    <p className="text-slate-200 mt-1">
                                        {transaction.completed_at ? formatDate(transaction.completed_at) : "—"}
                                    </p>
                                </div>
                            </div>

                            <div className="pt-6">
                                <h4 className="text-sm font-bold text-white mb-3">Ledger Entries</h4>
                                <div className="space-y-3">
                                    {transaction.entries.map((entry) => {
                                        const isCredit = entry.entry_type === "Credit";
                                        const isOwned = ownedAccountIds.has(entry.account_id);
                                        const accountLabel = `${entry.account_id.slice(0, 8)}…`;

                                        return (
                                            <div
                                                key={entry.entry_id}
                                                className="flex items-center justify-between gap-4 p-4 rounded-xl border border-white/5 bg-slate-900/40"
                                            >
                                                <div className="flex items-center gap-3 min-w-0">
                                                    <div className={`p-2.5 rounded-lg flex-shrink-0 ${
                                                        isCredit
                                                            ? "bg-emerald-500/10 text-emerald-400"
                                                            : "bg-slate-500/10 text-slate-300"
                                                    }`}>
                                                        {isCredit ? (
                                                            <ArrowDownLeft className="w-4 h-4" />
                                                        ) : (
                                                            <ArrowUpRight className="w-4 h-4" />
                                                        )}
                                                    </div>
                                                    <div className="min-w-0">
                                                        <p className="text-sm font-semibold text-white">
                                                            {entry.entry_type}
                                                        </p>
                                                        {isOwned ? (
                                                            <Link
                                                                href={`/dashboard/accounts/${entry.account_id}`}
                                                                className="text-xs text-nanobank-blue-sky hover:underline font-mono"
                                                            >
                                                                {accountLabel}
                                                            </Link>
                                                        ) : (
                                                            <p className="text-xs text-slate-500 font-mono">
                                                                {accountLabel}
                                                            </p>
                                                        )}
                                                    </div>
                                                </div>
                                                <div className="text-right flex-shrink-0">
                                                    <p className={`text-sm font-bold ${isCredit ? "text-emerald-400" : "text-white"}`}>
                                                        {isCredit ? "+" : "-"}{formatCurrency(parseFloat(entry.amount))}
                                                    </p>
                                                    <p className="text-xs text-slate-500 mt-0.5">
                                                        Balance {formatCurrency(parseFloat(entry.balance_after))}
                                                    </p>
                                                </div>
                                            </div>
                                        );
                                    })}
                                </div>
                            </div>
                        </>
                    )}
                </GlassCard>
            </div>
        </main>
    );
}
